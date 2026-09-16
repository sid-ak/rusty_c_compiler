# Rusty C Compiler

A C compiler in Rust that produces real ARM64 executables for Apple Silicon. No interpreter, no
virtual machine, no transpiling to something else — the output is the same kind of file `clang`
would hand you.

```c
void print_int(int n);
void print_char(char c);

int fib(int n) {
    if (n < 2) { return n; }
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    for (int i = 0; i < 10; i = i + 1) {
        print_int(fib(i));
        print_char(' ');
    }
    print_char('\n');
    return 0;
}
```

```console
$ rustycc fib.c -o fib && ./fib
0 1 1 2 3 5 8 13 21 34
```

## The Oracle

Every compiler project hits the same wall: how do you know the output is right? Writing the expected
answer by hand means being a compiler, which is the thing you were trying to build.

So this project doesn't. It compiles a subset of real C, which means `clang` can compile the exact
same file. Build both, run both, compare what they printed and how they exited. Agreement is
evidence that nobody had an opinion about; disagreement names a bug with a program attached to it.

That one decision shapes everything else. The language is a subset of C rather than something
invented, because an invented language has no second implementation to check against. Programs that
C leaves undefined are rejected rather than compiled, because two compilers doing different things
with undefined behaviour proves nothing — and two doing the same thing proves nothing either.

Sixty-four programs go through that comparison on every run. A seeded generator writes more of them
in shapes nobody would choose, and three fuzz targets throw bytes that aren't programs at all.

## Why macOS only, and Apple Silicon only

This is deliberate, not unfinished.

The compiler emits ARM64 instructions, follows Apple's AAPCS64 calling convention, and writes Mach-O
sections with Mach-O symbol naming — and does all of that directly, with no target abstraction and
no intermediate representation in between. Change the machine and essentially all of `src/codegen/`
becomes wrong: different instructions, different registers, a different object format, a different
rule for where the ninth argument to a function goes.

The upside is a backend you can read end to end. Target-specific facts sit in plain view where they
are used — the leading underscore on every Mach-O symbol, the 16-byte stack alignment at every call,
`__TEXT,__cstring` — instead of hiding behind an interface designed to make them interchangeable.

It also keeps the testing honest. The suite compiles a program and runs it, natively, as part of
`cargo test`. No emulator, no cross-linker, no remote runner, and `clang` on the same machine is
answering about the same architecture. A portability layer would have bought a second target nobody
was going to build, at the cost of the two things that make this project work.

The reasoning in full is [ADR 0003](docs/decisions/0003-single-target-arm64-macos.md).

## When your program is wrong

```console
$ rustycc --check oops.c
oops.c:3:9: error: redeclaration of 'total' in this scope
    int total;
        ^~~~~
oops.c:2:9: note: previous declaration of 'total' is here
    int total;
        ^~~~~
oops.c:4:12: error: undeclared identifier 'missing'
    return missing;
           ^~~~~~~
rustycc: 2 errors generated
```

Every mistake in the file, in source order, not just the first. A caret under the exact text, and a
second caret pointing at the other declaration when two names collide. Real C that this subset
leaves out — `struct`, `switch`, `?:` — says so by name rather than failing as a mystery syntax
error.

## Quick start

You will need an Apple Silicon Mac, Rust stable, and the Xcode Command Line Tools
(`xcode-select --install`). The Rust toolchain is pinned, so `cargo` sorts itself out.

```bash
cargo build                       # builds rustycc, and the runtime shim with clang
cargo test                        # every tier, about a minute on an M1
./target/debug/rustycc program.c -o program
```

`cargo build` leaves the compiler at `target/debug/rustycc`. The examples on this page write plain
`rustycc`, so to copy them straight out, put it on your path:

```bash
cargo install --path .
```

One wrinkle worth knowing: the runtime shim is compiled by the build script and the installed
compiler links against it where it was built, inside `target/`. So `cargo clean` will break the
installed binary until you run `cargo install --path .` again. It says so plainly when it happens —
the driver reports the linker's own words — but it is a surprising thing to meet without warning.

There is no preprocessor, so there is no header to include. A program that wants to print declares
the three runtime functions itself — `print_int`, `print_char`, `print_string` — and the driver
links them in.

To watch the compiler think, each stage will show its work:

| Flag | What it prints |
|---|---|
| `--dump-tokens` | every token, with the source range it came from |
| `--dump-ast` | the syntax tree |
| `--dump-annotations` | types, bindings, conversions, frame layouts, interned strings |
| `--check` | nothing, if the program is good; diagnostics and a non-zero exit if not |
| `-S` | the ARM64 assembly |

[`docs/CHEATSHEET.md`](docs/CHEATSHEET.md) has worked examples of all of them, with real output.

