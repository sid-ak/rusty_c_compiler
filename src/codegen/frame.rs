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

/// The same count, where a `u32` is wanted.
const ARGUMENT_REGISTERS_U32: u32 = 8;

/// The largest offset `stp` can carry, so the saved pair can be placed above the outgoing area.
const PAIR_OFFSET_LIMIT: u64 = 504;

/// What a function's body needs room for, beyond its named locals and parameters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Requirements {
    /// One temporary per level of expression nesting.
    pub temporaries: u32,
    /// The most arguments any one call in the function passes.
    pub arguments: u32,
    /// How deeply calls nest, so `f(g(1))` does not reuse `f`'s argument slots for `g`'s.
    pub call_depth: u32,
    /// Bytes of outgoing stack arguments, for calls passing more than eight.
    pub outgoing: u64,
}

/// What `body` needs room for.
pub fn requirements(body: &Block) -> Requirements {
    let mut found = Requirements {
        temporaries: block_depth(body),
        ..Requirements::default()
    };
    survey_block(body, 0, &mut found);

    // Eight arguments travel in registers. Anything past that goes on the stack, and eight bytes
    // each is at least as much room as the packed layout actually uses.
    let on_the_stack = u64::from(found.arguments.saturating_sub(ARGUMENT_REGISTERS_U32));
    found.outgoing = align_to(on_the_stack.saturating_mul(8), STACK_ALIGNMENT);

    found
}

/// Where everything in one function's frame lives.
#[derive(Debug, Clone)]
pub struct FrameLayout {
    /// The frame's total size, a multiple of sixteen.
    size: u64,
    /// Bytes at the bottom of the frame holding arguments for calls this function makes.
    outgoing: u64,
    /// The `x29`-relative offset of each named slot.
    offsets: BTreeMap<SlotId, u64>,
    /// The `x29`-relative offset of each expression temporary, by nesting depth.
    temporaries: Vec<u64>,
    /// The `x29`-relative offset of each argument slot, by call depth then argument position.
    arguments: Vec<u64>,
    /// How many argument slots each call depth has, so an index can be worked out.
    arguments_per_call: u32,
}

impl FrameLayout {
    /// Lays out `frame`, with room for `temporaries` nested expression results.
    ///
    /// Slots are placed in declaration order. Any order would work; this one is deterministic and
    /// reads the way the source does, which is what makes an emitted frame comparable against the
    /// function that produced it.
    pub fn build(frame: &Frame, needs: Requirements) -> Self {
        let mut cursor = SAVED_REGISTERS;
        let mut offsets = BTreeMap::new();

        for slot in &frame.slots {
            cursor = align_to(cursor, slot.align.max(1));
            offsets.insert(slot.slot, cursor);
            cursor = cursor.saturating_add(slot.size);
        }

        let mut temporary_offsets = Vec::new();
        for _ in 0..needs.temporaries {
            cursor = align_to(cursor, TEMPORARY_BYTES);
            temporary_offsets.push(cursor);
            cursor = cursor.saturating_add(TEMPORARY_BYTES);
        }

        // One block of argument slots per level of call nesting, so the arguments of `f(g(1))`
        // cannot be written over by `g`'s while `f`'s are still waiting to be placed.
        let mut argument_offsets = Vec::new();
        let blocks = needs.call_depth.saturating_mul(needs.arguments);
        for _ in 0..blocks {
            cursor = align_to(cursor, TEMPORARY_BYTES);
            argument_offsets.push(cursor);
            cursor = cursor.saturating_add(TEMPORARY_BYTES);
        }

        Self {
            size: align_to(cursor.saturating_add(needs.outgoing), STACK_ALIGNMENT),
            outgoing: needs.outgoing,
            offsets,
            temporaries: temporary_offsets,
            arguments: argument_offsets,
            arguments_per_call: needs.arguments,
        }
    }

    /// Where the `index`th argument of a call nested `depth` calls deep is held while it waits.
    pub fn argument(&self, depth: u32, index: u32) -> Option<u64> {
        let position = depth
            .checked_mul(self.arguments_per_call)?
            .checked_add(index)?;

        self.arguments.get(usize::try_from(position).ok()?).copied()
    }

