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

use crate::ast::{Block, FuncDef, Item, Program, Stmt, StmtKind};
use crate::codegen::emit::{Emitter, Width};
use crate::codegen::frame::{temporaries_needed, FrameLayout};
use crate::diagnostics::{Diagnostic, DiagnosticKind, Span};
use crate::sema::annotations::Annotations;

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
        if let Item::FuncDef(def) = item {
            generator.function(def);
        }
    }

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
    /// Gaps in the generator found while walking.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl<'a> Generator<'a> {
    /// A generator that has emitted nothing.
    fn new(annotations: &'a Annotations) -> Self {
        Self {
            emitter: Emitter::new(),
            annotations,
            layout: FrameLayout::build(&crate::sema::annotations::Frame::default(), 0),
            depth: 0,
            epilogue: String::new(),
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

    /// Lowers one function definition, from its prologue to its single epilogue.
    fn function(&mut self, def: &FuncDef) {
        let name = &def.signature.name.text;
        let Some(frame) = self.annotations.frame(name) else {
            self.unlowered(def.signature.span, "a function with no frame inventory");

            return;
        };
        let frame = frame.clone();

        self.layout = FrameLayout::build(&frame, temporaries_needed(&def.body));
        self.depth = 0;
        self.epilogue = self.emitter.new_label(&format!("{name}_return"));

        self.emitter.begin_function(name);
        self.layout.emit_prologue(&mut self.emitter, &frame.slots);

        self.block(&def.body);

        // Falling out of the body is reaching the end of the function. Analysis proved that only
        // happens where the return value does not matter, so the epilogue is simply next.
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
            StmtKind::If { .. } => self.unlowered(stmt.span, "an `if` statement"),
            StmtKind::While { .. } => self.unlowered(stmt.span, "a `while` loop"),
            StmtKind::For { .. } => self.unlowered(stmt.span, "a `for` loop"),
            StmtKind::Break => self.unlowered(stmt.span, "`break`"),
            StmtKind::Continue => self.unlowered(stmt.span, "`continue`"),
        }
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

        match init {
            crate::ast::Initializer::Expr(value) => {
                let width = self.width_of(value.id);
                self.expr(value);
                self.emitter.store_to_frame("w0", width, base);
            }
            crate::ast::Initializer::List { elements, .. } => {
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

/// How many bytes one element of `width` occupies.
pub(crate) fn element_stride(width: Width) -> u64 {
    match width {
        Width::Byte => 1,
        Width::Word => 4,
        Width::Double => 8,
    }
}
