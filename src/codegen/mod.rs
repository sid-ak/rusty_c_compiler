//! ARM64 code generation: the annotated tree becomes assembly.
//!
//! This stage does no type reasoning. Every question it would otherwise have to ask — what type is
//! this, where does this name live, does this value need widening, does this array need its
//! address — was answered by semantic analysis and is read back out of the annotations. What is
//! left is a structural walk: each node has a shape, and each shape has a sequence of instructions.
//!
//! Values live in the frame and visit registers. `w0` holds whatever the expression being lowered
//! evaluated to, and a binary operator's right operand is in `w1` at the point of use. Nothing is
//! kept in a register across the evaluation of anything else, which is
//! [ADR 0005](../../docs/decisions/0005-stack-spilling-instead-of-register-allocation.md): slower
//! than deciding which values deserve registers, and without the class of bug where two live values
//! are given the same one.

pub mod emit;
pub mod expr;
pub mod frame;

use crate::ast::{
    Block, Expr, ExprKind, ForInit, FuncDef, Initializer, Item, Program, Stmt, StmtKind, VarDecl,
};
use crate::codegen::emit::{Emitter, Width};
use crate::codegen::frame::{requirements, FrameLayout, Requirements};
use crate::diagnostics::{Diagnostic, DiagnosticKind, Span};
use crate::sema::annotations::Annotations;
use crate::sema::constant_value;
use crate::sema::types::Ty;

/// Where `break` and `continue` go from inside one loop.
///
/// A stack of these, rather than one pair, is what makes the innermost loop the one they apply to.
/// The two labels differ for a `for`: `continue` goes to the step clause, not to the condition, so
/// the loop still advances.
#[derive(Debug, Clone)]
pub(crate) struct LoopLabels {
    /// Where `break` goes: past the end of the loop.
    pub(crate) exit: String,
    /// Where `continue` goes: on to the next iteration, through the step clause if there is one.
    pub(crate) next: String,
}

/// What code generation produced.
#[derive(Debug, Clone)]
pub struct Generated {
    /// The assembly text.
    pub assembly: String,
    /// Anything the generator could not lower.
    ///
    /// These are not problems with the user's program — semantic analysis already accepted it — so
    /// they are [`DiagnosticKind::Internal`]: the compiler reporting a gap in itself. An empty list
    /// is the only acceptable outcome for a program that got this far.
    pub diagnostics: Vec<Diagnostic>,
}

/// Generates assembly for `program`, using what analysis recorded about it.
pub fn generate(program: &Program, annotations: &Annotations) -> Generated {
    let mut generator = Generator::new(annotations);

    for item in &program.items {
        match item {
            Item::FuncDef(def) => generator.function(def),
            Item::GlobalVar(decl) => generator.global(decl),
            Item::FuncDecl(_) => {}
        }
    }

    generator.string_table();

    generator.finish()
}