    /// Bytes at the bottom of the frame reserved for arguments this function passes on the stack.
    pub fn outgoing(&self) -> u64 {
        self.outgoing
    }

    /// Where this function's own stack-passed arguments arrived, as an offset from `x29`.
    ///
    /// They sit just above the frame: the caller wrote them at the stack pointer it held when it
    /// branched, which is where this frame ends.
    pub fn incoming(&self) -> u64 {
        self.size.saturating_sub(self.outgoing)
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
        if self.outgoing == 0 && self.size <= PRE_INDEXED_LIMIT {
            emitter.instruction(&format!("stp x29, x30, [sp, #-{}]!", self.size));
            emitter.instruction("mov x29, sp");
        } else {
            // The stack pointer is lowered on its own, either because the pre-indexed immediate
            // cannot reach this far or because the bottom of the frame is reserved for arguments
            // this function passes on the stack. `x29` then sits above that area, so every offset
            // to a named slot is the same as it would be in the simpler shape.
            self.adjust_stack(emitter, "sub");
            self.store_pair(emitter, "stp");
            self.address_of_saved_pair(emitter, "add x29, sp");
        }

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

            let width = width_for(slot.size);

            if position >= ARGUMENT_REGISTERS {
                // This one arrived on the stack. It is copied into its slot anyway, so the body
                // reaches every parameter the same way.
                let arrived = self
                    .incoming()
                    .saturating_add(incoming_offset(slots, position));
                emitter.load_from_frame("w8", width, arrived);
                emitter.store_to_frame("w8", width, offset);

                continue;
            }

            emitter.store_to_frame(&format!("w{position}"), width, offset);
        }
    }

    /// Closes the frame and returns.
    ///
    /// One epilogue per function, which every `return` branches to, so the sequence that undoes the
    /// prologue is written once and cannot disagree with itself.
    pub fn emit_epilogue(&self, emitter: &mut Emitter) {
        if self.outgoing == 0 && self.size <= PRE_INDEXED_LIMIT {
            emitter.instruction("mov sp, x29");
            emitter.instruction(&format!("ldp x29, x30, [sp], #{}", self.size));
        } else {
            // Nothing moves the stack pointer between the prologue and here, so putting it back is
            // arithmetic on where it already is.
            self.store_pair(emitter, "ldp");
            self.adjust_stack(emitter, "add");
        }

        emitter.instruction("ret");
    }

    /// Lowers or raises the stack pointer by the frame's size.
    fn adjust_stack(&self, emitter: &mut Emitter, operation: &str) {
        if self.size <= 4095 {
            emitter.instruction(&format!("{operation} sp, sp, #{}", self.size));

            return;
        }

        emitter.load_immediate(SCRATCH, self.size);
        emitter.instruction(&format!("{operation} sp, sp, {SCRATCH}"));
    }

    /// Stores or loads the saved pair, which sits just above the outgoing argument area.
    fn store_pair(&self, emitter: &mut Emitter, operation: &str) {
        if self.outgoing <= PAIR_OFFSET_LIMIT {
            emitter.instruction(&format!("{operation} x29, x30, [sp, #{}]", self.outgoing));

            return;
        }

        emitter.load_immediate(SCRATCH, self.outgoing);
        emitter.instruction(&format!("add {SCRATCH}, sp, {SCRATCH}"));
        emitter.instruction(&format!("{operation} x29, x30, [{SCRATCH}]"));
    }

    /// Points `x29` at the saved pair with `{prefix}, #offset`.
    fn address_of_saved_pair(&self, emitter: &mut Emitter, prefix: &str) {
        if self.outgoing <= 4095 {
            emitter.instruction(&format!("{prefix}, #{}", self.outgoing));

            return;
        }

        emitter.load_immediate(SCRATCH, self.outgoing);
        emitter.instruction(&format!("{prefix}, {SCRATCH}"));
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

/// Walks `block`, recording the widest and deepest calls it contains.
fn survey_block(block: &Block, depth: u32, found: &mut Requirements) {
    for stmt in &block.stmts {
        survey_stmt(stmt, depth, found);
    }
}

/// Walks one statement for the calls inside it.
fn survey_stmt(stmt: &Stmt, depth: u32, found: &mut Requirements) {
    match &stmt.kind {
        StmtKind::Block(block) => survey_block(block, depth, found),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            survey_expr(condition, depth, found);
            survey_stmt(then_branch, depth, found);
            if let Some(branch) = else_branch {
                survey_stmt(branch, depth, found);
            }
        }
        StmtKind::While { condition, body } => {
            survey_expr(condition, depth, found);
            survey_stmt(body, depth, found);
        }
        StmtKind::For {
            init,
            condition,
            step,
            body,
        } => {
            match init.as_deref() {
                Some(ForInit::Decl(decl)) => {
                    if let Some(init) = &decl.init {
                        survey_initializer(init, depth, found);
                    }
                }
                Some(ForInit::Expr(expr)) => survey_expr(expr, depth, found),
                None => {}
            }
            if let Some(condition) = condition {
                survey_expr(condition, depth, found);
            }
            if let Some(step) = step {
                survey_expr(step, depth, found);
            }
            survey_stmt(body, depth, found);
        }
        StmtKind::Return(Some(expr)) | StmtKind::Expr(expr) => survey_expr(expr, depth, found),
        StmtKind::LocalVar(decl) => {
            if let Some(init) = &decl.init {
                survey_initializer(init, depth, found);
            }
        }
        StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Empty => {}
    }
}

