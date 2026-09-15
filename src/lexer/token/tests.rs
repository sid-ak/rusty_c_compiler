//! Unit tests for the token vocabulary: spellings, the keyword table, and the escape table.

use super::*;

/// A sample of every `TokenKind` variant.
///
/// The exhaustive match below is the guard: a variant added without a sample here fails to
/// compile, and so cannot reach the parser without a spelling.
fn every_kind() -> Vec<TokenKind> {
    let samples = vec![
        TokenKind::Keyword(Keyword::Int),
        TokenKind::Ident("name".to_string()),
        TokenKind::IntLit(7),
        TokenKind::CharLit(b'x'),
        TokenKind::StrLit(b"hi".to_vec()),
        TokenKind::Plus,
        TokenKind::Minus,
        TokenKind::Star,
        TokenKind::Slash,
        TokenKind::Percent,
        TokenKind::Assign,
        TokenKind::EqEq,
        TokenKind::BangEq,
        TokenKind::Lt,
        TokenKind::Gt,
        TokenKind::LtEq,
        TokenKind::GtEq,
        TokenKind::AmpAmp,
        TokenKind::PipePipe,
        TokenKind::Bang,
        TokenKind::PlusPlus,
        TokenKind::MinusMinus,
        TokenKind::LParen,
        TokenKind::RParen,
        TokenKind::LBrace,
        TokenKind::RBrace,
        TokenKind::LBracket,
        TokenKind::RBracket,
        TokenKind::Semi,
        TokenKind::Comma,
        TokenKind::Eof,
    ];

    for kind in &samples {
        match kind {
            TokenKind::Keyword(_)
            | TokenKind::Ident(_)
            | TokenKind::IntLit(_)
            | TokenKind::CharLit(_)
            | TokenKind::StrLit(_)
            | TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::Assign
            | TokenKind::EqEq
            | TokenKind::BangEq
            | TokenKind::Lt
            | TokenKind::Gt
            | TokenKind::LtEq
            | TokenKind::GtEq
            | TokenKind::AmpAmp
            | TokenKind::PipePipe
            | TokenKind::Bang
            | TokenKind::PlusPlus
            | TokenKind::MinusMinus
            | TokenKind::LParen
            | TokenKind::RParen
            | TokenKind::LBrace
            | TokenKind::RBrace
            | TokenKind::LBracket
            | TokenKind::RBracket
            | TokenKind::Semi
            | TokenKind::Comma
            | TokenKind::Eof => {}
        }
    }

    samples
}

/// Every token kind renders as something a diagnostic can print.
#[test]
fn every_kind_has_a_non_empty_spelling() {
    for kind in every_kind() {
        assert!(!kind.to_string().is_empty(), "no spelling for {kind:?}");
    }
}

/// No two samples are the same variant, so the coverage list above is not padded.
#[test]
fn every_kind_sample_is_distinct() {
    let kinds = every_kind();
    let distinct: std::collections::HashSet<_> = kinds.iter().map(std::mem::discriminant).collect();

    assert_eq!(distinct.len(), kinds.len());
}

/// Operators and punctuators spell themselves; value-carrying kinds spell their value.
#[test]
fn fixed_spellings_are_the_source_text() {
    assert_eq!(TokenKind::LtEq.fixed_spelling(), Some("<="));
    assert_eq!(TokenKind::Semi.fixed_spelling(), Some(";"));
    assert_eq!(TokenKind::AmpAmp.fixed_spelling(), Some("&&"));
    assert_eq!(TokenKind::Ident("a".into()).fixed_spelling(), None);
    assert_eq!(TokenKind::IntLit(1).fixed_spelling(), None);
}

/// The parser's expected-versus-found wording reads as source text.
#[test]
fn spellings_read_as_source() {
    assert_eq!(
        format!(
            "expected '{}', found '{}'",
            TokenKind::Semi,
            TokenKind::RBrace
        ),
        "expected ';', found '}'"
    );
}

/// Every keyword round-trips: its spelling is recognized back as itself.
#[test]
fn keywords_round_trip_through_the_table() {
    for &keyword in Keyword::ALL {
        assert_eq!(
            Keyword::from_identifier(keyword.spelling()),
            Some(keyword),
            "for {keyword:?}"
        );
    }
}

