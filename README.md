# Rusty C Compiler

An ahead-of-time compiler for a defined subset of C, implemented in Rust, targeting ARM64 macOS on
Apple Silicon. It lowers source to AArch64 assembly and drives the system toolchain to assemble and
link a native Mach-O executable.

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

## Correctness model

The accepted language is a subset of C rather than an invented one, so `clang` can compile the same
translation unit. Correctness is therefore established by differential testing: each corpus program
is built by both compilers, both binaries are executed under a wall-clock timeout, and stdout,
stderr, and exit status are compared byte for byte. `clang` supplies the expected result, so no
expected output is authored by hand.

This makes the oracle's domain the binding constraint. Where C leaves behavior undefined, neither
implementation is obliged to do anything in particular, so agreement and disagreement are both
uninformative. The subset therefore excludes statically detectable undefined behavior, and the
corpus is constrained to defined programs by review and, for generated programs, by construction.

Three tiers run against that model: a curated corpus of sixty-four programs, a seeded generator that
emits random well-typed programs, and three coverage-guided fuzz targets over the front end.

## Target restriction

Code generation targets ARM64 macOS exclusively. The backend emits AArch64 instructions, implements
Apple's AAPCS64 variant, and writes Mach-O sections and symbol names directly, with no target
abstraction and no target-independent intermediate representation. Retargeting would invalidate
substantially all of `src/codegen/`: instruction selection, register conventions, object format, and
the stack-argument layout Apple packs differently from generic AAPCS64.

The restriction buys two properties:

- A backend that reads end to end. Target-specific facts — Mach-O's leading underscore, 16-byte
  stack alignment at call sites, `__TEXT,__cstring` — appear where they are used rather than behind
  an abstraction intended to make them interchangeable.
- In-process native testing. The suite compiles and executes programs as part of `cargo test`, with
  no emulator, cross-linker, or remote runner, and with `clang` on the same machine acting as oracle
  for the same architecture.

The trade-off is recorded in [ADR 0003](docs/decisions/0003-single-target-arm64-macos.md).

## Diagnostics

Diagnostics are accumulated rather than fatal: a translation unit is analyzed to completion and every
defect is reported in source order. Each carries a span rendered as a caret under the offending
text, and collisions carry a secondary span pointing at the earlier declaration.

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

Constructs that are valid C but outside the subset — `struct`, `switch`, `?:` — are reported by name
rather than as a generic syntax error.

## Requirements

Apple Silicon hardware, Rust stable, and the Xcode Command Line Tools
(`xcode-select --install`), which supply the `clang` used for assembly, linking, and differential
comparison. The Rust toolchain is pinned in `rust-toolchain.toml`.

```bash
cargo build                  # compiler, plus runtime/shim.c via the build script
cargo test                   # all tiers; approximately one minute on an M1
cargo install --path .       # optional: place rustycc on PATH
```

`cargo install --path .` links the installed binary against the shim object under `target/`, so
`cargo clean` invalidates it until the install is repeated.

The subset has no preprocessor. A translation unit declares the runtime functions it uses —
`print_int`, `print_char`, `print_string` — and the driver links the shim automatically.

## Usage

| Flag | Effect |
|---|---|
| `--dump-tokens` | token stream with source spans |
| `--dump-ast` | syntax tree |
| `--dump-annotations` | resolved types, bindings, conversions, frame layouts, interned literals |
| `--check` | front end only; exit status reports acceptance |
| `-S` | AArch64 assembly |
| `-c` | object file |
| `-o <file>` | output path |

Worked examples with real output are in [`docs/CHEATSHEET.md`](docs/CHEATSHEET.md).

## Language subset

Supported: `int`, `char`, and `void`; functions with recursion and forward declarations;
single-dimension arrays, which decay to a pointer only at a parameter boundary; `if`/`else`,
`while`, `for`, `break`, `continue`, and `return`; full C operator precedence with branch-based
short-circuit evaluation; and string literals.

