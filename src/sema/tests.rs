//! Unit tests for the analyzer: the two passes, resolution, and multi-error reporting.
//!
//! The rules each check enforces are tested one by one in task 4's fixture suite. What is pinned
//! here is the walk itself — that the second pass sees what the first one collected, that an error
//! does not stop the walk, and that no tree can drive it off the stack.

use super::*;

use crate::ast::{
    Expr, ExprKind, ForInit, Initializer, Item, NodeIds, Program, Stmt, StmtKind, UnOp,
};
use crate::{lexer, parser};

/// Analyzes `source`, failing the test if it does not lex and parse cleanly first.
///
/// Analysis is only defined on a tree the earlier passes accepted, so a fixture that does not get
/// that far is a broken fixture rather than a finding.
fn analyze_source(source: &str) -> Analysis {
    let lexed = lexer::lex(source.as_bytes());
    assert!(
        lexed.diagnostics.is_empty(),
        "fixture does not lex: {:?}",
        lexed.diagnostics
    );

    let parsed = parser::parse(&lexed.tokens);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture does not parse: {:?}",
        parsed.diagnostics
    );

    analyze(&parsed.program)
}

/// The messages analysis reports for `source`, in the order it reports them.
fn messages(source: &str) -> Vec<String> {
    analyze_source(source)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Every expression node in `program`, in source order, collected independently of the analyzer.
///
/// Written out by hand rather than reusing the analyzer's own walk: a completeness check that
/// asked the implementation which nodes it visited would agree with itself no matter what it
/// missed.
fn expression_nodes(program: &Program) -> Vec<NodeId> {
    /// Appends `expr` and everything under it.
    fn walk_expr(expr: &Expr, found: &mut Vec<NodeId>) {
        found.push(expr.id);

        match &expr.kind {
            ExprKind::IntLit(_)
            | ExprKind::CharLit(_)
            | ExprKind::StrLit(_)
            | ExprKind::Ident(_) => {}
            ExprKind::Unary { operand, .. } | ExprKind::PostfixIncDec { operand, .. } => {
                walk_expr(operand, found);
            }
            ExprKind::Binary { left, right, .. } => {
                walk_expr(left, found);
                walk_expr(right, found);
            }
            ExprKind::Assign { target, value } => {
                walk_expr(target, found);
                walk_expr(value, found);
            }
            ExprKind::Index { base, index } => {
                walk_expr(base, found);
                walk_expr(index, found);
            }
            ExprKind::Call { callee, args } => {
                walk_expr(callee, found);
                for arg in args {
                    walk_expr(arg, found);
                }
            }
        }
    }

    /// Appends every expression in `init`.
    fn walk_init(init: &Initializer, found: &mut Vec<NodeId>) {
        match init {
            Initializer::Expr(expr) => walk_expr(expr, found),
            Initializer::List { elements, .. } => {
                for element in elements {
                    walk_expr(element, found);
                }
            }
        }
    }

    /// Appends every expression in `stmt` and everything under it.
    fn walk_stmt(stmt: &Stmt, found: &mut Vec<NodeId>) {
        match &stmt.kind {
            StmtKind::Block(block) => {
                for stmt in &block.stmts {
                    walk_stmt(stmt, found);
                }
            }
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                walk_expr(condition, found);
                walk_stmt(then_branch, found);
                if let Some(branch) = else_branch {
                    walk_stmt(branch, found);
                }
            }
            StmtKind::While { condition, body } => {
                walk_expr(condition, found);
                walk_stmt(body, found);
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
                            walk_init(init, found);
                        }
                    }
                    Some(ForInit::Expr(expr)) => walk_expr(expr, found),
                    None => {}
                }
                if let Some(condition) = condition {
                    walk_expr(condition, found);
                }
                if let Some(step) = step {
                    walk_expr(step, found);
                }
                walk_stmt(body, found);
            }
            StmtKind::Return(Some(expr)) | StmtKind::Expr(expr) => walk_expr(expr, found),
            StmtKind::LocalVar(decl) => {
                if let Some(init) = &decl.init {
                    walk_init(init, found);
                }
            }
            StmtKind::Return(None) | StmtKind::Break | StmtKind::Continue | StmtKind::Empty => {}
        }
    }

    let mut found = Vec::new();
    for item in &program.items {
        match item {
            Item::FuncDef(def) => {
                for stmt in &def.body.stmts {
                    walk_stmt(stmt, &mut found);
                }
            }
            Item::GlobalVar(decl) => {
                if let Some(init) = &decl.init {
                    walk_init(init, &mut found);
                }
            }
            Item::FuncDecl(_) => {}
        }
    }

    found
}

/// A program exercising every expression form, used where a test needs a realistic tree.
const REPRESENTATIVE: &str = r#"
void print_int(int n);
void print_string(char s[]);

