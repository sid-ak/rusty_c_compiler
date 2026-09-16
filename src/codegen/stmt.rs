//! Statement lowering: control flow, declarations, and the places a value is discarded.
//!
//! Two things here are decisions rather than transcription.
//!
//! `break` and `continue` do not know which loop they are in. The generator does, because it pushes
//! a pair of labels on the way into a loop and pops them on the way out, so the innermost pair is
//! the one they find. The two labels are not the same place: in a `for`, `continue` goes to the
//! step clause rather than to the condition, and sending it to the condition assembles perfectly
//! well and produces a loop that never advances.
//!
//! An `if` jumps past its `else` arm rather than relying on layout. The arms are emitted one after
//! the other, so a `then` arm that simply ended would run the `else` arm as well.

use crate::ast::{Block, Expr, ExprKind, ForInit, Initializer, Stmt, StmtKind};
use crate::codegen::emit::Width;
use crate::codegen::{element_stride, fills_an_array, Generator, LoopLabels};
use crate::diagnostics::Span;
use crate::sema::types::Ty;

impl Generator<'_> {
    /// Lowers every statement in a block.
    pub(crate) fn block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.stmt(stmt);
        }
    }

    /// Lowers one statement.
    fn stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Block(block) => self.block(block),
            StmtKind::Expr(expr) => self.expr(expr),
            StmtKind::LocalVar(decl) => self.local(decl),
            StmtKind::Return(value) => {
                if let Some(value) = value {
                    self.expr(value);
                }
                self.emitter.branch(&self.epilogue.clone());
            }
            StmtKind::Empty => {}
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => self.branch(condition, then_branch, else_branch.as_deref()),
            StmtKind::While { condition, body } => self.while_loop(condition, body),
            StmtKind::For {
                init,
                condition,
                step,
                body,
            } => self.for_loop(init.as_deref(), condition.as_ref(), step.as_ref(), body),
            StmtKind::Break => self.leave_loop(stmt.span, true),
            StmtKind::Continue => self.leave_loop(stmt.span, false),
        }
    }

    /// Lowers an `if`, with or without an `else`.
    ///
    /// The `then` arm ends in a jump past the `else` arm, because the two are laid out one after
    /// the other and falling out of the first into the second would run both.
    fn branch(&mut self, condition: &Expr, then_branch: &Stmt, else_branch: Option<&Stmt>) {
        let end = self.emitter.new_label("if_end");
        let otherwise = match else_branch {
            Some(_) => self.emitter.new_label("if_else"),
            None => end.clone(),
        };

        self.expr(condition);
        self.emitter.branch_if_zero("w0", &otherwise);
        self.stmt(then_branch);

        if let Some(else_branch) = else_branch {
            self.emitter.branch(&end);
            self.emitter.place_label(&otherwise);
            self.stmt(else_branch);
        }

        self.emitter.place_label(&end);
    }

    /// Lowers a `while`, testing before each iteration.
    fn while_loop(&mut self, condition: &Expr, body: &Stmt) {
        let top = self.emitter.new_label("while_top");
        let end = self.emitter.new_label("while_end");

        self.emitter.place_label(&top);
        self.expr(condition);
        self.emitter.branch_if_zero("w0", &end);

        // `continue` goes back to the condition, since a `while` has no step clause.
        self.loops.push(LoopLabels {
            exit: end.clone(),
            next: top.clone(),
        });
        self.stmt(body);
        self.loops.pop();

        self.emitter.branch(&top);
        self.emitter.place_label(&end);
    }

    /// Lowers a `for`, with any of its three clauses absent.
    fn for_loop(
        &mut self,
        init: Option<&ForInit>,
        condition: Option<&Expr>,
        step: Option<&Expr>,
        body: &Stmt,
    ) {
        let top = self.emitter.new_label("for_top");
        let step_label = self.emitter.new_label("for_step");
        let end = self.emitter.new_label("for_end");

        match init {
            Some(ForInit::Decl(decl)) => self.local(decl),
            Some(ForInit::Expr(expr)) => self.expr(expr),
            None => {}
        }

        self.emitter.place_label(&top);
        // An absent condition is a condition that never fails, so nothing is tested at all.
        if let Some(condition) = condition {
            self.expr(condition);
            self.emitter.branch_if_zero("w0", &end);
        }

        // `continue` goes to the step clause rather than to the condition. Sending it to the
        // condition would skip the step, and a loop counting with one would never advance.
        self.loops.push(LoopLabels {
            exit: end.clone(),
            next: step_label.clone(),
        });
        self.stmt(body);
        self.loops.pop();

        self.emitter.place_label(&step_label);
        if let Some(step) = step {
            self.expr(step);
        }
        self.emitter.branch(&top);
        self.emitter.place_label(&end);
    }

    /// Lowers `break` or `continue` against the innermost enclosing loop.
    fn leave_loop(&mut self, span: Span, breaking: bool) {
        let Some(labels) = self.loops.last() else {
            // Analysis rejects these outside a loop, so reaching here is a gap in the compiler
            // rather than a problem with the program.
            self.unlowered(span, "`break` or `continue` outside a loop");

            return;
        };

        let target = if breaking {
            labels.exit.clone()
        } else {
            labels.next.clone()
        };
        self.emitter.branch(&target);
    }

    /// Lowers a local declaration, storing its initializer if it has one.
    fn local(&mut self, decl: &crate::ast::VarDecl) {
        let Some(init) = &decl.init else {
            return;
        };
        let Some(symbol) = self.annotations.binding_of(decl.id) else {
            self.unlowered(decl.span, "a local with no binding");

            return;
        };
        let Some(slot) = self
            .annotations
            .symbol(symbol)
            .and_then(|symbol| symbol.slot)
        else {
            self.unlowered(decl.span, "a local with no frame slot");

            return;
        };
        let Some(base) = self.layout.offset_of(slot) else {
            self.unlowered(decl.span, "a local with no frame offset");

            return;
        };
        let declared = self
            .annotations
            .symbol(symbol)
            .map_or(Ty::Int, |symbol| symbol.ty.clone());

        match init {
            // A string literal filling a `char` array is a copy of its bytes, not a reference to
            // the read-only one. Evaluating it as an expression would produce the literal's
            // address, and storing that into the array would put a fragment of a pointer where the
            // text should be.
            Initializer::Expr(value) if fills_an_array(value, &declared) => {
                self.copy_string_into(value, &declared, base);
            }
            Initializer::Expr(value) => {
                let width = self.width_of(value.id);
                self.expr(value);
                self.emitter.store_to_frame("w0", width, base);
            }
            Initializer::List { elements, .. } => {
                let width = self.element_width(decl.id);
                let stride = element_stride(width);
                let mut written = 0u64;

                for element in elements {
                    self.expr(element);
                    self.emitter
                        .store_to_frame("w0", width, base.saturating_add(written * stride));
                    written = written.saturating_add(1);
                }

                // The elements the list did not reach are zero. A global gets that from its section
                // being zero to begin with; a local's storage is whatever the stack was last using
                // it for, so the zeros are written here or they are not there at all.
                if let Ty::Array(_, count) = &declared {
                    self.zero_elements(base, written, u64::from(*count), width, stride);
                }
            }
        }
    }

    /// Copies a string literal's bytes into the array at `base`, zeroing whatever it does not fill.
    ///
    /// Written out one byte at a time. A copy loop would be shorter in the emitted code and longer
    /// here, and the arrays this subset can declare are small enough that the trade is not close.
    fn copy_string_into(&mut self, value: &Expr, declared: &Ty, base: u64) {
        let ExprKind::StrLit(bytes) = &value.kind else {
            return;
        };
        let Ty::Array(element, count) = declared else {
            return;
        };
        let stride = element.layout().map_or(1, |layout| layout.size);
        let width = if element.as_ref() == &Ty::Char {
            Width::Byte
        } else {
            Width::Word
        };

        let mut written = 0u64;
        for byte in bytes.iter().chain(std::iter::once(&0)) {
            if written >= u64::from(*count) {
                break;
            }
            self.emitter.load_word_immediate("w0", i32::from(*byte));
            self.emitter
                .store_to_frame("w0", width, base.saturating_add(written * stride));
            written = written.saturating_add(1);
        }

        // Anything the literal did not reach is zero, the same as a short brace list.
        self.zero_elements(base, written, u64::from(*count), width, stride);
    }

    /// Writes zero into the elements of the array at `base` from `written` up to `count`.
    ///
    /// Shared by the two initializer forms that can stop short of the end — a brace list with
    /// fewer values than the array holds, and a string literal shorter than the `char` array it
    /// fills — so the two cannot disagree about what an unmentioned element holds.
    ///
    /// The zero register is loaded once and reused, which is why this is a loop here rather than a
    /// loop in the emitted code: the arrays this subset can declare are small enough that the
    /// unrolled stores are cheaper than a counter and a branch.
    fn zero_elements(&mut self, base: u64, written: u64, count: u64, width: Width, stride: u64) {
        if written >= count {
            return;
        }

        self.emitter.instruction("movz w0, #0");
        for index in written..count {
            self.emitter
                .store_to_frame("w0", width, base.saturating_add(index * stride));
        }
    }
}