Excluded, each reported by name: the preprocessor, `struct`, `switch`, `do`/`while`, the conditional
operator, compound assignment, bitwise operators, floating point, multi-dimensional arrays, pointer
variables, `&`, `*`, variadic functions, and `sizeof`. Statically detectable undefined behavior is
also rejected ([ADR 0008](docs/decisions/0008-reject-undefined-behavior.md)).

Four programs in the invalid corpus are accepted by `clang` and rejected here deliberately. They are
enumerated with rationale in
[the architecture](docs/architecture.md#where-this-subset-is-stricter-than-c), and a test fails if
that enumeration and the corpus diverge. The grammar is in
[`docs/architecture.md`](docs/architecture.md#the-language-subset).

## Status

Complete against its stated acceptance criterion: sixty-four corpus programs, compiled by both
implementations, executed, and compared on stdout, stderr, and exit status, with no known
mismatches. The run is recorded in [`docs/reports/acceptance.md`](docs/reports/acceptance.md). All
five phases of [`docs/PLAN.md`](docs/PLAN.md) are complete.

This section is refreshed every iteration and records the project's actual state.

## Source layout

| Path | Contents |
|---|---|
| `src/diagnostics.rs` | spans, source map, caret renderer, spanned notes, diagnostic bag |
| `src/lexer/` | byte scanner with literal decoding and error resynchronization |
| `src/ast.rs` | node types, immutable after parsing, and the S-expression dump |
| `src/parser/` | recursive descent and precedence climbing, with recovery and a depth bound |
| `src/sema/` | type model, scope stack, two-pass analysis, thirty-one checks, annotation tables |
| `src/codegen/` | emitter, frame layout, expression and statement lowering, calls, data sections |
| `src/driver.rs` | toolchain invocation, temporary-file management, error propagation |
| `runtime/shim.c` | `print_int`, `print_char`, `print_string`, implemented on `write(2)` |
| `tests/programs/` | sixty-four valid programs and thirty-one that must be rejected |
| `tests/differential.rs`, `tests/harness/` | the comparison harness and its self-tests |
| `tests/generator/` | random well-typed program generation bounded by interval arithmetic |
| `fuzz/` | three `cargo-fuzz` targets over the front end |

## Testing

`cargo test` runs every tier; each is independently invocable.

1. `cargo test --lib`: in-crate unit tests, no compilation or linking.
2. `cargo test --test lexer_snapshots --test parser_snapshots --test sema_snapshots --test codegen_snapshots`:
   token stream, syntax tree, annotations, and emitted assembly against checked-in snapshots.
3. `cargo test --test codegen_exec`: corpus programs compiled, executed, and checked against
   recorded expectations.
4. `cargo test --test differential`: the acceptance suite, one test per program.
5. `cargo test --test generated`: the same comparison over generated programs.
6. `cargo test --test invalid_programs`: programs that must be rejected, each against its rule.
7. `cargo test --test frontend_no_panic`: truncated, adversarial, and regression inputs.
8. `cargo test --test harness_self_tests`: fault injection on each comparison axis of the harness.

A differential failure reports a directory containing both binaries, both output captures, and the
emitted assembly, so it can be diagnosed without reproduction.

Fuzzing requires a nightly toolchain and `cargo-fuzz`, which the compiler itself does not:

```bash
rustup toolchain install nightly && cargo install cargo-fuzz
./scripts/fuzz.sh lex        # also: parse, frontend
```

Environment overrides, snapshot triage, and crash minimization are documented in
[`docs/CHEATSHEET.md`](docs/CHEATSHEET.md).

## Documentation

[Architecture](docs/architecture.md) is the entry point: pipeline, grammar, code generation strategy,
and testing architecture, each with a dive-deeper section for exact detail. Beyond it,
[the phase explanations](docs/explanations/index.md) cover the implementation in order,
[Decisions](docs/decisions/index.md) holds ten ADRs, [the plan](docs/PLAN.md) records scope per
phase, [the proposal](docs/PROPOSAL.md) is the original statement of intent, and
[AGENTS.md](AGENTS.md) documents repository conventions.
