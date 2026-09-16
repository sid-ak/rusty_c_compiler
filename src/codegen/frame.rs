//! Stack frame layout: where every value a function needs lives, and the code that sets it up.
//!
//! The strategy is stack spilling ([ADR 0005](../../docs/decisions/0005-stack-spilling-instead-of-register-allocation.md)):
//! every local, every parameter, and every intermediate value gets a fixed place in the frame, and
//! a value is in a register only for the instant it is being used. That is slower than deciding
//! which values deserve registers, and it removes the entire class of bug where two live values are
//! given the same one.
//!
//! The frame is built downward from the caller's stack pointer and addressed upward from `x29`:
//!
//! ```text
//!   higher addresses
//!   ...                        the caller's frame
//!   [x29, #N)                  temporaries, one per expression nesting level
//!   ...                        locals and spilled parameters
//!   [x29, #16)                 first slot
//!   [x29, #8]                  saved x30, the return address
//!   [x29, #0]   <- x29, sp     saved x29, the caller's frame pointer
//!   lower addresses
//! ```
//!
//! `x29` sits at the bottom of the frame and every offset is positive, whatever the frame's size.
//! That uniformity is deliberate: the obvious alternative for large frames leaves `x29` near the
//! top and everything at negative offsets, and a compiler that used one convention for small frames
//! and the other for large ones would read a slot from the wrong side of the frame pointer in
//! exactly the programs least likely to be tested.

use std::collections::BTreeMap;

use crate::ast::{Block, Expr, ExprKind, ForInit, Initializer, Stmt, StmtKind};
use crate::codegen::emit::{Emitter, Width, SCRATCH};
use crate::sema::annotations::{Frame, FrameSlot};
use crate::sema::scope::{SlotId, SymbolKind};

/// Bytes at the bottom of every frame holding the saved `x29` and `x30`.
pub const SAVED_REGISTERS: u64 = 16;

/// Bytes reserved for one expression temporary.
///
/// Eight rather than four, so a temporary can hold a pointer as readily as an `int`.
pub const TEMPORARY_BYTES: u64 = 8;

/// The alignment AAPCS64 requires of the stack pointer.
const STACK_ALIGNMENT: u64 = 16;

/// The largest frame the pre-indexed `stp` can open, confirmed against the assembler.
const PRE_INDEXED_LIMIT: u64 = 512;

/// The registers the first eight integer or pointer arguments arrive in.
const ARGUMENT_REGISTERS: usize = 8;

/// Where everything in one function's frame lives.
#[derive(Debug, Clone)]
pub struct FrameLayout {
    /// The frame's total size, a multiple of sixteen.
    size: u64,
    /// The `x29`-relative offset of each named slot.
    offsets: BTreeMap<SlotId, u64>,
    /// The `x29`-relative offset of each expression temporary, by nesting depth.
    temporaries: Vec<u64>,
}

impl FrameLayout {
    /// Lays out `frame`, with room for `temporaries` nested expression results.
    ///
    /// Slots are placed in declaration order. Any order would work; this one is deterministic and
    /// reads the way the source does, which is what makes an emitted frame comparable against the
    /// function that produced it.
    pub fn build(frame: &Frame, temporaries: u32) -> Self {
        let mut cursor = SAVED_REGISTERS;
        let mut offsets = BTreeMap::new();

        for slot in &frame.slots {
            cursor = align_to(cursor, slot.align.max(1));
            offsets.insert(slot.slot, cursor);
            cursor = cursor.saturating_add(slot.size);
        }

        let mut temporary_offsets = Vec::new();
        for _ in 0..temporaries {
            cursor = align_to(cursor, TEMPORARY_BYTES);
            temporary_offsets.push(cursor);
            cursor = cursor.saturating_add(TEMPORARY_BYTES);
        }

        Self {
            size: align_to(cursor, STACK_ALIGNMENT),
            offsets,
            temporaries: temporary_offsets,
        }
    }

    /// The frame's total size in bytes, always a multiple of sixteen.
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Where `slot` lives, as an offset from `x29`.
    pub fn offset_of(&self, slot: SlotId) -> Option<u64> {
        self.offsets.get(&slot).copied()
    }

    /// Where the temporary for expression nesting `depth` lives, as an offset from `x29`.
    pub fn temporary(&self, depth: u32) -> Option<u64> {
        let index = usize::try_from(depth).ok()?;

        self.temporaries.get(index).copied()
    }

    /// Opens the frame and copies the parameters in `slots` out of their argument registers.
    ///
    /// Parameters arrive in registers and are stored immediately, so the body of the function can
    /// treat a parameter and a local as the same thing: a place in the frame.
    pub fn emit_prologue(&self, emitter: &mut Emitter, slots: &[FrameSlot]) {
        if self.size <= PRE_INDEXED_LIMIT {
            emitter.instruction(&format!("stp x29, x30, [sp, #-{}]!", self.size));
        } else {
            // The pre-indexed immediate cannot reach this far, so the stack pointer is lowered on
            // its own first and the pair stored where it lands.
            emitter.load_immediate(SCRATCH, self.size);
            emitter.instruction(&format!("sub sp, sp, {SCRATCH}"));
            emitter.instruction("stp x29, x30, [sp]");
        }
        emitter.instruction("mov x29, sp");

        for slot in slots {
            let SymbolKind::Parameter(index) = slot.kind else {
                continue;
            };
            let Ok(position) = usize::try_from(index) else {
                continue;
            };
            let Some(offset) = self.offset_of(slot.slot) else {
                continue;
            };

            // Arguments past the eighth arrived on the stack rather than in a register; placing
            // them is the calling convention's business, not the frame's.
            if position >= ARGUMENT_REGISTERS {
                continue;
            }

            let width = width_for(slot.size);
            emitter.store_to_frame(&format!("w{position}"), width, offset);
        }
    }

