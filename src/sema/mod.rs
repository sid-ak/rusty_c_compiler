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
    BaseType, BinOp, Block, Expr, ExprKind, ForInit, FuncSig, Initializer, Item, NodeId, Param,
    Program, Stmt, StmtKind, TypeSpec, UnOp, VarDecl,
};
use crate::diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, Span};
use crate::parser::MAX_NESTING_DEPTH;
use crate::sema::annotations::Annotations;
use crate::sema::scope::{Scopes, SymbolId, SymbolKind};
use crate::sema::types::{Assignability, Ty};

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
    /// What the function being walked returns.
    return_type: Ty,
    /// The name of the function being walked, for the message about reaching its end.
    function_name: String,
    /// How many loops enclose the statement being walked, so `break` can be placed.
    loop_depth: usize,
}

/// What an expression is being evaluated for, which decides what its type is allowed to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    /// An ordinary value.
    Value,
    /// The thing being called in `f(...)`, where naming a function is the point.
    Callee,
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
            return_type: Ty::Void,
            function_name: String::new(),
            loop_depth: 0,
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
        at: Span,
        note: impl Into<String>,
    ) {
        self.diagnostics
            .push(Diagnostic::new(DiagnosticKind::Semantic, span, message).with_note_at(at, note));
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
        self.check_return_type(signature);
        for param in &signature.params {
            self.check_parameter(param);
        }

        let ty = self.signature_type(signature);
        let name = &signature.name.text;

        if let Some(existing_id) = self.scopes.lookup_in_current_scope(name) {
            let Some(existing) = self.scopes.symbol(existing_id) else {
                return;
            };
            let previous = existing.span;
            let matches_kind = existing.kind == SymbolKind::Function;
            let matches_type = existing.ty == ty;

            if !matches_kind {
                self.report_redeclaration(name, signature.name.span, previous);
            } else if !matches_type {
                self.report_with_note(
                    signature.name.span,
                    format!("conflicting declaration of '{name}'"),
                    previous,
                    format!("previous declaration of '{name}' is here"),
                );
            } else if is_definition && self.defined.contains(name) {
                self.report_with_note(
                    signature.name.span,
                    format!("redefinition of '{name}'"),
                    previous,
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
        self.check_variable_type(&decl.ty, &decl.name.span);
        self.declare(decl, ty.clone(), SymbolKind::Global);

        let Some(init) = &decl.init else {
            return;
        };

        self.type_initializer(init, &ty);

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
        self.return_type = self.spec_type(&signature.return_type, DeclPosition::Variable);
        self.function_name = signature.name.text.clone();
        self.loop_depth = 0;
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
        self.check_reaches_the_end(signature, body);
    }

    /// Reports a non-`void` function whose control flow can run off its closing brace.
    ///
    /// `main` is excepted because C defines an implicit `return 0` for it. The judgement is made
    /// on the shapes that always return — a `return`, an `if`/`else` where both arms do, a loop
    /// that cannot end — rather than on what the last statement happens to be.
    fn check_reaches_the_end(&mut self, signature: &FuncSig, body: &Block) {
        let name = &signature.name.text;
        if self.return_type == Ty::Void || name == "main" {
            return;
        }

        if block_always_returns(body, 0) {
            return;
        }

        let closing_brace = Span::new(body.span.end.saturating_sub(1), body.span.end);
        self.report(
            closing_brace,
            format!("control reaches the end of non-void function '{name}'"),
        );
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
                self.condition(condition);
                self.stmt(then_branch);
                if let Some(branch) = else_branch {
                    self.stmt(branch);
                }
            }
            StmtKind::While { condition, body } => {
                self.condition(condition);
                self.loop_depth += 1;
                self.stmt(body);
                self.loop_depth -= 1;
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
                    self.condition(condition);
                }
                if let Some(step) = step {
                    self.expr(step);
                }

                self.loop_depth += 1;
                self.stmt(body);
                self.loop_depth -= 1;

                self.scopes.leave_block();
            }
            StmtKind::Return(Some(expr)) => {
                self.expr(expr);

                if self.return_type == Ty::Void {
                    self.report(
                        stmt.span,
                        "'return' with a value in a function returning 'void'",
                    );
                }
            }
            StmtKind::Return(None) => {
                if self.return_type != Ty::Void {
                    let returns = self.return_type.clone();
                    self.report(
                        stmt.span,
                        format!("'return' with no value in a function returning '{returns}'"),
                    );
                }
            }
            StmtKind::Expr(expr) => {
                self.expr(expr);
            }
            StmtKind::LocalVar(decl) => self.declare_local(decl),
            StmtKind::Break => self.check_inside_a_loop(stmt.span, "break"),
            StmtKind::Continue => self.check_inside_a_loop(stmt.span, "continue"),
            StmtKind::Empty => {}
        }

        self.leave();
    }

    /// Reports a `break` or a `continue` that has no loop to apply to.
    fn check_inside_a_loop(&mut self, span: Span, keyword: &str) {
        if self.loop_depth == 0 {
            self.report(span, format!("'{keyword}' outside of a loop"));
        }
    }

    /// Types an expression used as a condition, which has to be something testable for truth.
    fn condition(&mut self, expr: &Expr) {
        let ty = self.expr(expr);

        if !ty.is_scalar() {
            self.report(
                expr.span,
                format!("value of type '{ty}' is not a condition"),
            );
        }
    }

    /// Declares a local and types its initializer against the declared type.
    fn declare_local(&mut self, decl: &VarDecl) {
        let ty = self.spec_type(&decl.ty, DeclPosition::Variable);
        self.check_variable_type(&decl.ty, &decl.name.span);

        if let Some(init) = &decl.init {
            self.type_initializer(init, &ty);
        }

        self.declare(decl, ty, SymbolKind::Local);
    }

    /// Types every expression in `init` and checks that it fits what was declared.
    ///
    /// Only the length is checked here. An array is the one declared type an initializer can
    /// overrun, and a scalar's initializer is checked by the ordinary assignment rules.
    fn type_initializer(&mut self, init: &Initializer, declared: &Ty) {
        let written = match init {
            Initializer::Expr(expr) => {
                let ty = self.expr(expr);

                // A string literal initializing an array fills it, terminator included.
                match ty {
                    Ty::Array(_, length) if matches!(expr.kind, ExprKind::StrLit(_)) => {
                        Some((length, expr.span))
                    }
                    _ => None,
                }
            }
            Initializer::List { elements, span } => {
                for element in elements {
                    self.expr(element);
                }

                let count = u32::try_from(elements.len()).unwrap_or(u32::MAX);

                Some((count, *span))
            }
        };

        let (Some((written, span)), Ty::Array(_, declared)) = (written, declared) else {
            return;
        };

        if written > *declared {
            self.report(
                span,
                format!("{written} initializers for an array of {declared}"),
            );
        }
    }

    // -- Expressions ----------------------------------------------------------------------------

    /// Types one expression used as a value, records the result, and returns it.
    fn expr(&mut self, expr: &Expr) -> Ty {
        self.expr_used_as(expr, Use::Value)
    }

    /// Types one expression, records the result against its node, and returns it.
    fn expr_used_as(&mut self, expr: &Expr, used_as: Use) -> Ty {
        if !self.enter(expr.span) {
            self.annotations.record_type(expr.id, Ty::Int);

            return Ty::Int;
        }

        let ty = self.type_of(expr, used_as);
        self.annotations.record_type(expr.id, ty.clone());

        self.leave();

        ty
    }

    /// Works out what one expression's type is, without recording anything about the node itself.
    fn type_of(&mut self, expr: &Expr, used_as: Use) -> Ty {
        match &expr.kind {
            ExprKind::IntLit(_) => Ty::Int,
            ExprKind::CharLit(_) => Ty::Char,
            // A string literal is an array of `char` one longer than its text, for the terminator.
            ExprKind::StrLit(bytes) => {
                let length = u32::try_from(bytes.len().saturating_add(1)).unwrap_or(u32::MAX);

                Ty::array(Ty::Char, length)
            }
            ExprKind::Ident(name) => self.resolve(name, expr.id, expr.span, used_as),
            ExprKind::Unary { op, operand } => {
                let ty = self.expr(operand);

                match op {
                    UnOp::Not => {
                        if !ty.is_scalar() {
                            self.report(
                                operand.span,
                                format!("value of type '{ty}' is not a condition"),
                            );
                        }

                        Ty::Int
                    }
                    UnOp::Negate | UnOp::Plus | UnOp::PreIncrement | UnOp::PreDecrement => {
                        if !ty.is_arithmetic() {
                            self.report(
                                expr.span,
                                format!("invalid operand to unary '{}': '{ty}'", op.spelling()),
                            );
                        }

                        ty.promoted()
                    }
                }
            }
            ExprKind::Binary { op, left, right } => {
                let left_ty = self.expr(left);
                let right_ty = self.expr(right);

                self.check_binary(*op, &left_ty, &right_ty, expr.span)
            }
            ExprKind::Assign { target, value } => {
                let target_ty = self.expr(target);
                self.expr(value);

                if target_ty.decays() {
                    self.report(target.span, "array name is not assignable");
                }

                target_ty
            }
            ExprKind::Index { base, index } => {
                let base_ty = self.expr(base);
                let index_ty = self.expr(index);

                if !index_ty.is_arithmetic() {
                    self.report(index.span, "array subscript is not an integer");
                }

                match base_ty {
                    Ty::Array(element, _) | Ty::Ptr(element) => *element,
                    other => {
                        if !other.is_error() {
                            self.report(
                                expr.span,
                                "subscripted value is not an array or a pointer",
                            );
                        }

                        Ty::Error
                    }
                }
            }
            ExprKind::Call { callee, args } => self.type_of_call(callee, args, expr.span),
            ExprKind::PostfixIncDec { op, operand } => {
                let ty = self.expr(operand);

                if !ty.is_arithmetic() {
                    self.report(
                        expr.span,
                        format!("invalid operand to '{}': '{ty}'", op.spelling()),
                    );
                }

                ty.promoted()
            }
        }
    }

    /// The type of `op` applied to these operands, reporting operands it does not accept.
    ///
    /// `&&` and `||` take anything testable for truth, which is why they are separated out: they
    /// are condition contexts rather than arithmetic ones, and an array is rejected by both but
    /// with the message that fits where it was written.
    fn check_binary(&mut self, op: BinOp, left: &Ty, right: &Ty, span: Span) -> Ty {
        if matches!(op, BinOp::And | BinOp::Or) {
            for ty in [left, right] {
                if !ty.is_scalar() {
                    self.report(span, format!("value of type '{ty}' is not a condition"));
                }
            }

            return Ty::Int;
        }

        let Some(common) = Ty::common_arithmetic(left, right) else {
            // Either operand being the recovery type would have produced a type above, so
            // reaching here means both are real and genuinely do not go together.
            self.report(
                span,
                format!(
                    "invalid operands to binary '{}': '{left}' and '{right}'",
                    op.spelling()
                ),
            );

            return Ty::Int;
        };

        // A comparison answers a question about its operands rather than producing one of them.
        if matches!(
            op,
            BinOp::Equal
                | BinOp::NotEqual
                | BinOp::Less
                | BinOp::Greater
                | BinOp::LessEqual
                | BinOp::GreaterEqual
        ) {
            return Ty::Int;
        }

        common
    }

    /// Types a call, checking that the callee is a function and that the arity matches.
    fn type_of_call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Ty {
        let callee_ty = self.expr_used_as(callee, Use::Callee);

        let arg_types: Vec<Ty> = args.iter().map(|arg| self.expr(arg)).collect();

        let Ty::Func { ret, params } = callee_ty else {
            if !callee_ty.is_error() {
                self.report(span, "called object is not a function");
            }

            return Ty::Error;
        };

        if params.len() == args.len() {
            self.check_arguments(callee, &params, &arg_types, args);
        } else {
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

    /// Checks each argument against the parameter it is passed to.
    ///
    /// An array argument is compatible with a pointer parameter, which is the decay ADR 0007
    /// permits and the only place it happens.
    fn check_arguments(&mut self, callee: &Expr, params: &[Ty], arg_types: &[Ty], args: &[Expr]) {
        for (position, ((param, actual), arg)) in params.iter().zip(arg_types).zip(args).enumerate()
        {
            if Ty::assignability(param, actual) != Assignability::Incompatible {
                continue;
            }

            let name = called_name(callee);
            let ordinal = position.saturating_add(1);
            self.report(
                arg.span,
                format!(
                    "argument {ordinal} of {name} has type '{actual}', but '{param}' was expected"
                ),
            );
        }
    }

    /// Resolves `name`, recording the binding, and returns what it was declared as.
    ///
    /// An unresolved name is reported and typed `int`, which is the type least likely to provoke a
    /// second error about the same mistake further up the expression.
    fn resolve(&mut self, name: &str, node: NodeId, span: Span, used_as: Use) -> Ty {
        let Some(id) = self.scopes.lookup(name) else {
            self.report(span, format!("undeclared identifier '{name}'"));

            return Ty::Error;
        };

        self.annotations.record_binding(node, id);

        let ty = self
            .scopes
            .symbol(id)
            .map_or(Ty::Int, |symbol| symbol.ty.clone());

        // There are no function pointers in this subset, so a function's name is meaningful in
        // exactly one place. Anywhere else it is a mistake, and saying so here catches it once
        // rather than leaving each operator to complain about an operand it cannot use.
        if used_as == Use::Value && matches!(ty, Ty::Func { .. }) {
            self.report(
                span,
                format!("'{name}' is a function; it can only be called"),
            );

            return Ty::Error;
        }

        ty
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
            Err(existing) => {
                let previous = self
                    .scopes
                    .symbol(existing)
                    .map_or(span, |symbol| symbol.span);
                self.report_redeclaration(name, span, previous);

                None
            }
        }
    }

    /// Reports that `name` is declared twice in one scope, pointing at both sites.
    fn report_redeclaration(&mut self, name: &str, span: Span, previous: Span) {
        self.report_with_note(
            span,
            format!("redeclaration of '{name}' in this scope"),
            previous,
            format!("previous declaration of '{name}' is here"),
        );
    }

    /// Reports a variable whose written type cannot hold a value.
    fn check_variable_type(&mut self, spec: &TypeSpec, name: &Span) {
        self.check_written_type(spec, *name, "variable");
    }

    /// Reports a parameter whose written type cannot hold a value.
    fn check_parameter(&mut self, param: &Param) {
        self.check_written_type(&param.ty, param.name.span, "parameter");
    }

    /// Reports a function's return type if it is one a function cannot return.
    ///
    /// `void` is legal here and nowhere else, which is the whole reason the check is separate.
    fn check_return_type(&mut self, signature: &FuncSig) {
        if signature.return_type.is_array() {
            self.report(signature.name.span, "a function cannot return an array");
        }
    }

    /// Reports a written type that names no storage, at `span`, describing it as a `role`.
    ///
    /// The check reads the declaration as written rather than the type it produces, because an
    /// array parameter has already become a pointer by then and `void a[]` would look complete.
    fn check_written_type(&mut self, spec: &TypeSpec, span: Span, role: &str) {
        if spec.base == BaseType::Void {
            let message = if spec.is_array() {
                "array has incomplete element type 'void'".to_owned()
            } else {
                format!("{role} has incomplete type 'void'")
            };

            self.report(span, message);

            return;
        }

        if spec.array_len == Some(0) {
            self.report(span, "array size must be greater than zero");
        }
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

/// Whether every path through `block` ends in a `return`.
fn block_always_returns(block: &Block, depth: usize) -> bool {
    block
        .stmts
        .iter()
        .any(|stmt| always_returns(stmt, depth.saturating_add(1)))
}

/// Whether every path through `stmt` ends in a `return`.
///
/// Conservative in the one direction that matters: a shape it cannot reason about counts as not
/// returning, so the diagnostic is raised only where the analysis is sure. Past the nesting limit
/// it answers `true`, which suppresses the message rather than inventing one about a tree that has
/// already been reported as too deep.
fn always_returns(stmt: &Stmt, depth: usize) -> bool {
    if depth >= MAX_NESTING_DEPTH {
        return true;
    }

    let deeper = depth.saturating_add(1);

    match &stmt.kind {
        StmtKind::Return(_) => true,
        StmtKind::Block(block) => block_always_returns(block, deeper),
        StmtKind::If {
            then_branch,
            else_branch: Some(else_branch),
            ..
        } => always_returns(then_branch, deeper) && always_returns(else_branch, deeper),
        // A loop that cannot end never falls out of the bottom, so what follows it is unreachable
        // and the function cannot run off its closing brace. A `break` is the way out, and only
        // one belonging to this loop counts.
        StmtKind::While { condition, body } => {
            is_always_true(condition) && !escapes_the_loop(body, deeper)
        }
        StmtKind::For {
            condition, body, ..
        } => condition.as_ref().is_none_or(is_always_true) && !escapes_the_loop(body, deeper),
        StmtKind::If {
            else_branch: None, ..
        }
        | StmtKind::Break
        | StmtKind::Continue
        | StmtKind::Expr(_)
        | StmtKind::LocalVar(_)
        | StmtKind::Empty => false,
    }
}

/// Whether `stmt` contains a `break` that would leave the loop enclosing it.
///
/// A `break` inside a nested loop belongs to that loop, so the nested loop is not descended.
fn escapes_the_loop(stmt: &Stmt, depth: usize) -> bool {
    if depth >= MAX_NESTING_DEPTH {
        return false;
    }

    let deeper = depth.saturating_add(1);

    match &stmt.kind {
        StmtKind::Break => true,
        StmtKind::Block(block) => block
            .stmts
            .iter()
            .any(|stmt| escapes_the_loop(stmt, deeper)),
        StmtKind::If {
            then_branch,
            else_branch,
            ..
        } => {
            escapes_the_loop(then_branch, deeper)
                || else_branch
                    .as_deref()
                    .is_some_and(|branch| escapes_the_loop(branch, deeper))
        }
        StmtKind::While { .. }
        | StmtKind::For { .. }
        | StmtKind::Return(_)
        | StmtKind::Continue
        | StmtKind::Expr(_)
        | StmtKind::LocalVar(_)
        | StmtKind::Empty => false,
    }
}

/// Whether `expr` is a constant that is never zero, so a loop on it cannot end.
fn is_always_true(expr: &Expr) -> bool {
    constant_value(expr).is_some_and(|value| value != 0)
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
