//! Semantic analysis: the pass between parsing and code generation.
//!
//! Analysis answers every type question in the program exactly once and records the answers, so
//! the code generator performs no type reasoning of its own. The design is in
//! `docs/dive-deep/semantic-analysis.md`; the rule that the answers live beside the AST rather
//! than in it is ADR 0004.
//!
//! The walk runs in two sub-passes over the same tree:
//!
//! - Pass A reads the top level only, registering every global and every function signature. It
//!   is what lets a function call another one defined further down the file, which is ordinary C
//!   and impossible to resolve in a single pass without a forward-reference hack.
//! - Pass B walks each function body with that table already populated, resolving identifiers,
//!   typing expressions, and checking the rules.
//!
//! Nothing here stops at the first mistake. Every check reports and carries on with a plausible
//! type, so one run tells the programmer as much about the file as it can, and a cascade of
//! follow-on errors from one real one is avoided by choosing the recovery type rather than by
//! giving up.

pub mod annotations;
pub mod scope;
pub mod types;

use std::collections::BTreeSet;

use crate::ast::{
    BaseType, Block, Expr, ExprKind, ForInit, FuncSig, Initializer, Item, NodeId, Program, Stmt,
    StmtKind, TypeSpec, UnOp, VarDecl,
};
use crate::diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, Span};
use crate::parser::MAX_NESTING_DEPTH;
use crate::sema::annotations::Annotations;
use crate::sema::scope::{Scopes, SymbolId, SymbolKind};
use crate::sema::types::Ty;

/// What analysis produced: the annotations, and whatever it found wrong.
#[derive(Debug)]
pub struct Analysis {
    /// Everything the code generator needs that is not in the AST.
    pub annotations: Annotations,
    /// The problems found, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

impl Analysis {
    /// Whether the program was accepted.
    pub fn is_accepted(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// Analyze `program`, returning its annotations and any problems found.
pub fn analyze(program: &Program) -> Analysis {
    let mut analyzer = Analyzer::new();

    analyzer.collect_top_level(program);
    analyzer.check_bodies(program);

    analyzer.finish()
}

/// Where a declaration appears, which decides what its written type means.
///
/// The distinction exists for one rule: an array parameter is a pointer. `int a[10]` declares ten
/// integers as a local and one pointer as a parameter, because C passes arrays by address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeclPosition {
    /// A global, a local, or anything else that owns its storage.
    Variable,
    /// A function parameter.
    Parameter,
}

/// The walk, and everything it accumulates along the way.
struct Analyzer {
    /// Names in scope, and the symbol table behind them.
    scopes: Scopes,
    /// What the walk has recorded so far.
    annotations: Annotations,
    /// What it has found wrong.
    diagnostics: DiagnosticBag,
    /// Functions that have a body, so a second body can be reported.
    defined: BTreeSet<String>,
    /// How deep in the tree the walk currently is.
    depth: usize,
    /// Whether the depth limit has already been reported, so it is reported once.
    depth_reported: bool,
}

impl Analyzer {
    /// A walk that has seen nothing.
    fn new() -> Self {
        Self {
            scopes: Scopes::new(),
            annotations: Annotations::new(Scopes::new()),
            diagnostics: DiagnosticBag::new(),
            defined: BTreeSet::new(),
            depth: 0,
            depth_reported: false,
        }
    }

    /// The finished annotations and diagnostics.
    fn finish(mut self) -> Analysis {
        self.annotations.set_symbols(self.scopes);

        Analysis {
            annotations: self.annotations,
            diagnostics: self.diagnostics.into_sorted(),
        }
    }

    /// Reports `message` at `span` as a problem with what the program means.
    fn report(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::new(DiagnosticKind::Semantic, span, message));
    }

    /// Reports `message` at `span`, with `note` pointing at the declaration it collides with.
    fn report_with_note(
        &mut self,
        span: Span,
        message: impl Into<String>,
        note: impl Into<String>,
    ) {
        self.diagnostics
            .push(Diagnostic::new(DiagnosticKind::Semantic, span, message).with_note(note));
    }

    // -- Pass A: the top level ------------------------------------------------------------------

    /// Registers every global and every function signature, so pass B can resolve forward calls.
    fn collect_top_level(&mut self, program: &Program) {
        for item in &program.items {
            match item {
                Item::FuncDecl(decl) => self.declare_function(&decl.signature, false),
                Item::FuncDef(def) => self.declare_function(&def.signature, true),
                Item::GlobalVar(decl) => self.declare_global(decl),
            }
        }
    }