int total;
char letters[4] = {'a', 'b', 'c', 0};

int sum(int values[], int count);

int main(void) {
    int accumulated = 0;
    for (int i = 0; i < 4; i = i + 1) {
        if (letters[i] != 0 && i < 3) {
            accumulated = accumulated + letters[i];
        } else {
            accumulated = accumulated - 1;
        }
    }

    while (accumulated > 0) {
        accumulated = accumulated - 1;
        if (accumulated == 2) {
            break;
        }
    }

    char greeting[6] = "hello";
    print_string(greeting);
    print_string("done");

    int values[3] = {1, 2, 3};
    total = -sum(values, 3) + !accumulated;
    accumulated++;
    print_int(total);

    return total;
}

int sum(int values[], int count) {
    int running = 0;
    for (int i = 0; i < count; i++) {
        running = running + values[i];
    }
    return running;
}
"#;

/// A call to a function defined further down the file resolves, which is what pass A is for.
#[test]
fn a_call_to_a_function_defined_later_resolves() {
    let source = "
int main(void) { return helper(); }
int helper(void) { return 7; }
";

    assert_eq!(messages(source), Vec::<String>::new());
}

/// A call to a name that is never declared anywhere is still rejected.
#[test]
fn a_call_to_an_undeclared_function_is_rejected() {
    let source = "int main(void) { return helper(); }";

    assert_eq!(
        messages(source),
        vec!["undeclared identifier 'helper'".to_owned()]
    );
}

/// Analysis keeps walking after an error, so four mistakes produce four messages in source order.
#[test]
fn four_errors_yield_exactly_four_diagnostics_in_source_order() {
    let source = "
int main(void) {
    int value;
    int value;
    missing = 1;
    value = alsoMissing;
    return stillMissing;
}
";

    assert_eq!(
        messages(source),
        vec![
            "redeclaration of 'value' in this scope".to_owned(),
            "undeclared identifier 'missing'".to_owned(),
            "undeclared identifier 'alsoMissing'".to_owned(),
            "undeclared identifier 'stillMissing'".to_owned(),
        ]
    );
}

/// A redeclaration points at the declaration it collides with as well as at itself.
#[test]
fn a_redeclaration_names_the_declaration_it_collides_with() {
    let source = "int main(void) { int value; int value; return 0; }";

    let diagnostics = analyze_source(source).diagnostics;
    let [diagnostic] = diagnostics.as_slice() else {
        panic!("expected exactly one diagnostic, got {diagnostics:?}");
    };

    assert_eq!(diagnostic.message, "redeclaration of 'value' in this scope");
    assert_eq!(
        diagnostic.notes,
        vec!["previous declaration of 'value' is here".to_owned()]
    );
}

/// A forward declaration followed by a matching definition is one function, not two.
#[test]
fn a_declaration_followed_by_a_matching_definition_is_accepted() {
    let source = "
int helper(int value);
int helper(int value) { return value; }
";

    assert_eq!(messages(source), Vec::<String>::new());
}

/// A definition that disagrees with an earlier declaration is rejected, whichever part differs.
#[test]
fn a_definition_disagreeing_with_its_declaration_is_rejected() {
    let cases = [
        (
            "return type",
            "int helper(int a);\nchar helper(int a) { return a; }",
        ),
        (
            "arity",
            "int helper(int a);\nint helper(int a, int b) { return a; }",
        ),
        (
            "parameter type",
            "int helper(int a);\nint helper(char a) { return a; }",
        ),
    ];

    for (differing, source) in cases {
        assert_eq!(
            messages(source),
            vec!["conflicting declaration of 'helper'".to_owned()],
            "differing {differing}"
        );
    }
}

/// Defining the same function twice is rejected even when the two signatures agree.
#[test]
fn two_definitions_of_one_function_are_rejected() {
    let source = "
int helper(void) { return 1; }
int helper(void) { return 2; }
";

    assert_eq!(
        messages(source),
        vec!["redefinition of 'helper'".to_owned()]
    );
}

/// A call has to pass as many arguments as the function declares.
#[test]
fn a_call_with_the_wrong_arity_is_rejected() {
    let source = "
int helper(int a, int b) { return a + b; }
int main(void) { return helper(1); }
";

    assert_eq!(
        messages(source),
        vec!["'helper' takes 2 arguments, but 1 was passed".to_owned()]
    );
}

/// A global initializer has to be a constant, since it is written into the data section.
#[test]
fn a_non_constant_global_initializer_is_rejected() {
    let source = "
int seed(void) { return 1; }
int value = seed();
";

    assert_eq!(
        messages(source),
        vec!["global initializer is not a constant".to_owned()]
    );
}

