//! Snapshot of the full token stream for a representative program.
//!
//! A per-token unit test proves each category in isolation; this proves they compose, and turns a
//! regression anywhere in the scanner into a readable diff rather than one failed assertion.

use std::path::Path;

use mycc::diagnostics::SourceMap;
use mycc::lexer;

/// A program touching every lexable construct in the subset: both comment forms, all three integer
/// bases, character and string literals with escapes, every operator, and every punctuator.
const REPRESENTATIVE: &[u8] = br#"// Sum the first n squares, and say so.
void print_int(int n);
void print_char(char c);
void print_string(char s[]);

int total;

int add(int a, int b) {
    return a + b;
}

int square(int x) {
    return x * x;
}

int main(void) {
    int n = 10;          /* decimal */
    int mask = 0xff;     /* hex */
    int mode = 0755;     /* octal */
    char nl = '\n';

    for (int i = 0; i < n; i = i + 1) {
        if (i % 2 == 0 && i != 0) {
            total = add(total, square(i));
        } else if (i >= 3 || i <= 1) {
            continue;
        }
        while (!total) {
            break;
        }
    }

    int values[3];
    values[0] = total--;
    values[1] = ++total;
    values[2] = -total / 2;

    if (mask > mode) {
        print_string("sum:\t");
        print_int(values[0]);
        print_char(nl);
    }
    return 0;
}
"#;

/// The whole token stream, with each token's start and end position, for the program above.
#[test]
fn representative_program_token_stream() {
    let path = Path::new("representative.c");
    let lexed = lexer::lex(REPRESENTATIVE);

    assert!(
        lexed.diagnostics.is_empty(),
        "the fixture should lex cleanly, got: {:?}",
        lexed.diagnostics
    );

    let map = SourceMap::new(path, REPRESENTATIVE);
    insta::assert_snapshot!(lexer::dump(&map, &lexed.tokens));
}
