//! Execution tests: compile a C program with `rustycc`, run it, and check what it printed.
//!
//! Everything here is a question put to the machine. A snapshot says what was emitted and the
//! assembler says it is legal; only running the program says the answer is right, and for code
//! generation that is the only question that matters.
//!
//! Where an operator is not commutative, the operands are asymmetric on purpose. `2 - 2` is `0`
//! whichever way round the lowering reads its operands, so a transposed `sub` passes it; `10 - 3`
//! is `7` one way and `-7` the other.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too, and a scratch directory that cannot be created is a
// broken checkout rather than something to report a diagnostic about.
#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use rustycc::codegen;
use rustycc::lexer;
use rustycc::parser;
use rustycc::runtime::SHIM_OBJECT;
use rustycc::sema;

/// A scratch directory under Cargo's output directory, unique to `name`.
fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("OUT_DIR"))
        .join("codegen-programs")
        .join(name);
    fs::create_dir_all(&directory).expect("could not create the scratch directory");

    directory
}

/// Compiles `source` through every pass, asserting each one accepts it, and returns the assembly.
fn assemble(source: &str) -> String {
    let lexed = lexer::lex(source.as_bytes());
    assert!(
        lexed.diagnostics.is_empty(),
        "does not lex: {:?}",
        lexed.diagnostics
    );

    let parsed = parser::parse(&lexed.tokens);
    assert!(
        parsed.diagnostics.is_empty(),
        "does not parse: {:?}",
        parsed.diagnostics
    );

    let analysis = sema::analyze(&parsed.program);
    assert!(
        analysis.is_accepted(),
        "does not analyze: {:?}",
        analysis
            .diagnostics
            .iter()
            .map(|diagnostic| &diagnostic.message)
            .collect::<Vec<_>>()
    );

    let generated = codegen::generate(&parsed.program, &analysis.annotations);
    assert!(
        generated.diagnostics.is_empty(),
        "does not lower: {:?}",
        generated
            .diagnostics
            .iter()
            .map(|diagnostic| &diagnostic.message)
            .collect::<Vec<_>>()
    );

    generated.assembly
}

/// Compiles `source`, links it with a C `main` that prints `answer()`, runs it, and returns stdout.
///
/// The C driver exists so a test can read a full 32-bit result. An exit code is masked to eight
/// bits, which would quietly turn `256` into `0` and `-7` into `249`.
fn answer_of(name: &str, source: &str) -> String {
    let assembly = assemble(source);
    let directory = scratch(name);
    let assembly_path = directory.join("out.s");
    let driver = directory.join("driver.c");
    let binary = directory.join("program");

    fs::write(&assembly_path, &assembly).expect("could not write the assembly");
    fs::write(
        &driver,
        "#include <stdio.h>\nint answer(void);\nint main(void) { printf(\"%d\\n\", answer()); return 0; }\n",
    )
    .expect("could not write the driver");

    let built = Command::new("clang")
        .args(["-std=c99", "-O0"])
        .arg(&assembly_path)
        .arg(&driver)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("could not run clang; run xcode-select --install");
    assert!(
        built.status.success(),
        "linking failed:\n{}\n--- assembly ---\n{assembly}",
        String::from_utf8_lossy(&built.stderr)
    );

    let run = Command::new(&binary)
        .output()
        .expect("could not run the program");
    assert!(
        run.status.success(),
        "the program exited with {:?}",
        run.status.code()
    );

    String::from_utf8_lossy(&run.stdout).trim_end().to_owned()
}

/// Wraps `body` as the whole of `int answer(void)`.
fn answer(name: &str, body: &str) -> String {
    answer_of(name, &format!("int answer(void) {{\n{body}\n}}\n"))
}

