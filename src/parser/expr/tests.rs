//! Unit tests for expression parsing: precedence, associativity, and the postfix chain.

use super::*;

use crate::ast::{self, Spans};
use crate::lexer;

/// Parse `source` as a single expression, asserting it consumed all of it cleanly, and render
/// its tree on one line.
///
/// One line, because precedence and associativity are questions about shape and nothing else;
/// the indentation that makes a whole-program dump readable only gets in the way here.
fn shape(source: &str) -> String {
    let lexed = lexer::lex(source.as_bytes());
    assert!(
        lexed.diagnostics.is_empty(),
        "expected {source:?} to lex cleanly, got: {:?}",
        lexed.diagnostics
    );

    let mut parser = Parser::new(&lexed.tokens);
    let expr = parser
        .expression()
        .unwrap_or_else(|Bail| panic!("expected {source:?} to parse as one expression"));

    assert!(
        parser.diagnostics.is_empty(),
        "expected {source:?} to parse cleanly, got: {:?}",
        parser.diagnostics
    );
    assert!(
        parser.at_eof(),
        "expected {source:?} to be one whole expression"
    );

    ast::dump_expression(&expr, Spans::Hidden)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse `source` as an expression and return what it complained about.
fn errors(source: &str) -> Vec<String> {
    let lexed = lexer::lex(source.as_bytes());
    assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);

    let mut parser = Parser::new(&lexed.tokens);
    let _ = parser.expression();

    parser
        .diagnostics
        .clone()
        .into_sorted()
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Every primary form parses to itself.
#[test]
fn primaries_parse_to_themselves() {
    assert_eq!(shape("42"), "(int-lit 42)");
    assert_eq!(shape("0xff"), "(int-lit 255)");
    assert_eq!(shape("'a'"), "(char-lit 'a')");
    assert_eq!(shape(r#""hi""#), r#"(str-lit "hi")"#);
    assert_eq!(shape("total"), "(ident total)");
}

/// Precedence and associativity, asserted on the tree rather than on a result: `1+2*3 == 7`
/// would also hold if precedence were wrong in a way that cancelled out.
#[test]
fn precedence_and_associativity_match_c() {
    let cases = [
        (
            "1+2*3",
            "(binary + (int-lit 1) (binary * (int-lit 2) (int-lit 3)))",
        ),
        (
            "1*2+3",
            "(binary + (binary * (int-lit 1) (int-lit 2)) (int-lit 3))",
        ),
        (
            "1-2-3",
            "(binary - (binary - (int-lit 1) (int-lit 2)) (int-lit 3))",
        ),
        (
            "1/2/3",
            "(binary / (binary / (int-lit 1) (int-lit 2)) (int-lit 3))",
        ),
        ("a=b=c", "(assign (ident a) (assign (ident b) (ident c)))"),
        ("-x*y", "(binary * (unary - (ident x)) (ident y))"),
        ("!a&&b", "(binary && (unary ! (ident a)) (ident b))"),
        (
            "a||b&&c",
            "(binary || (ident a) (binary && (ident b) (ident c)))",
        ),
        (
            "a<b==c<d",
            "(binary == (binary < (ident a) (ident b)) (binary < (ident c) (ident d)))",
        ),
        (
            "a%b*c",
            "(binary * (binary % (ident a) (ident b)) (ident c))",
        ),
        (
            "a[i]+1",
            "(binary + (index (ident a) (ident i)) (int-lit 1))",
        ),
        (
            "f(x)*2",
            "(binary * (call (ident f) (args (ident x))) (int-lit 2))",
        ),
        (
            "-f(x)[i]",
            "(unary - (index (call (ident f) (args (ident x))) (ident i)))",
        ),
        ("1+ +2", "(binary + (int-lit 1) (unary + (int-lit 2)))"),
        ("1++ +2", "(binary + (postfix ++ (int-lit 1)) (int-lit 2))"),
        (
            "a<=b>=c",
            "(binary >= (binary <= (ident a) (ident b)) (ident c))",
        ),
        (
            "a!=b||c==d",
            "(binary || (binary != (ident a) (ident b)) (binary == (ident c) (ident d)))",
        ),
    ];

    for (source, expected) in cases {
        assert_eq!(shape(source), expected, "for {source:?}");
    }
}

/// Prefix operators are right-associative, so they nest outermost-first.
#[test]
fn prefix_operators_are_right_associative() {
    assert_eq!(shape("- -x"), "(unary - (unary - (ident x)))");
    assert_eq!(shape("!!a"), "(unary ! (unary ! (ident a)))");
    assert_eq!(shape("++--a"), "(unary ++ (unary -- (ident a)))");
}

/// The postfix operators chain in any combination, from one loop with no case for either.
#[test]
fn postfix_operators_chain() {
    assert_eq!(shape("a[i]++"), "(postfix ++ (index (ident a) (ident i)))");
    assert_eq!(
        shape("f(x)[0]"),
        "(index (call (ident f) (args (ident x))) (int-lit 0))"
    );
    assert_eq!(
        shape("a[i][j]"),
        "(index (index (ident a) (ident i)) (ident j))"
    );
    assert_eq!(shape("a--"), "(postfix -- (ident a))");
}

/// Call arguments parse at assignment precedence, so an assignment inside one is an argument
/// rather than a separator.
#[test]
fn call_arguments_parse_at_assignment_precedence() {
    assert_eq!(shape("f()"), "(call (ident f) (args))");
    assert_eq!(
        shape("f(a, b = c)"),
        "(call (ident f) (args (ident a) (assign (ident b) (ident c))))"
    );
    assert_eq!(
        shape("f(a + b, c)"),
        "(call (ident f) (args (binary + (ident a) (ident b)) (ident c)))"
    );
}

/// Parentheses group and then vanish, so a deeply parenthesized expression is the same tree as
/// the expression inside it.
#[test]
fn parentheses_leave_no_trace() {
    assert_eq!(shape("((((1))))"), shape("1"));
    assert_eq!(
        shape("(a+b)*c"),
        "(binary * (binary + (ident a) (ident b)) (ident c))"
    );
    assert_eq!(shape("(a) = 1"), "(assign (ident a) (int-lit 1))");
}

/// Only a variable or an array element can be written to; anything else is not an assignment
/// that is wrong, it is not an assignment.
#[test]
fn only_syntactic_lvalues_may_be_assigned_to() {
    for source in ["1 = 2", "f() = 1", "(a + b) = 1", "a++ = 1"] {
        assert_eq!(
            errors(source),
            ["expression is not assignable".to_string()],
            "for {source:?}"
        );
    }

    for source in ["a = 1", "a[i] = 1", "(a) = 1", "(a[i]) = 1"] {
        let _ = shape(source);
    }
}

/// An operator this subset omits that lexes as two tokens is named, rather than reported as a
/// stray second half.
#[test]
fn operators_spelled_as_two_tokens_are_named() {
    let cases = [
        ("a += b", "unsupported in this C subset: '+='"),
        ("a -= b", "unsupported in this C subset: '-='"),
        ("a *= b", "unsupported in this C subset: '*='"),
        ("a /= b", "unsupported in this C subset: '/='"),
        ("a %= b", "unsupported in this C subset: '%='"),
        ("a << b", "unsupported in this C subset: '<<'"),
        ("a >> b", "unsupported in this C subset: '>>'"),
    ];

    for (source, message) in cases {
        assert_eq!(errors(source), [message.to_string()], "for {source:?}");
    }
}

/// The pair is only recognized when the two halves touch, so spaced-out real operators still
/// parse as themselves.
#[test]
fn separated_operators_are_not_mistaken_for_a_pair() {
    assert_eq!(shape("a < -b"), "(binary < (ident a) (unary - (ident b)))");
    assert_eq!(shape("a > + b"), "(binary > (ident a) (unary + (ident b)))");
}

/// An expression that cannot be parsed says what it wanted and what it found.
#[test]
fn a_missing_operand_names_what_was_wanted() {
    assert_eq!(errors("1 +"), ["expected an expression, found end of file"]);
    assert_eq!(
        errors("a[i]["),
        ["expected an expression, found end of file"]
    );
    assert_eq!(errors("*x"), ["expected an expression, found '*'"]);
}

/// Every binary operator in the AST is reachable from source, so none is unreachable in
/// practice while still being representable.
#[test]
fn every_binary_operator_parses() {
    for op in BinOp::ALL {
        let source = format!("a {} b", op.spelling());

        assert_eq!(
            shape(&source),
            format!("(binary {} (ident a) (ident b))", op.spelling()),
            "for {source:?}"
        );
    }
}

/// Every prefix and postfix operator is reachable from source too.
#[test]
fn every_unary_operator_parses() {
    for op in UnOp::ALL {
        let source = format!("{}a", op.spelling());

        assert_eq!(
            shape(&source),
            format!("(unary {} (ident a))", op.spelling()),
            "for {source:?}"
        );
    }

    for op in IncDec::ALL {
        let source = format!("a{}", op.spelling());

        assert_eq!(
            shape(&source),
            format!("(postfix {} (ident a))", op.spelling()),
            "for {source:?}"
        );
    }
}