    /// Closes the frame and returns.
    ///
    /// One epilogue per function, which every `return` branches to, so the sequence that undoes the
    /// prologue is written once and cannot disagree with itself.
    pub fn emit_epilogue(&self, emitter: &mut Emitter) {
        emitter.instruction("mov sp, x29");

        if self.size <= PRE_INDEXED_LIMIT {
            emitter.instruction(&format!("ldp x29, x30, [sp], #{}", self.size));
        } else {
            emitter.instruction("ldp x29, x30, [sp]");
            emitter.load_immediate(SCRATCH, self.size);
            emitter.instruction(&format!("add sp, sp, {SCRATCH}"));
        }

        emitter.instruction("ret");
    }
}

/// How many expression temporaries the body of a function needs.
///
/// One per level of expression nesting, because the six-step shape a binary operation lowers to
/// spills its left operand while the right one is evaluated — and the right operand may be a binary
/// operation that does the same thing. Sizing from the deepest expression in the function is what
/// keeps an inner spill from landing on an outer one.
///
/// The count is the depth of the deepest expression, not the number of expressions: two operations
/// side by side reuse a slot safely, since the first has finished with it before the second starts.
pub fn temporaries_needed(body: &Block) -> u32 {
    block_depth(body)
}

/// The deepest expression anywhere in `block`.
fn block_depth(block: &Block) -> u32 {
    block.stmts.iter().map(statement_depth).max().unwrap_or(0)
}

/// The deepest expression anywhere in `stmt`.
fn statement_depth(stmt: &Stmt) -> u32 {
    match &stmt.kind {
        StmtKind::Block(block) => block_depth(block),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => expression_depth(condition)
            .max(statement_depth(then_branch))
            .max(else_branch.as_deref().map_or(0, statement_depth)),
        StmtKind::While { condition, body } => {
            expression_depth(condition).max(statement_depth(body))
        }
        StmtKind::For {
            init,
            condition,
            step,
            body,
        } => {
            let init = match init.as_deref() {
                Some(ForInit::Decl(decl)) => decl.init.as_ref().map_or(0, initializer_depth),
                Some(ForInit::Expr(expr)) => expression_depth(expr),
                None => 0,
            };

            init.max(condition.as_ref().map_or(0, expression_depth))
                .max(step.as_ref().map_or(0, expression_depth))
                .max(statement_depth(body))
        }
        StmtKind::Return(Some(expr)) | StmtKind::Expr(expr) => expression_depth(expr),
        StmtKind::LocalVar(decl) => decl.init.as_ref().map_or(0, initializer_depth),
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Empty => 0,
    }
}

/// The deepest expression in `init`.
fn initializer_depth(init: &Initializer) -> u32 {
    match init {
        Initializer::Expr(expr) => expression_depth(expr),
        Initializer::List { elements, .. } => {
            elements.iter().map(expression_depth).max().unwrap_or(0)
        }
    }
}

/// How deeply `expr` nests, counting itself as one level.
fn expression_depth(expr: &Expr) -> u32 {
    let below = match &expr.kind {
        ExprKind::IntLit(_) | ExprKind::CharLit(_) | ExprKind::StrLit(_) | ExprKind::Ident(_) => 0,
        ExprKind::Unary { operand, .. } | ExprKind::PostfixIncDec { operand, .. } => {
            expression_depth(operand)
        }
        ExprKind::Binary { left, right, .. } => expression_depth(left).max(expression_depth(right)),
        ExprKind::Assign { target, value } => expression_depth(target).max(expression_depth(value)),
        ExprKind::Index { base, index } => expression_depth(base).max(expression_depth(index)),
        ExprKind::Call { callee, args } => {
            expression_depth(callee).max(args.iter().map(expression_depth).max().unwrap_or(0))
        }
    };

    below.saturating_add(1)
}

/// The access width for a slot of `size` bytes.
///
/// A `char` is the only thing narrower than a word; anything wider than a word is an array, which
/// is addressed element by element rather than loaded whole.
fn width_for(size: u64) -> Width {
    if size == 1 {
        Width::Byte
    } else {
        Width::Word
    }
}

/// `value` rounded up to the next multiple of `alignment`.
fn align_to(value: u64, alignment: u64) -> u64 {
    if alignment == 0 {
        return value;
    }

    let remainder = value % alignment;
    if remainder == 0 {
        return value;
    }

    value.saturating_add(alignment - remainder)
}

#[cfg(test)]
mod tests;
