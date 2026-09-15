//! Unit tests for declarations, statements, error recovery, and the nesting limit.

use super::*;

use crate::ast::{self, Spans};
use crate::lexer;

/// Parse `source`, asserting it lexes and parses cleanly, and return its AST dump.
fn dump(source: &str) -> String {
    let lexed = lexer::lex(source.as_bytes());
    assert!(
        lexed.diagnostics.is_empty(),
        "expected {source:?} to lex cleanly, got: {:?}",
        lexed.diagnostics
    );

    let parsed = parse(&lexed.tokens);
    assert!(
        parsed.diagnostics.is_empty(),
        "expected {source:?} to parse cleanly, got: {:?}",
        parsed.diagnostics
    );

    ast::dump(&parsed.program, Spans::Hidden)
}

/// Parse `source` and return its dump collapsed onto one line.
///
/// Indentation is what makes a whole-program dump readable and what makes a one-construct
/// assertion unreadable, so structural tests below compare the shape and nothing else. Only
/// safe because none of them contain a string literal with a space in it.
fn shape(source: &str) -> String {
    dump(source)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse `source` and return the messages it complained about, in source order.
fn errors(source: &str) -> Vec<String> {
    let lexed = lexer::lex(source.as_bytes());
    assert!(
        lexed.diagnostics.is_empty(),
        "expected {source:?} to lex cleanly, got: {:?}",
        lexed.diagnostics
    );

    parse(&lexed.tokens)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Assert `source` produces exactly one diagnostic, reading `message`.
fn assert_one_error(source: &str, message: &str) {
    assert_eq!(errors(source), [message.to_string()], "for {source:?}");
}

/// A program spanning most of the grammar, for the whole-tree properties below.
const BROAD: &str = "int total;
     int add(int a, int b);
     int add(int a, int b) { return a + b; }
     int main(void) {
         int values[2] = {1, 2};
         for (int i = 0; i < 2; i = i + 1) {
             if (values[i] > 0) { total = add(total, values[i]++); } else { continue; }
         }
         while (!total) { break; }
         return total;
     }";

/// Every node the parser builds gets an identity no other node has, and `ast::nodes` lists
/// every one of them. Phase 3 keys its annotations by these ids, so a collision would give two
/// nodes the same type and an omission would leave a node with none.
///
/// Uniqueness alone cannot see an omission — dropping a node never makes two ids collide. Ids
/// are handed out 0, 1, 2, … and a clean parse keeps every node it allocates one for, so the
/// listed ids must be exactly `0..n` with no gap, which checks both properties at once against
/// a count the dump walk does not produce itself.
#[test]
fn the_parser_lists_every_node_it_built_exactly_once() {
    let lexed = lexer::lex(BROAD.as_bytes());
    let parsed = parse(&lexed.tokens);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let nodes = ast::nodes(&parsed.program);
    let mut ids: Vec<u32> = nodes.iter().map(|(id, _)| id.index()).collect();
    ids.sort_unstable();
    let expected: Vec<u32> = (0..).take(ids.len()).collect();

    assert!(ids.len() > 40, "expected a program worth walking");
    assert_eq!(ids, expected, "ids repeat or are missing from: {nodes:?}");
}

/// Every span the parser records is a real range inside the file it came from.
#[test]
fn every_span_points_into_the_source() {
    let lexed = lexer::lex(BROAD.as_bytes());
    let parsed = parse(&lexed.tokens);

    for (id, span) in ast::nodes(&parsed.program) {
        assert!(span.start <= span.end, "reversed span on {id:?}");
        assert!(span.end <= BROAD.len(), "span past the source on {id:?}");
        assert!(!span.is_empty(), "empty span on {id:?}");
    }
}

/// An empty file is a valid translation unit with nothing in it.
#[test]
fn an_empty_file_is_an_empty_program() {
    assert_eq!(dump(""), "(program)\n");
}

/// A function definition carries its return type, name, parameters, and body.
#[test]
fn a_function_definition_parses() {
    assert_eq!(
        dump("int main(void) { return 0; }"),
        concat!(
            "(program\n",
            "  (func-def int main\n",
            "    (params)\n",
            "    (block\n",
            "      (return\n",
            "        (int-lit 0)))))\n",
        )
    );
}

/// An empty parameter list and an explicit `(void)` mean the same thing.
#[test]
fn empty_and_void_parameter_lists_agree() {
    assert_eq!(dump("int f() { }"), dump("int f(void) { }"));
}

/// A declaration without a body is a forward declaration, and may be followed by the
/// definition it promised.
#[test]
fn a_forward_declaration_precedes_its_definition() {
    assert_eq!(
        shape("int f(int n); int f(int n) { return n; }"),
        "(program (func-decl int f (params (param int n))) \
         (func-def int f (params (param int n)) (block (return (ident n)))))"
    );
}

/// Zero through nine parameters parse. Nine matters because the ninth crosses the
/// eight-register boundary the ABI draws in Phase 4.
#[test]
fn functions_take_zero_through_nine_parameters() {
    for count in 0..=9 {
        let params: Vec<String> = (0..count).map(|index| format!("int p{index}")).collect();
        let source = format!("int f({}) {{ return 0; }}", params.join(", "));

        let dumped = dump(&source);
        assert_eq!(
            dumped.matches("(param ").count(),
            count,
            "for {count} parameters"
        );
    }
}

/// Every declaration form the grammar allows parses to the shape it describes.
#[test]
fn every_declaration_form_parses() {
    let cases = [
        ("int n;", "(program (global-var int n))"),
        (
            "int n = 1;",
            "(program (global-var int n (init (int-lit 1))))",
        ),
        ("int a[3];", "(program (global-var int[3] a))"),
        (
            "int a[3] = {1, 2, 3};",
            "(program (global-var int[3] a (init-list (int-lit 1) (int-lit 2) (int-lit 3))))",
        ),
        (
            "int a[1] = {};",
            "(program (global-var int[1] a (init-list)))",
        ),
        (
            "int a[2] = {1, 2,};",
            "(program (global-var int[2] a (init-list (int-lit 1) (int-lit 2))))",
        ),
        (
            "char c = 'x';",
            r"(program (global-var char c (init (char-lit 'x'))))",
        ),
        (
            "void f(int a[]) { }",
            "(program (func-def void f (params (param int[] a)) (block)))",
        ),
    ];

    for (source, expected) in cases {
        assert_eq!(shape(source), expected, "for {source:?}");
    }
}

/// Every statement form the grammar allows parses to the shape it describes.
#[test]
fn every_statement_form_parses() {
    let cases = [
        ("{ }", "(block)"),
        ("{ { } }", "(block (block))"),
        (";", "(empty)"),
        ("x;", "(expr-stmt (ident x))"),
        ("int y;", "(decl-stmt (local-var int y))"),
        ("return;", "(return)"),
        ("return 1;", "(return (int-lit 1))"),
        ("break;", "(break)"),
        ("continue;", "(continue)"),
        ("if (a) b;", "(if (ident a) (then (expr-stmt (ident b))))"),
        (
            "if (a) b; else c;",
            "(if (ident a) (then (expr-stmt (ident b))) (else (expr-stmt (ident c))))",
        ),
        ("while (a) b;", "(while (ident a) (expr-stmt (ident b)))"),
    ];

    for (source, expected) in cases {
        let wrapped = format!("void f(void) {{ {source} }}");
        let expected = format!("(program (func-def void f (params) (block {expected})))");

        assert_eq!(shape(&wrapped), expected, "for {source:?}");
    }
}

/// A body may be a single statement without braces, at any of the three loop and branch forms.
#[test]
fn bodies_may_be_a_single_unbraced_statement() {
    for source in ["if (a) b;", "while (a) b;", "for (;;) b;"] {
        let wrapped = format!("void f(void) {{ {source} }}");

        assert!(!shape(&wrapped).contains("(block (block"), "for {source:?}");
    }
}

/// A dangling `else` binds to the nearest `if`, checked three deep so a rule that happened to
/// work at two levels does not pass by luck.
#[test]
fn a_dangling_else_binds_to_the_nearest_if() {
    let shaped = shape("void f(void) { if (a) if (b) if (c) x; else y; }");

    assert_eq!(
        shaped,
        "(program (func-def void f (params) (block \
         (if (ident a) (then \
         (if (ident b) (then \
         (if (ident c) (then (expr-stmt (ident x))) (else (expr-stmt (ident y)))))))))))"
    );
}

/// All eight combinations of a present or absent `for` clause parse, each keeping its own slot
/// in the dump so an omitted clause cannot be mistaken for a shifted one.
#[test]
fn every_combination_of_for_clauses_parses() {
    for combination in 0..8u8 {
        let init = if combination & 1 == 0 { "" } else { "i = 0" };
        let condition = if combination & 2 == 0 { "" } else { "i < 3" };
        let step = if combination & 4 == 0 {
            ""
        } else {
            "i = i + 1"
        };
        let source = format!("void f(void) {{ for ({init}; {condition}; {step}) x; }}");

        let shaped = shape(&source);
        assert_eq!(
            shaped.contains("(init (assign"),
            !init.is_empty(),
            "init, for {source:?}: {shaped}"
        );
        assert_eq!(
            shaped.contains("(cond (binary"),
            !condition.is_empty(),
            "condition, for {source:?}: {shaped}"
        );
        assert_eq!(
            shaped.contains("(step (assign"),
            !step.is_empty(),
            "step, for {source:?}: {shaped}"
        );
    }
}

/// A `for` initializer may declare its own variable.
#[test]
fn a_for_initializer_may_declare() {
    assert!(
        shape("void f(void) { for (int i = 0; i < 3; i = i + 1) x; }")
            .contains("(init (local-var int i (init (int-lit 0))))")
    );
}

/// A declaration is a block item, not a statement, so it may not be a branch or loop body.
#[test]
fn a_declaration_may_not_be_a_branch_body() {
    for source in [
        "void f(void) { if (a) int y = 1; }",
        "void f(void) { while (a) int y = 1; }",
        "void f(void) { for (;;) int y = 1; }",
    ] {
        assert_one_error(source, "a declaration is not allowed here");
    }
}

/// Each syntax error names what was wanted and what was there, in source spelling.
#[test]
fn syntax_errors_name_the_token_they_wanted() {
    let cases = [
        ("int f(void) { return 1 }", "expected ';', found '}'"),
        ("int f(void) { g(1; }", "expected ')', found ';'"),
        ("int f(void) { if x; }", "expected '(', found 'x'"),
        (
            "int f(void) { if () x; }",
            "expected an expression, found ')'",
        ),
        ("int f(void) { ; ", "expected '}', found end of file"),
        ("int 1;", "expected an identifier, found '1'"),
        ("1;", "expected a type, found '1'"),
        (
            "int f(void) { int a[] = {1}; }",
            "expected an integer literal for the array length",
        ),
        (
            "void f(int a[3]) { }",
            "an array parameter may not declare a length",
        ),
        ("int f(void) { else x; }", "'else' without a matching 'if'"),
    ];

    for (source, message) in cases {
        assert_one_error(source, message);
    }
}

/// A keyword where a name belongs is reported as the keyword it is.
#[test]
fn a_keyword_used_as_an_identifier_is_rejected() {
    assert_one_error("int while;", "expected an identifier, found 'while'");
    assert_one_error(
        "int f(void) { int return; }",
        "expected an identifier, found 'return'",
    );
}

/// Two independent mistakes in one function produce two diagnostics, not a cascade.
#[test]
fn two_errors_produce_exactly_two_diagnostics() {
    assert_eq!(
        errors("int f(void) { int a = ; int b = 2; int c = ; return b; }"),
        [
            "expected an expression, found ';'",
            "expected an expression, found ';'",
        ]
    );
}

/// Recovery resumes at the next statement, so what follows a mistake still parses.
#[test]
fn parsing_resumes_after_a_bad_statement() {
    let lexed = lexer::lex(b"int f(void) { int a = ; return 7; }");
    let parsed = parse(&lexed.tokens);

    assert_eq!(parsed.diagnostics.len(), 1);
    assert!(
        ast::dump(&parsed.program, Spans::Hidden).contains("(return\n        (int-lit 7))"),
        "the statement after the mistake should still be in the tree"
    );
}

/// A mistake in one function does not consume the next one.
///
/// The missing semicolon leaves recovery staring at the `}` that closes `f`. Leaving that
/// brace where it is, rather than skipping it as part of the wreckage, is what keeps `g`.
#[test]
fn recovery_does_not_swallow_the_following_function() {
    let lexed = lexer::lex(b"int f(void) { return 1 } int g(void) { return 1; }");
    let parsed = parse(&lexed.tokens);

    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(
        parsed.program.items.len(),
        2,
        "both functions should survive: {:?}",
        ast::dump(&parsed.program, Spans::Hidden)
    );
}

/// A brace left open at the end of the file is reported once, not once per line after it.
#[test]
fn an_unbalanced_brace_at_eof_reports_once() {
    assert_one_error(
        "int f(void) { int a = 1;",
        "expected '}', found end of file",
    );
}

/// Recovery cannot loop: a file of nothing but closing parentheses ends, having complained a
/// bounded number of times.
#[test]
fn a_file_of_closing_parentheses_terminates() {
    let messages = errors(")))))))))))))))))))))))))))))))");

    assert!(!messages.is_empty());
    assert!(
        messages.len() < 5,
        "expected a bounded report, got {messages:?}"
    );
}

/// A file of nothing but braces ends too, whichever way they are unbalanced.
#[test]
fn a_file_of_braces_terminates() {
    for source in ["{{{{{{", "}}}}}}", "{}{}{}", "{ } } { {"] {
        let lexed = lexer::lex(source.as_bytes());
        let parsed = parse(&lexed.tokens);

        assert!(!parsed.diagnostics.is_empty(), "for {source:?}");
    }
}

/// Every C construct the subset leaves out is named as such, rather than reported as a
/// program that does not parse.
#[test]
fn unsupported_constructs_are_named() {
    let cases = [
        ("struct point p;", "unsupported in this C subset: 'struct'"),
        ("union u v;", "unsupported in this C subset: 'union'"),
        ("enum color c;", "unsupported in this C subset: 'enum'"),
        (
            "typedef int word;",
            "unsupported in this C subset: 'typedef'",
        ),
        (
            "int f(void) { switch (n) { } }",
            "unsupported in this C subset: 'switch'",
        ),
        (
            "int f(void) { do { } while (n); }",
            "unsupported in this C subset: 'do'",
        ),
        (
            "int f(void) { return sizeof(n); }",
            "unsupported in this C subset: 'sizeof'",
        ),
        ("float x;", "unsupported in this C subset: 'float'"),
        ("double x;", "unsupported in this C subset: 'double'"),
        ("long x;", "unsupported in this C subset: 'long'"),
        ("unsigned x;", "unsupported in this C subset: 'unsigned'"),
        ("short x;", "unsupported in this C subset: 'short'"),
        ("signed x;", "unsupported in this C subset: 'signed'"),
        ("const int x = 1;", "unsupported in this C subset: 'const'"),
        ("static int x;", "unsupported in this C subset: 'static'"),
        ("extern int x;", "unsupported in this C subset: 'extern'"),
        (
            "int *p;",
            "unsupported in this C subset: pointer declarators",
        ),
        (
            "int f(char *s) { return 0; }",
            "unsupported in this C subset: pointer declarators",
        ),
    ];

    for (source, message) in cases {
        let messages = errors(source);
        assert_eq!(
            messages.first().map(String::as_str),
            Some(message),
            "for {source:?}, got {messages:?}"
        );
    }
}

/// A type definition is one diagnostic, not one for the keyword and another for the `};` left
/// behind after recovery skipped its body.
#[test]
fn an_out_of_subset_type_definition_reports_once() {
    for source in [
        "struct point { int x; int y; };",
        "union value { int i; char c; };",
        "enum color { red, green };",
        "typedef int word;",
    ] {
        assert_eq!(errors(source).len(), 1, "for {source:?}");
    }
}

/// A definition the subset lacks does not take the declarations after it down with it.
#[test]
fn parsing_resumes_after_an_unsupported_definition() {
    let lexed = lexer::lex(b"struct point { int x; };\nint main(void) { return 0; }");
    let parsed = parse(&lexed.tokens);

    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(parsed.program.items.len(), 1, "main should still parse");
}

/// A function whose body nests `depth` levels deep in each way the grammar lets a tree grow
/// deeper, named for the failure message.
///
/// Recursion is only half of it. Parentheses, blocks, prefix operators, and assignment deepen
/// the tree by recursing, but a left-associative operator chain like `1 + 1 + 1` and a postfix
/// chain like `a[0][0][0]` deepen it inside a loop, with the parser's own call depth staying
/// flat. Every pass after the parser walks the tree recursively — the dump, `Drop`, and later
/// analysis and code generation — so each shape has to meet the same limit.
fn deep_programs(depth: usize) -> [(&'static str, String); 8] {
    let body = |statement: String| format!("int f(void) {{ {statement} }}");

    [
        (
            "parentheses",
            body(format!(
                "return {}1{};",
                "(".repeat(depth),
                ")".repeat(depth)
            )),
        ),
        (
            "blocks",
            body(format!("{}{}", "{".repeat(depth), "}".repeat(depth))),
        ),
        (
            "prefix operators",
            body(format!("return {}1;", "!".repeat(depth))),
        ),
        ("assignments", body(format!("{}1;", "a = ".repeat(depth)))),
        (
            "an operator chain",
            body(format!("return 1{};", "+1".repeat(depth))),
        ),
        (
            "an index chain",
            body(format!("return a{};", "[0]".repeat(depth))),
        ),
        (
            "a call chain",
            body(format!("return f{};", "(0)".repeat(depth))),
        ),
        (
            "an increment chain",
            body(format!("return a{};", "++".repeat(depth))),
        ),
    ]
}

/// Whether `messages` opens with the depth-limit diagnostic.
fn meets_the_depth_limit(messages: &[String]) -> bool {
    messages
        .first()
        .is_some_and(|message| message.starts_with("nesting is too deep"))
}

/// Every way of deepening the tree past the limit is a diagnostic rather than a stack overflow.
#[test]
fn every_deep_shape_meets_the_depth_limit() {
    for (shape, source) in deep_programs(MAX_NESTING_DEPTH * 4) {
        let messages = errors(&source);

        assert!(
            meets_the_depth_limit(&messages),
            "{shape}: got {messages:?}"
        );
    }
}

/// Everything done with a parsed tree fits on a stack far smaller than any the compiler runs
/// on, however deep the input tried to make it.
///
/// The point of the guard is that no input can exhaust the stack, and a limit tuned so finely
/// that it only just fits would move the crash rather than remove it. So each deep shape is
/// parsed, dumped, and dropped inside a thread given half of what the test harness hands out
/// by default, at a size — tens of thousands of levels — that overflows any walk the limit
/// fails to bound. Parsing alone is not enough to check: a loop can build a tree far deeper
/// than the parser ever recursed, and it is the dump and the drop that then descend it.
#[test]
fn deep_input_stays_within_a_small_stack() {
    /// Half the 2 MiB a test thread gets, and an eighth of the binary's main stack — about
    /// twice what the costliest shape, nested blocks, was measured to need at the limit.
    const SMALL_STACK: usize = 1024 * 1024;

    for (shape, source) in deep_programs(50_000) {
        let messages_on_a_small_stack = std::thread::Builder::new()
            .stack_size(SMALL_STACK)
            .spawn(move || {
                let lexed = lexer::lex(source.as_bytes());
                let parsed = parse(&lexed.tokens);
                ast::dump(&parsed.program, Spans::Hidden);

                parsed
                    .diagnostics
                    .into_iter()
                    .map(|diagnostic| diagnostic.message)
                    .collect::<Vec<_>>()
            })
            .expect("could not spawn the thread")
            .join()
            .expect("overflowed a small stack, so the depth limit is set too high");

        assert!(
            meets_the_depth_limit(&messages_on_a_small_stack),
            "{shape}: got {messages_on_a_small_stack:?}"
        );
    }
}

/// Every shape nested inside the limit still parses, so the guard rejects only what it must.
#[test]
fn every_shape_within_the_limit_parses() {
    for (shape, source) in deep_programs(MAX_NESTING_DEPTH / 4) {
        let messages = errors(&source);

        assert!(messages.is_empty(), "{shape}: got {messages:?}");
    }
}

/// Meeting the limit costs nothing afterwards: the levels a rejected or finished construct used
/// are all given back, so what follows it is judged from the depth it is actually at.
///
/// A loop that counted its levels up but did not count them back down on every way out —
/// including bailing out with a diagnostic — would make each later chain look deeper than it is,
/// until ordinary code started meeting the limit.
#[test]
fn the_depth_limit_charges_nothing_to_what_follows() {
    let allowed = MAX_NESTING_DEPTH / 4;
    let chain = format!("x = 1{};", "+1".repeat(allowed));
    let rejected = format!("return 1{};", "+1".repeat(MAX_NESTING_DEPTH * 4));
    let source = format!(
        "int f(void) {{ {rejected} }} int g(void) {{ {} {rejected} {} }}",
        chain.repeat(MAX_NESTING_DEPTH),
        chain.repeat(MAX_NESTING_DEPTH),
    );

    let messages = errors(&source);
    assert_eq!(
        messages.len(),
        2,
        "expected only the two over-long chains, got {messages:?}"
    );
}

/// A chain whose operands are chains of their own is charged for its depth, not its length.
///
/// In `1 * 1 + 1 * 1 + …` each `*` is one level below its `+`, so the tree is only one level
/// deeper than the `+` chain alone. If the levels a right operand charged were not given back
/// when it finished, they would pile up along the `+` chain and reject this well before its
/// real depth reaches the limit.
#[test]
fn an_operand_gives_back_its_levels_to_the_chain_around_it() {
    let terms = MAX_NESTING_DEPTH * 3 / 4;
    let source = format!(
        "int f(void) {{ return 1 * 1{}; }}",
        " + 1 * 1".repeat(terms)
    );

    assert_eq!(errors(&source), Vec::<String>::new());
}

/// An empty token slice is a valid, empty parse rather than an out-of-bounds read. The Phase 5
/// fuzz targets can hand the parser one, so it may not assume the lexer's trailing `Eof`.
#[test]
fn an_empty_token_slice_parses() {
    let parsed = parse(&[]);

    assert!(parsed.program.items.is_empty());
    assert!(parsed.diagnostics.is_empty());
}