    /// Registers `signature`, reconciling it with any earlier declaration of the same name.
    ///
    /// Three ways this goes wrong, each with its own message: the name is already something other
    /// than a function, the signature disagrees with the one already recorded, or the function
    /// already has a body.
    fn declare_function(&mut self, signature: &FuncSig, is_definition: bool) {
        let ty = self.signature_type(signature);
        let name = &signature.name.text;

        if let Some(existing_id) = self.scopes.lookup_in_current_scope(name) {
            let Some(existing) = self.scopes.symbol(existing_id) else {
                return;
            };
            let matches_kind = existing.kind == SymbolKind::Function;
            let matches_type = existing.ty == ty;

            if !matches_kind {
                self.report_redeclaration(name, signature.name.span);
            } else if !matches_type {
                self.report_with_note(
                    signature.name.span,
                    format!("conflicting declaration of '{name}'"),
                    format!("previous declaration of '{name}' is here"),
                );
            } else if is_definition && self.defined.contains(name) {
                self.report_with_note(
                    signature.name.span,
                    format!("redefinition of '{name}'"),
                    format!("previous definition of '{name}' is here"),
                );
            }
        } else {
            // The lookup above proved the name is free in this scope, so this cannot collide.
            // A `FuncSig` carries no node id of its own, so there is no binding to record here;
            // calls bind to the symbol through their callee's identifier node instead.
            let _ = self
                .scopes
                .declare(name, ty, SymbolKind::Function, signature.name.span);
        }

        if is_definition {
            self.defined.insert(name.clone());
        }
    }

    /// The function type `signature` describes.
    fn signature_type(&mut self, signature: &FuncSig) -> Ty {
        let ret = self.spec_type(&signature.return_type, DeclPosition::Variable);
        let params = signature
            .params
            .iter()
            .map(|param| self.spec_type(&param.ty, DeclPosition::Parameter))
            .collect();

        Ty::func(ret, params)
    }

    /// Registers a global and checks that its initializer is something the data section can hold.
    fn declare_global(&mut self, decl: &VarDecl) {
        let ty = self.spec_type(&decl.ty, DeclPosition::Variable);
        self.declare(decl, ty.clone(), SymbolKind::Global);

        let Some(init) = &decl.init else {
            return;
        };

        self.type_initializer(init);

        if !self.is_constant_initializer(init) {
            self.report(init.span(), "global initializer is not a constant");
        }
    }

    /// Whether every expression in `init` can be folded to a constant at compile time.
    fn is_constant_initializer(&self, init: &Initializer) -> bool {
        match init {
            Initializer::Expr(expr) => {
                matches!(expr.kind, ExprKind::StrLit(_)) || constant_value(expr).is_some()
            }
            Initializer::List { elements, .. } => elements
                .iter()
                .all(|element| constant_value(element).is_some()),
        }
    }

    // -- Pass B: the function bodies ------------------------------------------------------------

    /// Walks the body of every function that has one.
    fn check_bodies(&mut self, program: &Program) {
        for item in &program.items {
            if let Item::FuncDef(def) = item {
                self.check_function(&def.signature, &def.body);
            }
        }
    }

    /// Walks one function body, with its parameters declared in the body's own scope.
    ///
    /// Parameters share the body's scope rather than getting one of their own, which is C's rule:
    /// `int f(int a) { int a; }` is a redeclaration, and only a nested block may shadow `a`.
    fn check_function(&mut self, signature: &FuncSig, body: &Block) {
        self.scopes.enter_function();

        for (position, param) in signature.params.iter().enumerate() {
            let ty = self.spec_type(&param.ty, DeclPosition::Parameter);
            let index = u32::try_from(position).unwrap_or(u32::MAX);

            self.declare_name(
                &param.name.text,
                ty,
                SymbolKind::Parameter(index),
                param.name.span,
                param.id,
            );
        }

        for stmt in &body.stmts {
            self.stmt(stmt);
        }

        self.scopes.leave_function();
    }

    /// Walks one statement.
    fn stmt(&mut self, stmt: &Stmt) {
        if !self.enter(stmt.span) {
            return;
        }

        match &stmt.kind {
            StmtKind::Block(block) => {
                self.scopes.enter_block();
                for stmt in &block.stmts {
                    self.stmt(stmt);
                }
                self.scopes.leave_block();
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expr(condition);
                self.stmt(then_branch);
                if let Some(branch) = else_branch {
                    self.stmt(branch);
                }
            }
            StmtKind::While { condition, body } => {
                self.expr(condition);
                self.stmt(body);
            }
            StmtKind::For {
                init,
                condition,
                step,
                body,
            } => {
                // The init clause gets a scope that encloses the body, so a variable declared
                // there is visible inside the loop and gone after it.
                self.scopes.enter_block();

                match init.as_deref() {
                    Some(ForInit::Decl(decl)) => self.declare_local(decl),
                    Some(ForInit::Expr(expr)) => {
                        self.expr(expr);
                    }
                    None => {}
                }
                if let Some(condition) = condition {
                    self.expr(condition);
                }
                if let Some(step) = step {
                    self.expr(step);
                }
                self.stmt(body);

                self.scopes.leave_block();
            }
            StmtKind::Return(Some(expr)) | StmtKind::Expr(expr) => {
                self.expr(expr);
            }
            StmtKind::LocalVar(decl) => self.declare_local(decl),
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Empty => {}
        }

        self.leave();
    }