/// Arithmetic groups the way C says it does, and each operator computes what it should.
#[test]
fn arithmetic_and_precedence() {
    let cases = [
        ("literal", "return 42;", "42"),
        ("addition", "return 2 + 3;", "5"),
        ("precedence", "return 1 + 2 * 3;", "7"),
        ("precedence the other way", "return 2 * 3 + 1;", "7"),
        ("parentheses override", "return (1 + 2) * 3;", "9"),
        ("left associativity", "return 10 - 3 - 2;", "5"),
        ("nested to four levels", "return 1 + 2 * (3 + 4 * 2);", "23"),
        ("unary minus", "return -7;", "-7"),
        ("unary minus on an expression", "return -(3 + 4);", "-7"),
        ("unary plus", "return +7;", "7"),
        ("double negation", "return - -7;", "7"),
        ("logical not of zero", "return !0;", "1"),
        ("logical not of a value", "return !5;", "0"),
        ("logical not twice", "return !!5;", "1"),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// Non-commutative operators read their operands in source order.
///
/// Every case here is asymmetric. A lowering that swapped its operands would still pass `2 - 2` or
/// `a < a`, which is why none of those appear.
#[test]
fn non_commutative_operators_read_left_then_right() {
    let cases = [
        ("subtraction", "return 10 - 3;", "7"),
        ("division", "return 10 / 3;", "3"),
        ("remainder", "return 10 % 3;", "1"),
        ("less than, true", "return 1 < 2;", "1"),
        ("less than, false", "return 2 < 1;", "0"),
        ("greater than, true", "return 2 > 1;", "1"),
        ("greater than, false", "return 1 > 2;", "0"),
        ("less or equal, below", "return 1 <= 2;", "1"),
        ("less or equal, above", "return 2 <= 1;", "0"),
        ("greater or equal, above", "return 2 >= 1;", "1"),
        ("greater or equal, below", "return 1 >= 2;", "0"),
        ("equality, unequal", "return 1 == 2;", "0"),
        ("inequality, unequal", "return 1 != 2;", "1"),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// Division and remainder truncate toward zero, which is what C requires of negative operands.
#[test]
fn division_and_remainder_truncate_toward_zero() {
    let cases = [
        ("negative dividend", "return -10 / 3;", "-3"),
        ("negative divisor", "return 10 / -3;", "-3"),
        ("both negative", "return -10 / -3;", "3"),
        ("negative remainder", "return -10 % 3;", "-1"),
        ("remainder of a negative divisor", "return 10 % -3;", "1"),
        (
            "remainder sign follows the dividend",
            "return -7 % 2;",
            "-1",
        ),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// Constants at the edges of `int` survive the immediate-building path.
#[test]
fn constants_at_the_edges_of_int() {
    let cases = [
        ("zero", "return 0;", "0"),
        ("small", "return 255;", "255"),
        ("just past a byte", "return 256;", "256"),
        ("just past sixteen bits", "return 65536;", "65536"),
        ("large", "return 123456789;", "123456789"),
        ("int max", "return 2147483647;", "2147483647"),
        ("int min", "return -2147483647 - 1;", "-2147483648"),
        ("negative large", "return -123456789;", "-123456789"),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// Locals are stored and read back, and assignment yields the value assigned.
#[test]
fn locals_store_and_load() {
    let cases = [
        ("declare and read", "int x; x = 5; return x;", "5"),
        ("declare with an initializer", "int x = 5; return x;", "5"),
        ("reassign", "int x = 1; x = 9; return x;", "9"),
        (
            "assignment is an expression",
            "int x; int y; y = (x = 3); return x + y;",
            "6",
        ),
        (
            "chained assignment",
            "int x; int y; x = y = 4; return x + y;",
            "8",
        ),
        (
            "arithmetic on locals",
            "int a = 10; int b = 3; return a - b;",
            "7",
        ),
        (
            "shadowing in a block",
            "int x = 1; { int x = 2; x = x + 1; } return x;",
            "1",
        ),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// `&&` and `||` skip the right operand when the left one already decides the answer.
///
/// Proved by side effect rather than by result: the right operand increments a variable that is
/// then returned, so a lowering that evaluated both sides gives a different number.
#[test]
fn short_circuit_skips_the_right_operand() {
    let cases = [
        (
            "and stops at a false left",
            "int n = 0; int r = (0 && (n = 1)); return n;",
            "0",
        ),
        (
            "and evaluates a true left",
            "int n = 0; int r = (1 && (n = 1)); return n;",
            "1",
        ),
        (
            "or stops at a true left",
            "int n = 0; int r = (1 || (n = 1)); return n;",
            "0",
        ),
        (
            "or evaluates a false left",
            "int n = 0; int r = (0 || (n = 1)); return n;",
            "1",
        ),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// `&&` and `||` produce exactly `0` or `1`, not whichever operand decided the answer.
#[test]
fn logical_operators_normalize_to_zero_or_one() {
    let cases = [
        ("and of two truthy values", "return 3 && 5;", "1"),
        ("and with a false right", "return 3 && 0;", "0"),
        ("or of a truthy left", "return 3 || 5;", "1"),
        ("or of two false values", "return 0 || 0;", "0"),
        ("or reaching a truthy right", "return 0 || 7;", "1"),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// Prefix and postfix increment differ in the value they produce, not only in what they store.
#[test]
fn prefix_and_postfix_differ_in_the_value_they_produce() {
    let cases = [
        (
            "prefix increment yields the new value",
            "int i = 5; return ++i;",
            "6",
        ),
        (
            "postfix increment yields the old value",
            "int i = 5; return i++;",
            "5",
        ),
        (
            "prefix decrement yields the new value",
            "int i = 5; return --i;",
            "4",
        ),
        (
            "postfix decrement yields the old value",
            "int i = 5; return i--;",
            "5",
        ),
        ("postfix still stores", "int i = 5; i++; return i;", "6"),
        ("prefix still stores", "int i = 5; ++i; return i;", "6"),
        (
            "difference in one expression",
            "int i = 5; return (i++) + (i++);",
            "11",
        ),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// Arrays index by element, read and write, and scale the index by the element's size.
#[test]
fn arrays_index_read_and_write() {
    let cases = [
        (
            "read an initialized element",
            "int a[3] = {7, 8, 9}; return a[1];",
            "8",
        ),
        (
            "read the last element",
            "int a[3] = {7, 8, 9}; return a[2];",
            "9",
        ),
        ("write then read", "int a[3]; a[0] = 4; return a[0];", "4"),
        (
            "write past the first element",
            "int a[3]; a[2] = 6; a[0] = 1; return a[2];",
            "6",
        ),
        (
            "index by a variable",
            "int a[3] = {7, 8, 9}; int i = 2; return a[i];",
            "9",
        ),
        (
            "index by an expression",
            "int a[3] = {7, 8, 9}; return a[1 + 1];",
            "9",
        ),
        (
            "elements are independent",
            "int a[2]; a[0] = 1; a[1] = 2; return a[0] * 10 + a[1];",
            "12",
        ),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// A brace list shorter than the array zeroes every element it does not reach.
///
/// C says the elements a short initializer does not mention are zero, and for a global that falls
/// out of the data section being zero to begin with. A local's storage is whatever the stack
/// happened to be holding, so the zeros have to be written — and a compiler that only stores the
/// elements it was given passes every test that reads back one of them.
#[test]
fn a_short_initializer_list_zeroes_the_rest() {
    let cases = [
        (
            "one element of four",
            "int a[4] = {5}; return a[0] + a[1] + a[2] + a[3];",
            "5",
        ),
        (
            "the elements past the list are individually zero",
            "int a[4] = {1, 2}; return a[2] * 10 + a[3];",
            "0",
        ),
        (
            "a full list leaves nothing to zero",
            "int a[3] = {1, 2, 3}; return a[0] + a[1] * 10 + a[2] * 100;",
            "321",
        ),
        (
            "an empty list zeroes the whole array",
            "int a[3] = {}; return a[0] + a[1] + a[2];",
            "0",
        ),
        (
            "a char array zeroes at its own stride",
            "char c[4] = {'a'}; return c[0] + c[1] + c[2] + c[3];",
            "97",
        ),
        (
            "the tail is zero even after the stack has been used",
            "int used[4]; for (int i = 0; i < 4; i = i + 1) { used[i] = 999; }              int a[4] = {5}; return a[1] + a[2] + a[3] + used[0] - 999;",
            "0",
        ),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// A `char` occupies one byte and sign-extends when it is read back.
#[test]
fn chars_store_in_one_byte_and_sign_extend() {
    let cases = [
        ("a character literal", "char c = 'A'; return c;", "65"),
        ("arithmetic promotes", "char c = 'a'; return c - 'A';", "32"),
        (
            "a value above 127 is negative",
            "char c = 200; return c;",
            "-56",
        ),
        (
            "a char array element",
            "char a[3]; a[1] = 'z'; return a[1];",
            "122",
        ),
        (
            "char elements are one byte apart",
            "char a[3]; a[0] = 1; a[1] = 2; a[2] = 3; return a[0] + a[1] * 10 + a[2] * 100;",
            "321",
        ),
        (
            "comparison against a literal",
            "char c = 'm'; return c == 'm';",
            "1",
        ),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// A deeply nested expression gives every level its own temporary.
///
/// The value is chosen so that any two levels sharing a slot produces a different number rather
/// than a crash.
#[test]
fn nested_expressions_do_not_share_temporaries() {
    assert_eq!(
        answer("deep", "return 1 + (2 + (3 + (4 + (5 + (6 + (7 + 8))))));"),
        "36"
    );
    assert_eq!(
        answer("deep-subtraction", "return 100 - (50 - (25 - (10 - 5)));"),
        "70"
    );
}

/// A name for a scratch directory, made from a test case's description.
fn slug(shape: &str) -> String {
    shape.replace(' ', "-")
}

/// `if` and `else` pick exactly one arm, and chains nest correctly.
#[test]
fn branches_take_one_arm() {
    let cases = [
        ("if taken", "if (1) { return 1; } return 2;", "1"),
        ("if not taken", "if (0) { return 1; } return 2;", "2"),
        ("else taken", "if (0) { return 1; } else { return 2; } return 3;", "2"),
        ("if taken with an else present", "if (1) { return 1; } else { return 2; } return 3;", "1"),
        ("no fallthrough into else", "int n = 0; if (1) { n = 1; } else { n = 2; } return n;", "1"),
        (
            "else if chain, first",
            "int n = 1; if (n == 1) { return 10; } else if (n == 2) { return 20; } else { return 30; }",
            "10",
        ),
        (
            "else if chain, middle",
            "int n = 2; if (n == 1) { return 10; } else if (n == 2) { return 20; } else { return 30; }",
            "20",
        ),
        (
            "else if chain, last",
            "int n = 3; if (n == 1) { return 10; } else if (n == 2) { return 20; } else { return 30; }",
            "30",
        ),
        (
            "dangling else binds to the nearest if",
            "int n = 0; if (1) if (0) n = 1; else n = 2; return n;",
            "2",
        ),
        ("body without braces", "int n = 0; if (1) n = 5; return n;", "5"),
        ("nested ifs", "int n = 0; if (1) { if (1) { n = 7; } } return n;", "7"),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// `while` tests before each iteration and runs until its condition fails.
#[test]
fn while_loops_run_and_terminate() {
    let cases = [
        ("never entered", "int n = 0; while (0) { n = 1; } return n;", "0"),
        ("counts up", "int n = 0; while (n < 5) { n = n + 1; } return n;", "5"),
        ("sums", "int i = 0; int t = 0; while (i < 5) { t = t + i; i = i + 1; } return t;", "10"),
        ("break leaves early", "int n = 0; while (1) { n = n + 1; if (n == 3) { break; } } return n;", "3"),
        (
            "continue skips the rest of the body",
            "int i = 0; int t = 0; while (i < 5) { i = i + 1; if (i == 3) { continue; } t = t + i; } return t;",
            "12",
        ),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// Every combination of present and absent `for` clauses behaves as C says.
#[test]
fn for_loops_handle_every_clause_combination() {
    let cases = [
        ("all three clauses", "int t = 0; for (int i = 0; i < 4; i = i + 1) { t = t + i; } return t;", "6"),
        ("declaration in the init", "int t = 0; for (int i = 1; i <= 3; i = i + 1) { t = t * 10 + i; } return t;", "123"),
        ("expression in the init", "int i; int t = 0; for (i = 0; i < 3; i = i + 1) { t = t + 1; } return t;", "3"),
        ("no init", "int i = 0; int t = 0; for (; i < 3; i = i + 1) { t = t + 1; } return t;", "3"),
        ("no step", "int t = 0; for (int i = 0; i < 3;) { t = t + 1; i = i + 1; } return t;", "3"),
        ("no condition", "int t = 0; for (int i = 0; ; i = i + 1) { t = t + 1; if (i == 2) { break; } } return t;", "3"),
        ("no init or step", "int i = 0; int t = 0; for (; i < 3;) { t = t + 1; i = i + 1; } return t;", "3"),
        ("no clauses at all", "int t = 0; for (;;) { t = t + 1; if (t == 4) { break; } } return t;", "4"),
        ("the init variable does not escape", "int i = 99; for (int i = 0; i < 3; i = i + 1) { } return i;", "99"),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// `continue` in a `for` runs the step clause, so the loop still advances.
///
/// Without this the loop would spin forever on the value that triggered the `continue`, so the test
/// would hang rather than print the wrong number — which is itself the signal.
#[test]
fn continue_in_a_for_still_runs_the_step() {
    assert_eq!(
        answer(
            "for-continue",
            "int t = 0; for (int i = 0; i < 5; i = i + 1) { if (i == 2) { continue; } t = t + i; } return t;"
        ),
        "8"
    );
}

/// `break` and `continue` apply to the innermost loop enclosing them, not to any outer one.
#[test]
fn break_and_continue_bind_to_the_innermost_loop() {
    let cases = [
        (
            "break leaves only the inner loop",
            "int t = 0; for (int i = 0; i < 3; i = i + 1) { for (int j = 0; j < 3; j = j + 1) { if (j == 1) { break; } t = t + 1; } } return t;",
            "3",
        ),
        (
            "continue skips only the inner iteration",
            "int t = 0; for (int i = 0; i < 2; i = i + 1) { for (int j = 0; j < 3; j = j + 1) { if (j == 1) { continue; } t = t + 1; } } return t;",
            "4",
        ),
        (
            "three levels, break at the innermost",
            "int t = 0; for (int i = 0; i < 2; i = i + 1) { for (int j = 0; j < 2; j = j + 1) { for (int k = 0; k < 5; k = k + 1) { if (k == 2) { break; } t = t + 1; } } } return t;",
            "8",
        ),
        (
            "a while inside a for",
            "int t = 0; for (int i = 0; i < 3; i = i + 1) { int j = 0; while (1) { j = j + 1; if (j == 2) { break; } t = t + 1; } } return t;",
            "3",
        ),
    ];

    for (shape, body, expected) in cases {
        assert_eq!(answer(&slug(shape), body), expected, "{shape}");
    }
}

/// A `return` inside nested loops leaves the function, not just the loop.
#[test]
fn a_return_inside_nested_loops_leaves_the_function() {
    assert_eq!(
        answer(
            "return-from-loops",
            "for (int i = 0; i < 9; i = i + 1) { for (int j = 0; j < 9; j = j + 1) { if (i * 10 + j == 34) { return 34; } } } return 0;"
        ),
        "34"
    );
}

/// `main` that reaches its closing brace exits zero, which C defines for `main` alone.
#[test]
fn main_without_a_return_exits_zero() {
    let assembly = assemble("int main(void) { int x; x = 5; }");
    let directory = scratch("implicit-return");
    let path = directory.join("out.s");
    let binary = directory.join("program");

    fs::write(&path, &assembly).expect("could not write the assembly");
    let built = Command::new("clang")
        .args(["-std=c99", "-O0"])
        .arg(&path)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("could not run clang");
    assert!(
        built.status.success(),
        "linking failed:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );

    let run = Command::new(&binary)
        .output()
        .expect("could not run the program");

    assert_eq!(run.status.code(), Some(0));
}

/// Arguments arrive with the right values, at every arity including past the register boundary.
///
/// Nine is the case that matters: the first eight travel in registers and the ninth on the stack,
/// so it is the first arity where the caller and the callee have to agree about memory.
#[test]
fn arguments_arrive_at_every_arity() {
    let cases = [
        ("zero", "int f(void) { return 7; }\nint answer(void) { return f(); }", "7"),
        ("one", "int f(int a) { return a; }\nint answer(void) { return f(3); }", "3"),
        ("two, asymmetric", "int f(int a, int b) { return a - b; }\nint answer(void) { return f(10, 3); }", "7"),
        (
            "three, each distinguishable",
            "int f(int a, int b, int c) { return a * 100 + b * 10 + c; }\nint answer(void) { return f(1, 2, 3); }",
            "123",
        ),
        (
            "eight, the last register argument",
            "int f(int a,int b,int c,int d,int e,int g,int h,int i) { return i; }\nint answer(void) { return f(1,2,3,4,5,6,7,8); }",
            "8",
        ),
        (
            "nine, the first stack argument",
            "int f(int a,int b,int c,int d,int e,int g,int h,int i,int j) { return j; }\nint answer(void) { return f(1,2,3,4,5,6,7,8,9); }",
            "9",
        ),
        (
            "nine, every argument summed",
            "int f(int a,int b,int c,int d,int e,int g,int h,int i,int j) { return a+b+c+d+e+g+h+i+j; }\nint answer(void) { return f(1,2,3,4,5,6,7,8,9); }",
            "45",
        ),
        (
            "nine, weighted so a transposition shows",
            "int f(int a,int b,int c,int d,int e,int g,int h,int i,int j) { return a*1+b*2+c*4+d*8+e*16+g*32+h*64+i*128+j*256; }\nint answer(void) { return f(1,1,1,1,1,1,1,1,1); }",
            "511",
        ),
    ];

    for (shape, source, expected) in cases {
        assert_eq!(answer_of(&slug(shape), source), expected, "{shape}");
    }
}

/// A pointer passed on the stack arrives whole, and does not misalign what follows it.
///
/// Eight bytes, not four. On ARM64 the register name fixes the width — `ldr`/`str` are spelled the
/// same for a word and a doubleword, and `w8` is the low half of `x8` — so naming the `w` form for
/// an address writes half of it and leaves the rest as whatever was there. The width also has to
/// come from the type after decay, since the source writes an array and the callee receives an
/// address.
#[test]
fn a_pointer_passed_on_the_stack_arrives_whole() {
    let cases = [
        (
            "an array as the ninth argument",
            "int ninth(int a,int b,int c,int d,int e,int g,int h,int i,int values[]) { return values[0] + values[1]; }\nint answer(void) { int a[2]; a[0] = 40; a[1] = 2; return ninth(1,2,3,4,5,6,7,8,a); }",
            "42",
        ),
        (
            "an already-decayed parameter forwarded to a stack position",
            "int ninth(int a,int b,int c,int d,int e,int g,int h,int i,int values[]) { return values[0]; }\nint forward(int values[]) { return ninth(1,2,3,4,5,6,7,8,values); }\nint answer(void) { int a[1]; a[0] = 7; return forward(a); }",
            "7",
        ),
        (
            "an int after a pointer, which a mis-sized pointer would misalign",
            "int tenth(int a,int b,int c,int d,int e,int g,int h,int i,int values[],int last) { return values[0] * 100 + last; }\nint answer(void) { int a[1]; a[0] = 3; return tenth(1,2,3,4,5,6,7,8,a,9); }",
            "309",
        ),
        (
            "two pointers on the stack",
            "int both(int a,int b,int c,int d,int e,int g,int h,int i,int x[],int y[]) { return x[0] * 10 + y[0]; }\nint answer(void) { int p[1]; int q[1]; p[0] = 4; q[0] = 2; return both(1,2,3,4,5,6,7,8,p,q); }",
            "42",
        ),
        (
            "a char after a pointer, packed at its natural size",
            "int mixed(int a,int b,int c,int d,int e,int g,int h,int i,int values[],char c2) { return values[0] + c2; }\nint answer(void) { int a[1]; a[0] = 1; return mixed(1,2,3,4,5,6,7,8,a,'A'); }",
            "66",
        ),
    ];

    for (shape, source, expected) in cases {
        assert_eq!(answer_of(&slug(shape), source), expected, "{shape}");
    }
}

/// An argument that is itself a call does not clobber an argument already evaluated.
///
/// Proved through the returned value rather than through observable order: C leaves the order in
/// which arguments are evaluated unspecified, so a test that watched for side effects could fail
/// on a disagreement that is not a bug.
#[test]
fn nested_calls_do_not_clobber_placed_arguments() {
    let source = "\
int g(int n) { return n * 10; }
int h(int n) { return n + 1; }
int f(int a, int b) { return a - b; }
int answer(void) { return f(g(5), h(2)); }
";

    assert_eq!(answer_of("nested-calls", source), "47");
}

/// Nested calls survive at the stack-argument boundary too.
#[test]
fn nested_calls_survive_nine_arguments() {
    let source = "\
int one(int n) { return n; }
int nine(int a,int b,int c,int d,int e,int g,int h,int i,int j) { return a*1+b*2+c*4+d*8+e*16+g*32+h*64+i*128+j*256; }
int answer(void) { return nine(one(1),one(1),one(1),one(1),one(1),one(1),one(1),one(1),one(1)); }
";

    assert_eq!(answer_of("nested-nine", source), "511");
}

/// Recursion works, at the shapes that exercise it hardest.
#[test]
fn recursion_computes_what_it_should() {
    let cases = [
        (
            "factorial",
            "int fact(int n) { if (n <= 1) { return 1; } return n * fact(n - 1); }\nint answer(void) { return fact(10); }",
            "3628800",
        ),
        (
            "fibonacci",
            "int fib(int n) { if (n < 2) { return n; } return fib(n - 1) + fib(n - 2); }\nint answer(void) { return fib(20); }",
            "6765",
        ),
        (
            "ackermann at a small bound",
            "int ack(int m, int n) { if (m == 0) { return n + 1; } if (n == 0) { return ack(m - 1, 1); } return ack(m - 1, ack(m, n - 1)); }\nint answer(void) { return ack(2, 3); }",
            "9",
        ),
    ];

    for (shape, source, expected) in cases {
        assert_eq!(answer_of(&slug(shape), source), expected, "{shape}");
    }
}

/// Two functions that call each other resolve and terminate.
#[test]
fn mutual_recursion_works() {
    let source = "\
int is_odd(int n);
int is_even(int n) { if (n == 0) { return 1; } return is_odd(n - 1); }
int is_odd(int n) { if (n == 0) { return 0; } return is_even(n - 1); }
int answer(void) { return is_even(10) * 10 + is_odd(7); }
";

    assert_eq!(answer_of("mutual-recursion", source), "11");
}

/// A call to a function declared first and defined later resolves.
#[test]
fn a_forward_declared_call_resolves() {
    let source = "\
int later(int n);
int answer(void) { return later(6); }
int later(int n) { return n * 7; }
";

    assert_eq!(answer_of("forward-declared", source), "42");
}

/// A `void` function is called for its effect and returns nothing.
#[test]
fn a_void_function_is_called_as_a_statement() {
    let source = "\
void put(int values[], int index, int value) { values[index] = value; }
int answer(void) {
    int store[2];
    put(store, 0, 30);
    put(store, 1, 12);
    return store[0] + store[1];
}
";

    assert_eq!(answer_of("void-call", source), "42");
}

/// An array passed to a function is mutated in place and the caller sees the change.
///
/// This is the decay ADR 0007 permits, working end to end: the callee receives an address, not a
/// copy, so what it writes is what the caller reads back.
#[test]
fn an_array_is_mutated_through_a_call() {
    let source = "\
void fill(int values[], int count, int value) {
    for (int i = 0; i < count; i = i + 1) { values[i] = value + i; }
}
int total(int values[], int count) {
    int sum = 0;
    for (int i = 0; i < count; i = i + 1) { sum = sum + values[i]; }
    return sum;
}
int answer(void) {
    int numbers[4];
    fill(numbers, 4, 10);
    return total(numbers, 4);
}
";

    assert_eq!(answer_of("array-through-a-call", source), "46");
}

/// A `char` argument is promoted before the call and arrives as the value C says it has.
#[test]
fn char_arguments_are_promoted() {
    let source = "\
int take(int n) { return n; }
int answer(void) { char c = 200; return take(c); }
";

    assert_eq!(answer_of("char-argument", source), "-56");
}

/// Compiles `source`, links it with the runtime shim, runs it, and returns stdout.
///
/// `source` provides its own `main`, so this is the whole program rather than a function under a
/// driver. It is what a program printing through `print_string` needs.
fn output_of(name: &str, source: &str) -> String {
    let assembly = assemble(source);
    let directory = scratch(name);
    let path = directory.join("out.s");
    let binary = directory.join("program");

    fs::write(&path, &assembly).expect("could not write the assembly");

    let built = Command::new("clang")
        .args(["-std=c99", "-O0"])
        .arg(&path)
        .arg(SHIM_OBJECT)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("could not run clang; run xcode-select --install");
    assert!(
        built.status.success(),
        "linking failed:\n{}\n--- assembly ---\n{assembly}",
        String::from_utf8_lossy(&built.stderr)
    );

    let run = Command::new(&binary)
        .output()
        .expect("could not run the program");
    assert!(
        run.status.success(),
        "the program exited with {:?}",
        run.status.code()
    );

    String::from_utf8_lossy(&run.stdout).into_owned()
}

/// The declarations of the runtime shim, as a subset-C program writes them.
const SHIM: &str =
    "void print_int(int n);\nvoid print_char(char c);\nvoid print_string(char s[]);\n";

/// Globals are read and written, keep their values across calls, and start out as declared.
#[test]
fn globals_hold_their_values() {
    let cases = [
        (
            "read an initialized global",
            "int g = 7;\nint answer(void) { return g; }",
            "7",
        ),
        (
            "write then read",
            "int g = 7;\nint answer(void) { g = 9; return g; }",
            "9",
        ),
        (
            "a negative initializer",
            "int g = -12345;\nint answer(void) { return g; }",
            "-12345",
        ),
        (
            "a folded initializer",
            "int g = 2 * 3 + 1;\nint answer(void) { return g; }",
            "7",
        ),
        (
            "uninitialized starts at zero",
            "int g;\nint answer(void) { return g; }",
            "0",
        ),
        (
            "a char global",
            "char c = 'A';\nint answer(void) { return c; }",
            "65",
        ),
        (
            "a char global above 127 is negative",
            "char c = 200;\nint answer(void) { return c; }",
            "-56",
        ),
        (
            "written in one function and read in another",
            "int g;\nvoid put(int n) { g = n; }\nint answer(void) { put(42); return g; }",
            "42",
        ),
    ];

    for (shape, source, expected) in cases {
        assert_eq!(answer_of(&slug(shape), source), expected, "{shape}");
    }
}

/// Global arrays are laid out element by element, zero-filled past their initializer.
#[test]
fn global_arrays_are_laid_out_and_iterated() {
    let cases = [
        ("read an element", "int a[3] = {4, 5, 6};\nint answer(void) { return a[1]; }", "5"),
        (
            "summed in a loop",
            "int a[4] = {1, 2, 3, 4};\nint answer(void) { int t = 0; for (int i = 0; i < 4; i = i + 1) { t = t + a[i]; } return t; }",
            "10",
        ),
        (
            "a short initializer zero-fills the rest",
            "int a[4] = {1, 2};\nint answer(void) { return a[0] + a[1] * 10 + a[2] * 100 + a[3] * 1000; }",
            "21",
        ),
        (
            "uninitialized is all zeroes",
            "int a[3];\nint answer(void) { return a[0] + a[1] + a[2]; }",
            "0",
        ),
        (
            "a char array",
            "char a[3] = {1, 2, 3};\nint answer(void) { return a[0] + a[1] * 10 + a[2] * 100; }",
            "321",
        ),
        (
            "a char array from a string literal",
            "char greeting[6] = \"hello\";\nint answer(void) { return greeting[0] + greeting[4]; }",
            "215",
        ),
        (
            "the terminator is there",
            "char greeting[6] = \"hello\";\nint answer(void) { return greeting[5]; }",
            "0",
        ),
        (
            "written across a call",
            "int a[3];\nvoid put(int i, int n) { a[i] = n; }\nint answer(void) { put(2, 8); return a[2]; }",
            "8",
        ),
    ];

    for (shape, source, expected) in cases {
        assert_eq!(answer_of(&slug(shape), source), expected, "{shape}");
    }
}

/// A string literal reaches `print_string` as the address of its bytes.
#[test]
fn string_literals_print() {
    let source = format!("{SHIM}int main(void) {{ print_string(\"hello, world\"); return 0; }}\n");

    assert_eq!(output_of("string-literal", &source), "hello, world");
}

/// Escapes reach stdout as the bytes the lexer decoded, not as the text that was written.
#[test]
fn escapes_round_trip_to_their_bytes() {
    let source = format!(
        "{SHIM}int main(void) {{ print_string(\"a\\tb\\nq:\\\" s:\\\\ done\"); return 0; }}\n"
    );

    assert_eq!(output_of("escapes", &source), "a\tb\nq:\" s:\\ done");
}

/// A literal written twice is one entry in the read-only section, and prints the same both times.
#[test]
fn a_repeated_literal_is_interned_once() {
    let source = format!(
        "{SHIM}int main(void) {{ print_string(\"twice\"); print_string(\"twice\"); return 0; }}\n"
    );

    assert_eq!(output_of("interned", &source), "twicetwice");

    let assembly = assemble(&source);
    assert_eq!(
        assembly.matches(".asciz").count(),
        1,
        "two occurrences should share one entry:\n{assembly}"
    );
}

/// A `char` array holding a string is passed to the shim as the pointer it decays to.
#[test]
fn a_char_array_prints_as_a_string() {
    let source =
        format!("{SHIM}int main(void) {{ char g[6] = \"hello\"; print_string(g); return 0; }}\n");

    assert_eq!(output_of("char-array-string", &source), "hello");
}

/// Assembles `assembly` with `clang -c -Werror`, failing on anything at all on stderr.
fn assembles_cleanly(name: &str, assembly: &str) {
    let directory = scratch(name);
    let path = directory.join("out.s");
    fs::write(&path, assembly).expect("could not write the assembly");

    let assembled = Command::new("clang")
        .args(["-c", "-Werror"])
        .arg(&path)
        .arg("-o")
        .arg(directory.join("out.o"))
        .output()
        .expect("could not run clang");

    assert!(
        assembled.status.success() && assembled.stderr.is_empty(),
        "{name} did not assemble cleanly:\n{}\n--- assembly ---\n{assembly}",
        String::from_utf8_lossy(&assembled.stderr)
    );
}

/// One snapshot fixture: a name, and the program whose lowering it pins.
const CONSTRUCTS: &[(&str, &str)] = &[
    ("arithmetic", "int answer(void) { return 1 + 2 * 3; }"),
    ("subtraction", "int answer(void) { return 10 - 3; }"),
    ("remainder", "int answer(void) { return 10 % 3; }"),
    ("comparison", "int answer(void) { return 2 < 1; }"),
    (
        "short_circuit_and",
        "int answer(void) { int n = 0; return 0 && (n = 1); }",
    ),
    (
        "short_circuit_or",
        "int answer(void) { int n = 0; return 1 || (n = 1); }",
    ),
    (
        "local_round_trip",
        "int answer(void) { int x = 7; return x; }",
    ),
    (
        "array_index",
        "int answer(void) { int a[3]; a[1] = 5; return a[1]; }",
    ),
    (
        "char_round_trip",
        "int answer(void) { char c = 'A'; return c; }",
    ),
    (
        "postfix_increment",
        "int answer(void) { int i = 5; return i++; }",
    ),
    (
        "prefix_increment",
        "int answer(void) { int i = 5; return ++i; }",
    ),
    ("unary", "int answer(void) { return -!0; }"),
    ("large_constant", "int answer(void) { return 123456789; }"),
    ("if_only", "int answer(void) { int n = 0; if (n) { n = 1; } return n; }"),
    ("if_else", "int answer(void) { int n = 0; if (n) { n = 1; } else { n = 2; } return n; }"),
    ("while_loop", "int answer(void) { int n = 0; while (n < 3) { n = n + 1; } return n; }"),
    ("while_break", "int answer(void) { int n = 0; while (1) { n = n + 1; break; } return n; }"),
    ("for_full", "int answer(void) { int t = 0; for (int i = 0; i < 3; i = i + 1) { t = t + i; } return t; }"),
    ("for_no_condition", "int answer(void) { int t = 0; for (int i = 0; ; i = i + 1) { t = t + 1; break; } return t; }"),
    ("for_empty_clauses", "int answer(void) { int t = 0; for (;;) { t = 1; break; } return t; }"),
    ("for_continue", "int answer(void) { int t = 0; for (int i = 0; i < 3; i = i + 1) { continue; } return t; }"),
    ("implicit_main_return", "int main(void) { int x; x = 5; }"),
    ("call_no_arguments", "int f(void) { return 1; }\nint answer(void) { return f(); }"),
    ("call_two_arguments", "int f(int a, int b) { return a - b; }\nint answer(void) { return f(10, 3); }"),
    (
        "call_nine_arguments",
        "int f(int a,int b,int c,int d,int e,int g,int h,int i,int j) { return j; }\nint answer(void) { return f(1,2,3,4,5,6,7,8,9); }",
    ),
    ("call_nested", "int g(int n) { return n; }\nint f(int a, int b) { return a - b; }\nint answer(void) { return f(g(5), g(2)); }"),
    ("call_recursive", "int f(int n) { if (n <= 1) { return 1; } return n * f(n - 1); }\nint answer(void) { return f(5); }"),
    ("global_scalar", "int g = -5;\nint answer(void) { return g; }"),
    ("global_char", "char c = 'A';\nint answer(void) { return c; }"),
    ("global_array", "int a[3] = {4, 5, 6};\nint answer(void) { return a[0]; }"),
    ("global_array_short_initializer", "int a[4] = {1, 2};\nint answer(void) { return a[0]; }"),
    ("global_uninitialized", "int g;\nint a[8];\nint answer(void) { return g + a[0]; }"),
    (
        "two_string_literals",
        "void print_string(char s[]);\nint answer(void) { print_string(\"one\"); print_string(\"two\"); print_string(\"one\"); return 0; }",
    ),
    ("local_char_array_from_literal", "int answer(void) { char g[6] = \"hi\"; return g[0]; }"),
];

/// The emitted assembly for each construct, pinned so a regression is a readable diff.
#[test]
fn construct_snapshots() {
    for (name, source) in CONSTRUCTS {
        insta::assert_snapshot!(*name, assemble(source), source);
    }
}

/// Every construct's assembly is something the assembler accepts without comment.
///
/// A snapshot taken after a malformed directive was introduced would pin the malformed version
/// quite happily, so the two checks are kept separate.
#[test]
fn every_construct_assembles_cleanly() {
    for (name, source) in CONSTRUCTS {
        assembles_cleanly(name, &assemble(source));
    }
}

/// Lowering the same program twice produces the same bytes.
#[test]
fn lowering_is_deterministic() {
    for (name, source) in CONSTRUCTS {
        assert_eq!(
            assemble(source),
            assemble(source),
            "{name} differed between runs"
        );
    }
}
