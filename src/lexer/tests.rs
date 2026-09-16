//! Unit tests for the scanner: every token category, maximal munch, diagnostics, and recovery.

use super::*;

/// What a preprocessor directive is reported as.
const DIRECTIVE_MESSAGE: &str = "unsupported in this C subset: preprocessor directives";

use std::path::Path;

use crate::diagnostics::DiagnosticKind;

/// Lex `source`, asserting it produced no diagnostics, and return its kinds without the `Eof`.
fn kinds(source: &str) -> Vec<TokenKind> {
    let lexed = lex(source.as_bytes());
    assert!(
        lexed.diagnostics.is_empty(),
        "expected {source:?} to lex cleanly, got: {:?}",
        lexed.diagnostics
    );
    assert_eq!(
        lexed.tokens.last().map(|token| &token.kind),
        Some(&TokenKind::Eof),
        "the stream must end in Eof"
    );

    lexed
        .tokens
        .into_iter()
        .map(|token| token.kind)
        .filter(|kind| *kind != TokenKind::Eof)
        .collect()
}

/// The single token `source` lexes to.
fn single(source: &str) -> TokenKind {
    let mut kinds = kinds(source);
    assert_eq!(kinds.len(), 1, "expected one token from {source:?}");

    kinds.remove(0)
}

/// Every keyword lexes as itself.
#[test]
fn keywords_lex_as_keywords() {
    for &keyword in Keyword::ALL {
        assert_eq!(single(keyword.spelling()), TokenKind::Keyword(keyword));
    }
}

/// A keyword with anything attached is one identifier, not a keyword plus leftovers.
#[test]
fn keyword_prefixes_lex_as_identifiers() {
    assert_eq!(single("integer"), TokenKind::Ident("integer".into()));
    assert_eq!(single("if_"), TokenKind::Ident("if_".into()));
    assert_eq!(single("_int"), TokenKind::Ident("_int".into()));
    assert_eq!(single("returns"), TokenKind::Ident("returns".into()));
}

/// Identifiers may start with a letter or underscore and continue with digits.
#[test]
fn identifiers_lex_as_identifiers() {
    for name in ["x", "_", "_x9", "camelCase", "SHOUT", "a1b2"] {
        assert_eq!(single(name), TokenKind::Ident(name.into()));
    }
}

/// Decimal, hex, and octal literals decode to the same value the base implies.
#[test]
fn integer_literals_decode_by_base() {
    let cases = [
        ("0", 0),
        ("7", 7),
        ("42", 42),
        ("0x0", 0),
        ("0xff", 255),
        ("0XFF", 255),
        ("0755", 493),
        ("010", 8),
        ("2147483647", i32::MAX),
    ];

    for (source, expected) in cases {
        assert_eq!(single(source), TokenKind::IntLit(expected), "for {source}");
    }
}