    /// Declares a local and types its initializer against the declared type.
    fn declare_local(&mut self, decl: &VarDecl) {
        let ty = self.spec_type(&decl.ty, DeclPosition::Variable);

        if let Some(init) = &decl.init {
            self.type_initializer(init);
        }

        self.declare(decl, ty, SymbolKind::Local);
    }

    /// Types every expression in `init`.
    fn type_initializer(&mut self, init: &Initializer) {
        match init {
            Initializer::Expr(expr) => {
                self.expr(expr);
            }
            Initializer::List { elements, .. } => {
                for element in elements {
                    self.expr(element);
                }
            }
        }
    }

    // -- Expressions ----------------------------------------------------------------------------

    /// Types one expression, records the result against its node, and returns it.
    fn expr(&mut self, expr: &Expr) -> Ty {
        if !self.enter(expr.span) {
            self.annotations.record_type(expr.id, Ty::Int);

            return Ty::Int;
        }

        let ty = self.type_of(expr);
        self.annotations.record_type(expr.id, ty.clone());

        self.leave();

        ty
    }

    /// Works out what one expression's type is, without recording anything about the node itself.
    fn type_of(&mut self, expr: &Expr) -> Ty {
        match &expr.kind {
            ExprKind::IntLit(_) => Ty::Int,
            ExprKind::CharLit(_) => Ty::Char,
            // A string literal is an array of `char` one longer than its text, for the terminator.
            ExprKind::StrLit(bytes) => {
                let length = u32::try_from(bytes.len().saturating_add(1)).unwrap_or(u32::MAX);

                Ty::array(Ty::Char, length)
            }
            ExprKind::Ident(name) => self.resolve(name, expr.id, expr.span),
            ExprKind::Unary { op, operand } => {
                let operand = self.expr(operand);

                match op {
                    UnOp::Negate | UnOp::Plus | UnOp::Not => operand.promoted(),
                    UnOp::PreIncrement | UnOp::PreDecrement => operand.promoted(),
                }
            }
            ExprKind::Binary { left, right, .. } => {
                let left = self.expr(left);
                let right = self.expr(right);

                Ty::common_arithmetic(&left, &right).unwrap_or(Ty::Int)
            }
            ExprKind::Assign { target, value } => {
                let target = self.expr(target);
                self.expr(value);

                target
            }
            ExprKind::Index { base, index } => {
                let base = self.expr(base);
                self.expr(index);

                match base {
                    Ty::Array(element, _) | Ty::Ptr(element) => *element,
                    _ => Ty::Int,
                }
            }
            ExprKind::Call { callee, args } => self.type_of_call(callee, args, expr.span),
            ExprKind::PostfixIncDec { operand, .. } => self.expr(operand).promoted(),
        }
    }

    /// Types a call, checking that the callee is a function and that the arity matches.
    fn type_of_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Ty {
        let callee_ty = self.expr(callee);

        for arg in args {
            self.expr(arg);
        }

        let Ty::Func { ret, params } = callee_ty else {
            return Ty::Int;
        };

        if params.len() != args.len() {
            let name = called_name(callee);
            let plural = if params.len() == 1 {
                "argument"
            } else {
                "arguments"
            };
            let passed = if args.len() == 1 { "was" } else { "were" };

            self.report(
                span,
                format!(
                    "{name} takes {} {plural}, but {} {passed} passed",
                    params.len(),
                    args.len()
                ),
            );
        }

        *ret
    }

    /// Resolves `name`, recording the binding, and returns what it was declared as.
    ///
    /// An unresolved name is reported and typed `int`, which is the type least likely to provoke a
    /// second error about the same mistake further up the expression.
    fn resolve(&mut self, name: &str, node: NodeId, span: Span) -> Ty {
        let Some(id) = self.scopes.lookup(name) else {
            self.report(span, format!("undeclared identifier '{name}'"));

            return Ty::Int;
        };

        self.annotations.record_binding(node, id);

        self.scopes
            .symbol(id)
            .map_or(Ty::Int, |symbol| symbol.ty.clone())
    }

    // -- Declarations ---------------------------------------------------------------------------

    /// Declares `decl` under `kind`, reporting a collision in the same scope.
    fn declare(&mut self, decl: &VarDecl, ty: Ty, kind: SymbolKind) {
        self.declare_name(&decl.name.text, ty, kind, decl.name.span, decl.id);
    }