/// Walks an initializer for the calls inside it.
fn survey_initializer(init: &Initializer, depth: u32, found: &mut Requirements) {
    match init {
        Initializer::Expr(expr) => survey_expr(expr, depth, found),
        Initializer::List { elements, .. } => {
            for element in elements {
                survey_expr(element, depth, found);
            }
        }
    }
}

/// Walks one expression, counting a call's arguments and how deep inside other calls it sits.
fn survey_expr(expr: &Expr, depth: u32, found: &mut Requirements) {
    match &expr.kind {
        ExprKind::IntLit(_) | ExprKind::CharLit(_) | ExprKind::StrLit(_) | ExprKind::Ident(_) => {}
        ExprKind::Unary { operand, .. } | ExprKind::PostfixIncDec { operand, .. } => {
            survey_expr(operand, depth, found);
        }
        ExprKind::Binary { left, right, .. } => {
            survey_expr(left, depth, found);
            survey_expr(right, depth, found);
        }
        ExprKind::Assign { target, value } => {
            survey_expr(target, depth, found);
            survey_expr(value, depth, found);
        }
        ExprKind::Index { base, index } => {
            survey_expr(base, depth, found);
            survey_expr(index, depth, found);
        }
        ExprKind::Call { callee, args } => {
            let here = depth.saturating_add(1);
            found.call_depth = found.call_depth.max(here);
            found.arguments = found
                .arguments
                .max(u32::try_from(args.len()).unwrap_or(u32::MAX));

            survey_expr(callee, depth, found);
            for arg in args {
                survey_expr(arg, here, found);
            }
        }
    }
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

/// Where the stack-passed parameter at `position` sits, relative to the start of the incoming area.
///
/// Apple's ARM64 platforms pack stack arguments at their natural size and alignment rather than
/// giving each one eight bytes, which `clang -S` shows plainly: a ninth `int` argument is written
/// with `str w8`, four bytes, not `str x8`.
pub fn incoming_offset(slots: &[FrameSlot], position: usize) -> u64 {
    let mut cursor = 0;

    for slot in slots {
        let SymbolKind::Parameter(index) = slot.kind else {
            continue;
        };
        let Ok(index) = usize::try_from(index) else {
            continue;
        };
        if index < ARGUMENT_REGISTERS {
            continue;
        }

        cursor = align_to(cursor, slot.align.max(1));
        if index == position {
            return cursor;
        }
        cursor = cursor.saturating_add(slot.size);
    }

    cursor
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