/// The subset's ten keywords are the ones the grammar names, and nothing else.
#[test]
fn the_keyword_set_is_the_documented_one() {
    let spellings: Vec<_> = Keyword::ALL.iter().map(|k| k.spelling()).collect();

    assert_eq!(
        spellings,
        ["int", "char", "void", "if", "else", "while", "for", "return", "break", "continue"]
    );
}

/// A keyword with any prefix or suffix character is an identifier, not a keyword.
#[test]
fn keywords_with_affixes_are_identifiers() {
    for &keyword in Keyword::ALL {
        let spelling = keyword.spelling();

        for candidate in [
            format!("{spelling}eger"),
            format!("{spelling}_"),
            format!("{spelling}1"),
            format!("x{spelling}"),
            format!("_{spelling}"),
            spelling.to_uppercase(),
        ] {
            assert_eq!(
                Keyword::from_identifier(&candidate),
                None,
                "{candidate} should be an identifier"
            );
        }
    }
}

/// The two keyword lists partition C89's 32 reserved words: nothing is in both, and nothing
/// real C reserves is missing from either. A word in neither would be silently accepted as a
/// variable name, which is how `int static;` would sneak through.
#[test]
fn the_two_keyword_lists_partition_c89() {
    let mut all: Vec<&str> = Keyword::ALL.iter().map(|k| k.spelling()).collect();
    all.extend(UNSUPPORTED_KEYWORDS);
    all.sort_unstable();

    assert_eq!(
        all,
        [
            "auto", "break", "case", "char", "const", "continue", "default", "do", "double",
            "else", "enum", "extern", "float", "for", "goto", "if", "int", "long", "register",
            "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef",
            "union", "unsigned", "void", "volatile", "while",
        ]
    );
}

/// A word the subset leaves out is recognized as such; one it implements, and one nobody
/// reserves, are not.
#[test]
fn unsupported_keywords_are_recognized_by_name() {
    assert_eq!(unsupported_keyword("struct"), Some("struct"));
    assert_eq!(unsupported_keyword("sizeof"), Some("sizeof"));
    assert_eq!(unsupported_keyword("int"), None);
    assert_eq!(unsupported_keyword("total"), None);
    assert_eq!(unsupported_keyword("Struct"), None);
}

/// The escape table reads the same in both directions, which is what keeps the lexer's
/// decoding and the renderer's spelling from drifting apart.
#[test]
fn escape_table_round_trips() {
    for (letter, byte) in ESCAPES {
        assert_eq!(escape_byte(letter), Some(byte));
        assert_eq!(escape_letter(byte), Some(letter));
    }
}

/// A letter outside the table is not an escape.
#[test]
fn unknown_escape_letters_decode_to_nothing() {
    for letter in *b"qz8x" {
        assert_eq!(escape_byte(letter), None, "for {}", char::from(letter));
    }
}

/// Literals spell back out in a form a C programmer would recognize.
#[test]
fn literals_spell_themselves_back() {
    assert_eq!(TokenKind::CharLit(b'\n').to_string(), r"'\n'");
    assert_eq!(TokenKind::CharLit(b'a').to_string(), "'a'");
    assert_eq!(
        TokenKind::StrLit(b"a\tb\0c".to_vec()).to_string(),
        r#""a\tb\0c""#
    );
    assert_eq!(TokenKind::StrLit(Vec::new()).to_string(), r#""""#);
    assert_eq!(TokenKind::IntLit(-5).to_string(), "-5");
}

/// A byte with no escape and no printable form still renders, rather than being dropped.
#[test]
fn unprintable_bytes_render_as_hex() {
    assert_eq!(TokenKind::CharLit(0x01).to_string(), r"'\x01'");
    assert_eq!(TokenKind::CharLit(0xff).to_string(), r"'\xff'");
}

/// Spelling a literal body is quote-free, so both literal forms and the AST dump share it.
#[test]
fn spelling_a_literal_body_adds_no_quotes() {
    assert_eq!(spell_literal(b""), "");
    assert_eq!(spell_literal(b"hi"), "hi");
    assert_eq!(spell_literal(b"a\tb"), r"a\tb");
    assert_eq!(spell_literal(&[0x01]), r"\x01");
}

/// A token pairs a kind with the span it was scanned from, and displays as its kind.
#[test]
fn token_pairs_a_kind_with_a_span() {
    let token = Token::new(TokenKind::Semi, Span::new(4, 5));

    assert_eq!(token.kind, TokenKind::Semi);
    assert_eq!(token.span, Span::new(4, 5));
    assert_eq!(token.to_string(), ";");
}