/// A constant expression folded from literals is a legal global initializer.
#[test]
fn a_folded_constant_global_initializer_is_accepted() {
    let source = "int value = -2 * 3 + 'a';";

    assert_eq!(messages(source), Vec::<String>::new());
}

/// A valid program produces no diagnostics at all.
#[test]
fn the_representative_program_is_accepted() {
    assert_eq!(messages(REPRESENTATIVE), Vec::<String>::new());
}

/// Every expression node in a valid program has a type recorded against it.
///
/// Asserted over the whole set rather than a sample: a count or a spot check would pass while the
/// one form the walk forgot went unrecorded, and code generation is what would find it.
#[test]
fn every_expression_node_of_a_valid_program_is_typed() {
    let lexed = lexer::lex(REPRESENTATIVE.as_bytes());
    let parsed = parser::parse(&lexed.tokens);
    let analysis = analyze(&parsed.program);

    let expected = expression_nodes(&parsed.program);
    assert!(!expected.is_empty(), "the fixture has no expressions");

    let untyped: Vec<NodeId> = expected
        .iter()
        .copied()
        .filter(|id| analysis.annotations.type_of(*id).is_none())
        .collect();

    assert_eq!(
        untyped,
        Vec::<NodeId>::new(),
        "expression nodes with no type"
    );
}

/// Every identifier in a valid program resolves to a symbol that analysis recorded.
#[test]
fn every_identifier_of_a_valid_program_is_bound() {
    let lexed = lexer::lex(REPRESENTATIVE.as_bytes());
    let parsed = parser::parse(&lexed.tokens);
    let analysis = analyze(&parsed.program);

    let mut identifiers = 0;
    for id in expression_nodes(&parsed.program) {
        if let Some(symbol) = analysis.annotations.binding_of(id) {
            assert!(
                analysis.annotations.symbol(symbol).is_some(),
                "binding for {id:?} names a symbol that is not in the table"
            );
            identifiers += 1;
        }
    }

    assert!(identifiers > 0, "the fixture has no identifiers");
}

/// An expression nested past the limit reports it rather than running off the stack.
///
/// The tree is built here rather than parsed, because the parser refuses to build one this deep.
/// That is exactly why the guard has to exist: `analyze` is public, and a caller reaching it
/// without going through the parser must get a diagnostic rather than a crash.
#[test]
fn a_tree_nested_past_the_limit_reports_the_limit_on_a_small_stack() {
    /// Half of what a test thread is given, which the guard has to fit inside comfortably.
    const SMALL_STACK: usize = 1024 * 1024;

    /// Far past the limit, and past what the analyzer's own frames would survive unguarded.
    ///
    /// Not larger, because dropping the tree at the end of the test descends it too, and that is
    /// the AST's recursion rather than the analyzer's.
    const NESTING: usize = 4_000;

    let reported = std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(|| {
            let lexed = lexer::lex(b"int main(void) { return 0; }");
            let parsed = parser::parse(&lexed.tokens);

            let mut ids = NodeIds::new();
            let program = deeply_nested_program(&parsed.program, &mut ids, NESTING);

            analyze(&program)
                .diagnostics
                .into_iter()
                .map(|diagnostic| diagnostic.message)
                .collect::<Vec<_>>()
        })
        .expect("could not spawn the thread")
        .join()
        .expect("overflowed a small stack, so the analyzer's depth guard is missing or too high");

    assert!(
        reported
            .iter()
            .any(|message| message.contains("nesting is too deep")),
        "got {reported:?}"
    );
}

/// `main` with `return` wrapped in `nesting` unary operators, deeper than any parser would build.
///
/// Built by moving the previous expression into the new one rather than cloning it, because
/// cloning a tree this deep would itself descend it and defeat the point of the test.
fn deeply_nested_program(template: &Program, ids: &mut NodeIds, nesting: usize) -> Program {
    let mut program = template.clone();

    let Some(Item::FuncDef(def)) = program.items.first_mut() else {
        panic!("the template's first item is the function to deepen");
    };
    let Some(stmt) = def.body.stmts.first_mut() else {
        panic!("the template's function has a body statement to deepen");
    };
    let StmtKind::Return(Some(expr)) = &mut stmt.kind else {
        panic!("the template's body statement is a `return` with a value");
    };

    for _ in 0..nesting {
        let span = expr.span;
        let placeholder = Expr {
            id: ids.next_id(),
            kind: ExprKind::IntLit(0),
            span,
        };
        let inner = std::mem::replace(expr, placeholder);

        *expr = Expr {
            id: ids.next_id(),
            kind: ExprKind::Unary {
                op: UnOp::Negate,
                operand: Box::new(inner),
            },
            span,
        };
    }

    program
}