/// A character literal decodes to the one byte it denotes, escape or not.
#[test]
fn character_literals_decode_to_one_byte() {
    assert_eq!(single("'a'"), TokenKind::CharLit(b'a'));
    assert_eq!(single("' '"), TokenKind::CharLit(b' '));
    assert_eq!(single(r"'\n'"), TokenKind::CharLit(b'\n'));
    assert_eq!(single(r"'\0'"), TokenKind::CharLit(0));
    assert_eq!(single(r"'\\'"), TokenKind::CharLit(b'\\'));
    assert_eq!(single(r"'\''"), TokenKind::CharLit(b'\''));
    assert_eq!(single(r#"'"'"#), TokenKind::CharLit(b'"'));
}

/// Every escape in the table decodes inside a character literal.
#[test]
fn every_escape_decodes() {
    for (letter, byte) in token::ESCAPES {
        let source = format!("'\\{}'", char::from(letter));

        assert_eq!(single(&source), TokenKind::CharLit(byte), "for {source}");
    }
}

/// A string literal stores decoded bytes, so nothing downstream re-parses escapes.
#[test]
fn string_literals_store_decoded_bytes() {
    assert_eq!(single(r#""""#), TokenKind::StrLit(Vec::new()));
    assert_eq!(single(r#""hi""#), TokenKind::StrLit(b"hi".to_vec()));
    assert_eq!(
        single(r#""a\tb\0c""#),
        TokenKind::StrLit(vec![b'a', b'\t', b'b', 0, b'c'])
    );
    assert_eq!(
        single(r#""quote:\" done""#),
        TokenKind::StrLit(b"quote:\" done".to_vec())
    );
}

/// A string literal may contain an unescaped single quote, and vice versa.
#[test]
fn quotes_nest_inside_the_other_literal_form() {
    assert_eq!(single(r#""it's""#), TokenKind::StrLit(b"it's".to_vec()));
}

/// Every operator and punctuator lexes as its own token.
#[test]
fn operators_and_punctuators_lex_as_themselves() {
    let cases = [
        ("+", TokenKind::Plus),
        ("-", TokenKind::Minus),
        ("*", TokenKind::Star),
        ("/", TokenKind::Slash),
        ("%", TokenKind::Percent),
        ("=", TokenKind::Assign),
        ("==", TokenKind::EqEq),
        ("!=", TokenKind::BangEq),
        ("<", TokenKind::Lt),
        (">", TokenKind::Gt),
        ("<=", TokenKind::LtEq),
        (">=", TokenKind::GtEq),
        ("&&", TokenKind::AmpAmp),
        ("||", TokenKind::PipePipe),
        ("!", TokenKind::Bang),
        ("++", TokenKind::PlusPlus),
        ("--", TokenKind::MinusMinus),
        ("(", TokenKind::LParen),
        (")", TokenKind::RParen),
        ("{", TokenKind::LBrace),
        ("}", TokenKind::RBrace),
        ("[", TokenKind::LBracket),
        ("]", TokenKind::RBracket),
        (";", TokenKind::Semi),
        (",", TokenKind::Comma),
    ];

    for (source, expected) in cases {
        assert_eq!(single(source), expected, "for {source}");
    }
}

/// Multi-character operators win over their prefixes, and a longer run splits greedily.
#[test]
fn maximal_munch_splits_operators_the_documented_way() {
    let a = || TokenKind::Ident("a".into());
    let b = || TokenKind::Ident("b".into());

    assert_eq!(kinds("a<=b"), [a(), TokenKind::LtEq, b()]);
    assert_eq!(kinds("a<-b"), [a(), TokenKind::Lt, TokenKind::Minus, b()]);
    assert_eq!(
        kinds("a++ +b"),
        [a(), TokenKind::PlusPlus, TokenKind::Plus, b()]
    );
    assert_eq!(
        kinds("a+++b"),
        [a(), TokenKind::PlusPlus, TokenKind::Plus, b()]
    );
    assert_eq!(
        kinds("a---b"),
        [a(), TokenKind::MinusMinus, TokenKind::Minus, b()]
    );
    assert_eq!(kinds("a==b"), [a(), TokenKind::EqEq, b()]);
    assert_eq!(
        kinds("a= =b"),
        [a(), TokenKind::Assign, TokenKind::Assign, b()]
    );
}

/// An empty input is just the end of the input.
#[test]
fn empty_input_is_only_eof() {
    assert_eq!(kinds(""), []);
}

/// Whitespace separates tokens without becoming one.
#[test]
fn whitespace_is_not_a_token() {
    assert_eq!(kinds(" \t\r\n  ;  \n"), [TokenKind::Semi]);
}

/// Both comment forms are skipped, including between a token and its operator.
#[test]
fn comments_are_skipped_wherever_they_appear() {
    assert_eq!(kinds("// nothing here\n;"), [TokenKind::Semi]);
    assert_eq!(kinds("/* nothing */ ;"), [TokenKind::Semi]);
    assert_eq!(
        kinds("a /* between */ + /* and */ b"),
        [
            TokenKind::Ident("a".into()),
            TokenKind::Plus,
            TokenKind::Ident("b".into())
        ]
    );
    assert_eq!(
        kinds("a/**/+/**/b"),
        [
            TokenKind::Ident("a".into()),
            TokenKind::Plus,
            TokenKind::Ident("b".into())
        ]
    );
}

/// A comment running to the end of a file with no trailing newline still ends cleanly.
#[test]
fn line_comment_at_eof_without_a_newline() {
    assert_eq!(kinds(";// trailing"), [TokenKind::Semi]);
    assert_eq!(kinds("// only a comment"), []);
}

/// A block comment does not nest: the first `*/` closes it.
#[test]
fn block_comments_do_not_nest() {
    assert_eq!(
        kinds("/* outer /* inner */ ;"),
        [TokenKind::Semi],
        "the first */ should close the comment"
    );
}

/// A `/` that does not begin a comment is division.
#[test]
fn a_lone_slash_is_division() {
    assert_eq!(
        kinds("a / b"),
        [
            TokenKind::Ident("a".into()),
            TokenKind::Slash,
            TokenKind::Ident("b".into())
        ]
    );
}

/// The dump of a multi-line fixture, which pins the line and column of every token.
#[test]
fn every_token_reports_its_line_and_column() {
    let source = b"int x;\nif (x)\n\treturn 0;\n";
    let lexed = lex(source);
    let map = SourceMap::new(Path::new("t.c"), source);

    assert_eq!(
        dump(&map, &lexed.tokens),
        concat!(
            "1:1-1:4    Keyword(Int)\n",
            "1:5-1:6    Ident(\"x\")\n",
            "1:6-1:7    Semi\n",
            "2:1-2:3    Keyword(If)\n",
            "2:4-2:5    LParen\n",
            "2:5-2:6    Ident(\"x\")\n",
            "2:6-2:7    RParen\n",
            "3:2-3:8    Keyword(Return)\n",
            "3:9-3:10   IntLit(0)\n",
            "3:10-3:11  Semi\n",
            "4:1-4:1    Eof\n",
        )
    );
}

/// The position column widens with the file, so a dump of a long program still lines up.
///
/// A fixed-width column stops aligning once line numbers outgrow it, and that is precisely the
/// size of file where a dump is long enough for alignment to be what makes it readable.
#[test]
fn dump_column_widens_for_large_line_numbers() {
    let mut source = vec![b'\n'; 10_000];
    source.extend_from_slice(b"int x;\n");
    let lexed = lex(&source);
    let map = SourceMap::new(Path::new("t.c"), &source);

    let dumped = dump(&map, &lexed.tokens);

    // The kind begins after the padding, so every line should start it at the same column.
    let kind_starts: std::collections::HashSet<usize> = dumped
        .lines()
        .map(|line| line.rfind("  ").map(|index| index + 2).unwrap_or(0))
        .collect();
    assert_eq!(kind_starts.len(), 1, "columns not aligned:\n{dumped}");

    let column = kind_starts.into_iter().next().unwrap_or(0);
    assert!(
        column > 16,
        "expected five-digit lines to need more than the old fixed width, got {column}"
    );
}

/// A tab advances the column by one, so the token after it starts one column further on.
#[test]
fn a_tab_advances_the_column_by_one() {
    let source = b"\t\t;";
    let lexed = lex(source);
    let map = SourceMap::new(Path::new("t.c"), source);
    let semi = lexed.tokens.first().expect("expected a token");

    assert_eq!(map.location(semi.span.start).column, 3);
}

/// CRLF input produces the same tokens, and the same line numbers, as LF input.
#[test]
fn crlf_lexes_the_same_as_lf() {
    let crlf = lex(b"int x;\r\nint y;\r\n");
    let lf = lex(b"int x;\nint y;\n");

    let crlf_kinds: Vec<_> = crlf.tokens.iter().map(|token| &token.kind).collect();
    let lf_kinds: Vec<_> = lf.tokens.iter().map(|token| &token.kind).collect();
    assert_eq!(crlf_kinds, lf_kinds);

    let map = SourceMap::new(Path::new("t.c"), b"int x;\r\nint y;\r\n");
    let second_line_start = crlf.tokens.get(3).expect("expected a fourth token");
    assert_eq!(map.location(second_line_start.span.start).line, 2);
}

/// Invalid UTF-8 is a diagnostic, not a panic, and the scan still reaches the end.
#[test]
fn invalid_utf8_is_a_diagnostic_not_a_panic() {
    let lexed = lex(b"int \xff x;");

    assert_eq!(lexed.diagnostics.len(), 1);
    assert!(
        lexed
            .diagnostics
            .first()
            .is_some_and(|diagnostic| diagnostic.message.contains("byte 0xff")),
        "got: {:?}",
        lexed.diagnostics
    );
    assert_eq!(
        lexed.tokens.last().map(|token| &token.kind),
        Some(&TokenKind::Eof)
    );
}

/// A file of arbitrary bytes still terminates and still ends in `Eof`.
#[test]
fn arbitrary_bytes_terminate() {
    let source: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    let lexed = lex(&source);

    assert_eq!(
        lexed.tokens.last().map(|token| &token.kind),
        Some(&TokenKind::Eof)
    );
}

/// Assert `source` produces exactly one lexing diagnostic reading `message`, whose span covers
/// `offending` — the whole malformed construct, not just the byte that gave it away — and that
/// the scan still reached `Eof`.
fn assert_one_error(source: &str, message: &str, offending: &str) {
    let lexed = lex(source.as_bytes());

    assert_eq!(
        lexed.diagnostics.len(),
        1,
        "for {source:?}, got: {:?}",
        lexed.diagnostics
    );
    let diagnostic = lexed
        .diagnostics
        .first()
        .expect("the length was just asserted");

    assert_eq!(diagnostic.kind, DiagnosticKind::Lex, "for {source:?}");
    assert_eq!(diagnostic.message, message, "for {source:?}");
    assert_eq!(
        source
            .as_bytes()
            .get(diagnostic.span.start..diagnostic.span.end),
        Some(offending.as_bytes()),
        "for {source:?}, the span should cover the whole offending construct"
    );
    assert_eq!(
        lexed.tokens.last().map(|token| &token.kind),
        Some(&TokenKind::Eof),
        "for {source:?}, scanning must still reach the end"
    );
}

/// Every malformed-input path reports its own diagnostic, spanning the whole construct.
#[test]
fn each_malformed_construct_has_its_own_diagnostic() {
    let cases = [
        ("\"abc", "unterminated string literal", "\"abc"),
        ("\"abc\ndone", "unterminated string literal", "\"abc"),
        ("'a", "unterminated character literal", "'a"),
        ("''", "empty character literal", "''"),
        (
            "'ab'",
            "character literal must contain exactly one character",
            "'ab'",
        ),
        (r"'\q'", r"unknown escape sequence '\q'", r"\q"),
        (r#""a\qb""#, r"unknown escape sequence '\q'", r"\q"),
        ("/* open", "unterminated block comment", "/* open"),
        ("0x", "expected digits after '0x'", "0x"),
        ("0xzz", "invalid digit 'z' in hexadecimal literal", "0xzz"),
        ("08", "invalid digit '8' in octal literal", "08"),
        ("12ab", "invalid digit 'a' in decimal literal", "12ab"),
        (
            "2147483648",
            "integer literal is too large for 'int': 2147483648",
            "2147483648",
        ),
        (
            "99999999999999999999999",
            "integer literal is too large for 'int': 99999999999999999999999",
            "99999999999999999999999",
        ),
        ("@", "stray '@' in program", "@"),
        ("$", "stray '$' in program", "$"),
        ("`", "stray '`' in program", "`"),
    ];

    for (source, message, offending) in cases {
        assert_one_error(source, message, offending);
    }
}

/// Punctuation of real C that this grammar has no token for is named as unsupported rather
/// than as a stray byte, in the same words the parser uses for the constructs it catches.
///
/// The parser cannot report these: there is no token for `?` or `#`, so nothing about them
/// ever reaches it.
#[test]
fn unsupported_punctuation_is_named_not_called_stray() {
    for character in ["&", "|", "?", ":", "^", "~"] {
        assert_one_error(
            character,
            &format!("unsupported in this C subset: '{character}'"),
            character,
        );
    }
}

/// Each unsupported character says what to reach for instead.
#[test]
fn unsupported_punctuation_says_what_to_do_instead() {
    let cases = [
        ("a & b", "did you mean '&&'?"),
        ("a | b", "did you mean '||'?"),
        (
            "#include <stdio.h>",
            "this subset has no preprocessor, so no directive has any meaning here",
        ),
        (
            "a ? b : c",
            "the conditional operator is not in this subset; use an 'if' statement",
        ),
        ("a ^ b", "the bitwise operators are not in this subset"),
    ];

    for (source, note) in cases {
        let lexed = lex(source.as_bytes());
        let diagnostic = lexed
            .diagnostics
            .first()
            .unwrap_or_else(|| panic!("expected a diagnostic for {source:?}"));

        assert_eq!(diagnostic.note_messages(), [note], "for {source:?}");
    }
}

/// An over-large literal says what the limit is and how to write `INT_MIN` within it.
#[test]
fn integer_overflow_explains_the_limit() {
    let lexed = lex(b"2147483648");
    let diagnostic = lexed.diagnostics.first().expect("expected a diagnostic");

    assert_eq!(
        diagnostic.note_messages(),
        ["the maximum is 2147483647; write INT_MIN as -2147483647 - 1"]
    );
}

/// One bad construct produces one diagnostic, not a cascade.
#[test]
fn a_single_error_does_not_cascade() {
    for source in [r"'\q'", "0x", "'ab'", "\"unterminated"] {
        assert_eq!(
            lex(source.as_bytes()).diagnostics.len(),
            1,
            "for {source:?}"
        );
    }
}

/// Lex `source`, which may contain errors, and return its token kinds and diagnostic messages.
fn kinds_and_messages(source: &[u8]) -> (Vec<TokenKind>, Vec<String>) {
    let lexed = lex(source);

    (
        lexed.tokens.into_iter().map(|token| token.kind).collect(),
        lexed
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect(),
    )
}

/// The kinds of `int x;` followed by the end of the stream, which the recovery tests expect to
/// find intact after the construct they recover from.
fn int_x_then_eof() -> Vec<TokenKind> {
    vec![
        TokenKind::Keyword(Keyword::Int),
        TokenKind::Ident("x".into()),
        TokenKind::Semi,
        TokenKind::Eof,
    ]
}

/// An unterminated literal resynchronizes at the end of its line, so the next line still lexes.
#[test]
fn scanning_resumes_on_the_line_after_an_unterminated_literal() {
    let (kinds, messages) = kinds_and_messages(b"\"oops\nint x;\n");

    assert_eq!(messages.len(), 1);
    let mut expected = vec![TokenKind::StrLit(b"oops".to_vec())];
    expected.extend(int_x_then_eof());
    assert_eq!(kinds, expected);
}

/// An unterminated block comment resynchronizes at end of file, swallowing the rest.
#[test]
fn an_unterminated_block_comment_runs_to_the_end_of_file() {
    let (kinds, messages) = kinds_and_messages(b"int x;\n/* oops\nint y;\n");

    assert_eq!(messages.len(), 1);
    assert_eq!(kinds, int_x_then_eof());
}

/// A preprocessor directive is one unsupported construct: one diagnostic whose span covers the
/// directive, not a report for the `#` followed by more for the rest of the line.
#[test]
fn a_directive_is_one_diagnostic_spanning_the_directive() {
    let cases = [
        ("#include <stdio.h>", "#include <stdio.h>"),
        ("#define LIMIT 10   ", "#define LIMIT 10"),
        ("#pragma once\n", "#pragma once"),
        ("#", "#"),
    ];

    for (source, directive) in cases {
        assert_one_error(source, DIRECTIVE_MESSAGE, directive);
    }
}

/// Scanning resumes on the line after a directive, so the code that follows still lexes.
#[test]
fn scanning_resumes_on_the_line_after_a_directive() {
    for source in [
        "#include <stdio.h>\nint x;\n",
        "#\nint x;\n",
        "#if 0 /* a */\r\nint x;\r\n",
    ] {
        let (kinds, messages) = kinds_and_messages(source.as_bytes());

        assert_eq!(messages, [DIRECTIVE_MESSAGE], "for {source:?}");
        assert_eq!(kinds, int_x_then_eof(), "for {source:?}");
    }
}

/// A backslash before the newline splices the next line onto the directive (C11 5.1.1.2, phase
/// 2), so a multi-line macro is still one directive and its body is not lexed as code.
#[test]
fn a_directive_continues_across_spliced_lines() {
    let spliced = "#define SUM(a, b) \\\n    ((a) + (b))";
    assert_one_error(spliced, DIRECTIVE_MESSAGE, spliced);

    for source in [
        "#define SUM(a, b) \\\n    ((a) + (b))\nint x;\n",
        "#define ONE \\\r\n    1\r\nint x;\r\n",
        "#define TWO \\\n \\\n 2\nint x;\n",
    ] {
        let (kinds, messages) = kinds_and_messages(source.as_bytes());

        assert_eq!(messages, [DIRECTIVE_MESSAGE], "for {source:?}");
        assert_eq!(kinds, int_x_then_eof(), "for {source:?}");
    }
}

/// A directive may follow whitespace or a comment on its own line: what matters is that no token
/// precedes the `#` on that line (C11 6.10p2).
#[test]
fn a_directive_may_be_indented_or_follow_a_comment() {
    for source in [
        "  #define X 1",
        "\t# define X 1",
        "/* banner */ #define X 1",
        "int x;\n   #define X 1\n",
    ] {
        let (_, messages) = kinds_and_messages(source.as_bytes());

        assert_eq!(messages, [DIRECTIVE_MESSAGE], "for {source:?}");
    }
}

/// A `#` after a token on the same line is not a directive, so it is the single unsupported
/// character and the rest of the line lexes as usual.
#[test]
fn a_hash_after_a_token_on_its_line_is_a_single_character() {
    assert_one_error("a # b", "unsupported in this C subset: '#'", "#");

    let (kinds, messages) = kinds_and_messages(b"int # x;");
    assert_eq!(messages, ["unsupported in this C subset: '#'"]);
    assert_eq!(kinds, int_x_then_eof());
}

/// A newline inside a block comment does not start a new line for a directive, because the
/// comment is replaced by one space before directives are recognized (C11 5.1.1.2, phase 3).
#[test]
fn a_newline_inside_a_comment_does_not_start_a_directive() {
    let (_, messages) = kinds_and_messages(b"int a; /* one\ntwo */ # b");

    assert_eq!(messages, ["unsupported in this C subset: '#'"]);
}

/// A directive ending at the end of file, including one whose last line is spliced into nothing,
/// still ends the stream in `Eof`.
#[test]
fn a_directive_at_the_end_of_file_terminates() {
    for source in ["#include <stdio.h>", "#define X \\", "#define X \\\n"] {
        let (kinds, messages) = kinds_and_messages(source.as_bytes());

        assert_eq!(messages, [DIRECTIVE_MESSAGE], "for {source:?}");
        assert_eq!(kinds, [TokenKind::Eof], "for {source:?}");
    }
}

/// A file with several mistakes reports all of them, in source order.
#[test]
fn several_errors_are_reported_in_source_order() {
    let source = b"int a = 0x;\nchar c = '';\nint b = @;\nchar *s = \"open\n";
    let lexed = lex(source);

    let messages: Vec<_> = lexed
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();
    assert_eq!(
        messages,
        [
            "expected digits after '0x'",
            "empty character literal",
            "stray '@' in program",
            "unterminated string literal",
        ]
    );

    let offsets: Vec<_> = lexed
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.span.start)
        .collect();
    let mut sorted = offsets.clone();
    sorted.sort_unstable();
    assert_eq!(offsets, sorted, "diagnostics must come out in source order");
}

/// A file of nothing but stray characters terminates, reporting one error per character.
#[test]
fn a_file_of_stray_characters_terminates() {
    let lexed = lex(b"@$#`@$#`");

    assert_eq!(lexed.diagnostics.len(), 8);
    assert_eq!(
        lexed
            .tokens
            .iter()
            .map(|token| &token.kind)
            .collect::<Vec<_>>(),
        [&TokenKind::Eof],
        "stray characters produce no tokens"
    );
}

/// An error path cannot loop: repeated unterminated constructs still reach the end.
#[test]
fn repeated_errors_still_terminate() {
    let lexed = lex(b"'\n'\n'\n'\n\"\n\"\n\"\n");

    assert!(!lexed.diagnostics.is_empty());
    assert_eq!(
        lexed.tokens.last().map(|token| &token.kind),
        Some(&TokenKind::Eof)
    );
}

/// A backslash at the very end of a file is an unterminated literal, not an overrun.
#[test]
fn a_trailing_backslash_does_not_overrun() {
    for source in [r"'\", r#""\"#, r"'", r#"""#] {
        let lexed = lex(source.as_bytes());

        assert_eq!(lexed.diagnostics.len(), 1, "for {source:?}");
        assert_eq!(
            lexed.tokens.last().map(|token| &token.kind),
            Some(&TokenKind::Eof),
            "for {source:?}"
        );
    }
}

/// Spans are byte ranges over the original source, so slicing one back out gives the token.
#[test]
fn spans_slice_back_to_the_source_text() {
    let source = b"int total = 0xff;";
    let lexed = lex(source);

    for token in &lexed.tokens {
        if token.kind == TokenKind::Eof {
            continue;
        }
        assert!(
            source.get(token.span.start..token.span.end).is_some(),
            "span {:?} is not a valid range",
            token.span
        );
    }

    let literal = lexed.tokens.get(3).expect("expected the literal token");
    assert_eq!(literal.kind, TokenKind::IntLit(255));
    assert_eq!(
        source.get(literal.span.start..literal.span.end),
        Some(&b"0xff"[..])
    );
}