/// The walk, and the state one function's lowering needs.
pub(crate) struct Generator<'a> {
    /// Where the assembly is written.
    pub(crate) emitter: Emitter,
    /// What analysis recorded about the program.
    pub(crate) annotations: &'a Annotations,
    /// Where the current function keeps its values.
    pub(crate) layout: FrameLayout,
    /// How deeply nested the expression being lowered is, which picks its temporary slot.
    pub(crate) depth: u32,
    /// The label every `return` in the current function branches to.
    pub(crate) epilogue: String,
    /// The enclosing loops, innermost last, giving `break` and `continue` somewhere to go.
    pub(crate) loops: Vec<LoopLabels>,
    /// How many calls enclose the expression being lowered, which picks its argument slots.
    pub(crate) call_depth: u32,
    /// Gaps in the generator found while walking.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl<'a> Generator<'a> {
    /// A generator that has emitted nothing.
    fn new(annotations: &'a Annotations) -> Self {
        Self {
            emitter: Emitter::new(),
            annotations,
            layout: FrameLayout::build(
                &crate::sema::annotations::Frame::default(),
                Requirements::default(),
            ),
            depth: 0,
            epilogue: String::new(),
            loops: Vec::new(),
            call_depth: 0,
            diagnostics: Vec::new(),
        }
    }

    /// The finished assembly and anything that could not be lowered.
    fn finish(self) -> Generated {
        Generated {
            assembly: self.emitter.finish(),
            diagnostics: self.diagnostics,
        }
    }

    /// Reports a construct this generator does not know how to lower.
    pub(crate) fn unlowered(&mut self, span: Span, construct: &str) {
        self.diagnostics.push(Diagnostic::new(
            DiagnosticKind::Internal,
            span,
            format!("code generation does not yet lower {construct}"),
        ));
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
        self.emitter.instruction("movz w0, #0");
        while written < u64::from(*count) {
            self.emitter
                .store_to_frame("w0", width, base.saturating_add(written * stride));
            written = written.saturating_add(1);
        }
    }

    /// Emits every distinct string literal the program used, under the label analysis interned it to.
    ///
    /// One entry per distinct literal, so two occurrences of `"hello"` share one run of bytes in
    /// the read-only section. The bytes are the ones the lexer decoded; nothing here re-reads an
    /// escape from the source text.
    fn string_table(&mut self) {
        for literal in self.annotations.strings() {
            self.emitter.define_string(&literal.label, &literal.bytes);
        }
    }

    /// Emits one global variable, into the data section or the zero-filled one.
    fn global(&mut self, decl: &VarDecl) {
        let Some(symbol) = self
            .annotations
            .binding_of(decl.id)
            .and_then(|id| self.annotations.symbol(id))
        else {
            self.unlowered(decl.span, "a global with no binding");

            return;
        };
        let name = symbol.name.clone();
        let ty = symbol.ty.clone();
        let Some(layout) = ty.layout() else {
            self.unlowered(decl.span, "a global whose type has no storage");

            return;
        };

        let Some(init) = &decl.init else {
            // Nothing to write. `.zerofill` records the size and the loader provides the zeroes,
            // so a large uninitialized array costs nothing in the object file.
            self.emitter
                .reserve_zeroed(&name, layout.size, power_of_two(layout.align));

            return;
        };

        self.emitter.begin_data(&name, power_of_two(layout.align));

        let (element, count) = match &ty {
            Ty::Array(element, count) => (element.as_ref().clone(), u64::from(*count)),
            scalar => (scalar.clone(), 1),
        };
        let stride = element.layout().map_or(1, |layout| layout.size);
        let written = self.global_values(decl, init, &element, count);

        // An initializer shorter than the array leaves the rest zeroed, which is what C says a
        // partial brace list means.
        self.emitter
            .data_zero(count.saturating_sub(written).saturating_mul(stride));
    }

    /// Writes the values in `init`, returning how many elements were written.
    fn global_values(
        &mut self,
        decl: &VarDecl,
        init: &Initializer,
        element: &Ty,
        count: u64,
    ) -> u64 {
        match init {
            // A string literal filling a `char` array is written as its decoded bytes, terminator
            // included, rather than as a reference to the read-only copy.
            Initializer::Expr(value) => {
                if let ExprKind::StrLit(bytes) = &value.kind {
                    let mut written = 0;
                    for byte in bytes.iter().chain(std::iter::once(&0)) {
                        if written >= count {
                            break;
                        }
                        self.emitter.data_byte(i32::from(*byte));
                        written = written.saturating_add(1);
                    }

                    return written;
                }

                self.global_element(decl, value, element);

                1
            }
            Initializer::List { elements, .. } => {
                let mut written = 0;
                for value in elements {
                    if written >= count {
                        break;
                    }
                    self.global_element(decl, value, element);
                    written = written.saturating_add(1);
                }

                written
            }
        }
    }

    /// Writes one constant value at the element type's width.
    fn global_element(&mut self, decl: &VarDecl, value: &Expr, element: &Ty) {
        let Some(folded) = constant_value(value) else {
            // Analysis rejects a non-constant global initializer, so reaching here is a gap in the
            // compiler rather than a problem with the program.
            self.unlowered(
                decl.span,
                "a global initializer that did not fold to a constant",
            );

            return;
        };

        if element == &Ty::Char {
            self.emitter.data_byte(folded);
        } else {
            self.emitter.data_word(folded);
        }
    }

    /// Lowers one function definition, from its prologue to its single epilogue.
    fn function(&mut self, def: &FuncDef) {
        let name = &def.signature.name.text;
        let Some(frame) = self.annotations.frame(name) else {
            self.unlowered(def.signature.span, "a function with no frame inventory");

            return;
        };
        let frame = frame.clone();

        self.layout = FrameLayout::build(&frame, requirements(&def.body));
        self.depth = 0;
        self.call_depth = 0;
        self.epilogue = self.emitter.new_label(&format!("{name}_return"));

        self.emitter.begin_function(name);
        self.layout.emit_prologue(&mut self.emitter, &frame.slots);

        self.block(&def.body);

        // Falling out of the body is reaching the end of the function. Analysis proved that only
        // happens in `main` or in a `void` function, so the only value that has to be produced here
        // is `main`'s implicit zero. An explicit `return` branches straight to the label below and
        // never passes through it.
        if name == "main" {
            self.emitter.instruction("mov w0, #0");
        }

        self.emitter.place_label(&self.epilogue.clone());
        self.layout.emit_epilogue(&mut self.emitter);
    }

    /// Lowers every statement in a block.
    fn block(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.stmt(stmt);
        }
    }

    /// Lowers one statement.
    ///
    /// Control flow is task 4; the forms here are the ones an expression needs in order to be run
    /// at all, and anything else reports itself rather than emitting code that would be wrong.
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

                for (index, element) in elements.iter().enumerate() {
                    self.expr(element);

                    let Ok(index) = u64::try_from(index) else {
                        continue;
                    };
                    self.emitter
                        .store_to_frame("w0", width, base.saturating_add(index * stride));
                }
            }
        }
    }
}

/// Whether `init` is a string literal filling an array rather than a value being assigned.
fn fills_an_array(init: &Expr, declared: &Ty) -> bool {
    matches!(init.kind, ExprKind::StrLit(_)) && matches!(declared, Ty::Array(_, _))
}

/// The power of two that `alignment` is, which is what `.p2align` wants.
fn power_of_two(alignment: u64) -> u32 {
    match alignment {
        1 => 0,
        2 => 1,
        4 => 2,
        8 => 3,
        // Nothing in this subset aligns more strictly than a pointer; sixteen is the safe answer
        // for anything that somehow did, since over-aligning is never wrong.
        _ => 4,
    }
}

/// How many bytes one element of `width` occupies.
pub(crate) fn element_stride(width: Width) -> u64 {
    match width {
        Width::Byte => 1,
        Width::Word => 4,
        Width::Double => 8,
    }
}