    /// Declares `name` under `kind`, reporting a collision in the same scope.
    fn declare_name(
        &mut self,
        name: &str,
        ty: Ty,
        kind: SymbolKind,
        span: Span,
        node: NodeId,
    ) -> Option<SymbolId> {
        match self.scopes.declare(name, ty, kind, span) {
            Ok(id) => {
                self.annotations.record_binding(node, id);

                Some(id)
            }
            Err(_) => {
                self.report_redeclaration(name, span);

                None
            }
        }
    }

    /// Reports that `name` is declared twice in one scope.
    fn report_redeclaration(&mut self, name: &str, span: Span) {
        self.report_with_note(
            span,
            format!("redeclaration of '{name}' in this scope"),
            format!("previous declaration of '{name}' is here"),
        );
    }

    /// The type `spec` names, read according to where it was written.
    fn spec_type(&mut self, spec: &TypeSpec, position: DeclPosition) -> Ty {
        let base = match spec.base {
            BaseType::Int => Ty::Int,
            BaseType::Char => Ty::Char,
            BaseType::Void => Ty::Void,
        };

        if !spec.is_array() {
            return base;
        }

        // An array parameter is a pointer, however it was written: C passes the address, and the
        // length in `int a[10]` is not part of the parameter's type. This is ADR 0007's boundary.
        if position == DeclPosition::Parameter {
            return Ty::ptr(base);
        }

        match spec.array_len {
            Some(length) => Ty::array(base, length),
            None => base,
        }
    }

    // -- The depth guard ------------------------------------------------------------------------

    /// Charges one level of tree depth, reporting once and refusing to descend past the limit.
    ///
    /// The parser already bounds the trees it builds, so nothing coming through the pipeline
    /// reaches this. `analyze` is public, though, and Phase 5 fuzzes the front end, so the walk
    /// carries its own guard rather than trusting the pass before it.
    fn enter(&mut self, span: Span) -> bool {
        if self.depth >= MAX_NESTING_DEPTH {
            if !self.depth_reported {
                self.depth_reported = true;
                self.report(
                    span,
                    format!(
                        "nesting is too deep: the syntax tree goes at most {MAX_NESTING_DEPTH} levels deep"
                    ),
                );
            }

            return false;
        }

        self.depth += 1;

        true
    }

    /// Gives back the level [`Analyzer::enter`] charged.
    fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }
}

/// The name in `f(...)`, quoted, or a description when the callee is not a plain name.
fn called_name(callee: &Expr) -> String {
    match &callee.kind {
        ExprKind::Ident(name) => format!("'{name}'"),
        _ => "this call".to_owned(),
    }
}

/// The value `expr` folds to, or `None` if it is not a constant expression.
///
/// Only the forms a data-section initializer can hold: literals and arithmetic over them. A name,
/// a call, or an assignment is not constant however simple it looks.
fn constant_value(expr: &Expr) -> Option<i32> {
    use crate::ast::BinOp;

    match &expr.kind {
        ExprKind::IntLit(value) => Some(*value),
        ExprKind::CharLit(byte) => Some(i32::from(*byte)),
        ExprKind::Unary { op, operand } => {
            let operand = constant_value(operand)?;

            match op {
                UnOp::Plus => Some(operand),
                UnOp::Negate => operand.checked_neg(),
                UnOp::Not => Some(i32::from(operand == 0)),
                UnOp::PreIncrement | UnOp::PreDecrement => None,
            }
        }
        ExprKind::Binary { op, left, right } => {
            let left = constant_value(left)?;
            let right = constant_value(right)?;

            match op {
                BinOp::Add => left.checked_add(right),
                BinOp::Subtract => left.checked_sub(right),
                BinOp::Multiply => left.checked_mul(right),
                BinOp::Divide => left.checked_div(right),
                BinOp::Remainder => left.checked_rem(right),
                BinOp::Equal => Some(i32::from(left == right)),
                BinOp::NotEqual => Some(i32::from(left != right)),
                BinOp::Less => Some(i32::from(left < right)),
                BinOp::Greater => Some(i32::from(left > right)),
                BinOp::LessEqual => Some(i32::from(left <= right)),
                BinOp::GreaterEqual => Some(i32::from(left >= right)),
                BinOp::And => Some(i32::from(left != 0 && right != 0)),
                BinOp::Or => Some(i32::from(left != 0 || right != 0)),
            }
        }
        ExprKind::StrLit(_)
        | ExprKind::Ident(_)
        | ExprKind::Assign { .. }
        | ExprKind::Index { .. }
        | ExprKind::Call { .. }
        | ExprKind::PostfixIncDec { .. } => None,
    }
}

#[cfg(test)]
mod tests;