## What's in the language

In:

- `int`, `char`, `void`
- functions, including recursion and forward declarations
- single-dimension arrays, which become pointers at a call boundary and nowhere else
- `if`/`else`, `while`, `for`, `break`, `continue`, `return`
- full C precedence, with short-circuiting that genuinely short-circuits
- string literals

Out, each with a diagnostic that names it rather than a generic parse error:

- the preprocessor, `struct`, `switch`, `do`/`while`, `?:`, compound assignment, bitwise operators,
  floating point, multi-dimensional arrays, pointer variables, `&`, `*`, variadics, `sizeof`
- anything C leaves undefined, which cannot be differentially tested
  ([ADR 0008](docs/decisions/0008-reject-undefined-behavior.md))

Four programs are real C that `clang` accepts and this compiler turns down on purpose. They are
listed, with reasons, in
[the architecture](docs/architecture.md#where-this-subset-is-stricter-than-c), and a test fails if
that list and the corpus ever disagree.

The full grammar lives in
[`docs/architecture.md`](docs/architecture.md#the-language-subset).

## Status

Functionally complete against its own definition of done: sixty-four corpus programs, built by both
compilers, run, compared byte for byte on stdout, stderr, and exit status. No known mismatches. The
acceptance run is recorded in [`docs/reports/acceptance.md`](docs/reports/acceptance.md).

All five phases of [`docs/PLAN.md`](docs/PLAN.md) are done. This section is refreshed every
iteration, so it records where the project actually is.

## The tour

| Where | What is in it |
|---|---|
| `src/diagnostics.rs` | spans, the source map, the caret renderer, notes that point at a second place |
| `src/lexer/` | a byte scanner that always terminates, decodes literals once, and resynchronizes after a bad one |
| `src/ast.rs` | the node types, never mutated after parsing, plus the S-expression dump |
| `src/parser/` | recursive descent and precedence climbing, with recovery and a depth limit |
| `src/sema/` | types, scopes, a two-pass walk, thirty-one checks, and the tables the backend reads |
| `src/codegen/` | the emitter, stack frames, expression and statement lowering, calls, data sections |
| `src/driver.rs` | assembling and linking through `clang`, and cleaning up after itself |
| `runtime/shim.c` | `print_int`, `print_char`, `print_string`, on `write(2)` and nothing else |
| `tests/programs/` | sixty-four programs that must work, thirty-one that must be rejected |
| `tests/differential.rs`, `tests/harness/` | build both ways, run both, compare; and the harness's own self-tests |
| `tests/generator/` | random well-typed programs, kept defined by interval arithmetic rather than by hope |
| `fuzz/` | three targets over the front end, seeded from whatever is already in the repo |

## Testing

`cargo test` runs every tier. Each one catches what the tier below it cannot, and each can be run
alone, which is what to do when one of them goes red.

1. `cargo test --lib`: unit tests, no C compiled. Answers in under a second.
2. `cargo test --test lexer_snapshots --test parser_snapshots --test sema_snapshots --test codegen_snapshots`:
   tokens, tree, annotations, and assembly against checked-in files.
3. `cargo test --test codegen_exec`: every corpus program compiled, run, and checked.
4. `cargo test --test differential`: the acceptance suite — both compilers, both binaries, compared.
5. `cargo test --test generated`: the same comparison, over programs nobody wrote.
6. `cargo test --test invalid_programs`: the programs that must stay rejected.
7. `cargo test --test frontend_no_panic`: truncated, adversarial, and previously-crashing input.
8. `cargo test --test harness_self_tests`: the harness fed wrong answers on purpose, because a
   harness that cannot fail proves nothing.

When a differential test fails it names a directory holding both binaries, both captures of their
output, and the assembly `rustycc` produced, so the failure can be taken apart without reproducing
it first.

Fuzzing needs a nightly toolchain and `cargo-fuzz`, which the compiler itself does not:

```bash
rustup toolchain install nightly && cargo install cargo-fuzz
./scripts/fuzz.sh lex             # also: parse, frontend
```

Environment overrides, snapshot triage, crash minimization, and the rest are in
[`docs/CHEATSHEET.md`](docs/CHEATSHEET.md).

## Documentation

[Architecture](docs/architecture.md) is the place to start: the passes, the grammar, the code
generation strategy, and the testing architecture, each with a dive-deeper for the exact detail.

From there — [the phase explanations](docs/explanations/index.md) narrate how it was built,
[Decisions](docs/decisions/index.md) holds the ten ADRs and the arguments behind them,
[the plan](docs/PLAN.md) is what was built when, [the proposal](docs/PROPOSAL.md) is where it
started, and [AGENTS.md](AGENTS.md) is how to work in the repo.
