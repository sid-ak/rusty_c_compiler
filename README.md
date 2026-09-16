# Rusty C Compiler

A compiler for a well-defined subset of C, written in Rust, emitting ARM64 assembly for Apple
Silicon macOS and linking it into a real native executable.

C was chosen over a toy language so that `clang` can serve as the testing oracle: every corpus
program is compiled by both compilers, run, and compared. That comparison is the definition of
correct.

## Status

`rustycc` compiles a subset of C to a native macOS executable. `rustycc program.c -o program`
produces a program that runs, and every program in the test corpus produces byte-for-byte the same
output as the same program built by `clang -O0 -std=c99`.

The pipeline runs end to end: source text to tokens, tokens to a syntax tree, the tree to types and
bindings, and those to ARM64 assembly that `clang` assembles and links against the runtime shim.
`--dump-tokens`, `--dump-ast`, `--dump-annotations`, and `-S` print what each stage made of a file;
`--check` answers whether a program is accepted, with diagnostics carrying a caret under the
offending text — one per mistake, in source order, and a second caret under the earlier declaration
where a name collides with one.

Not built yet: the differential harness that replaces the corpus's recorded expectations with
`clang` run side by side, the fuzzing targets, and the random program generator.

In the repo:

- `src/diagnostics.rs` — spans over byte offsets, a source map that resolves one to a line and
  column, the caret renderer every pass reports through, notes that can point at a second place in
  the file, and a bag that collects diagnostics and returns them in source order.
- `src/lexer/` — the token set and keyword table, and a scanner over raw bytes that always
  terminates, never panics, decodes literals once, and resynchronizes after a malformed construct so
  a file with four mistakes reports four of them.
- `src/ast.rs` — the node types the parser builds and the two later passes read, each carrying a
  span and an identity, with no field for anything a later pass works out; plus the deterministic
  S-expression dump that parser tests assert against.
- `src/parser/` — recursive descent over declarations and statements, precedence climbing over
  expressions, panic-mode recovery that provably consumes a token per step, and a limit on how
  deep the tree may grow that turns a hostile input into a diagnostic rather than a stack overflow.
- `src/sema/` — the type model and its promotion, decay, and compatibility rules; a scope stack
  resolving every identifier; a two-pass walk that registers the top level before it walks any body,
  so a call to a function defined later in the file resolves; thirty-one checks, each with its own
  message and span; and the annotation tables code generation reads.
- `src/codegen/` — the assembly emitter and Mach-O conventions, stack frame layout with a fixed slot
  for every value, expression lowering where each instruction reads its operands in source order,
  control flow with a loop-context stack, Apple's ARM64 calling convention, and the data sections.
- `src/driver.rs` — assembling and linking through `clang`, with intermediates removed however the
  run ends and a toolchain failure reported in the toolchain's own words.
- `tests/programs/` — seven subset-C programs covering the grammar, each carrying the exit code and
  stdout `clang` produces for it, and thirty-one in `invalid/` that must stay rejected. Both have a
  coverage matrix CI holds them to.
- `runtime/shim.c` — `print_int`, `print_char`, and `print_string` on `write(2)`, compiled once by
  the build script into the object both compilers link against.
- `.github/workflows/ci.yml` — fmt, clippy, test, and docs on an Apple Silicon runner, behind a
  preflight that checks the C toolchain resolves.

Four programs in `tests/programs/invalid/` are real C that `clang` builds and this compiler rejects
on purpose. They are listed in
[`docs/architecture.md`](docs/architecture.md#where-this-subset-is-stricter-than-c), and a test fails
if that list and the corpus disagree.

Differential testing, fuzzing, and system acceptance are open, tracked as
[GitHub issues](https://github.com/sid-ak/rusty_c_compiler/issues) under one milestone per phase.
The design and subset grammar are in [`docs/architecture.md`](docs/architecture.md), the phased plan
in [`docs/PLAN.md`](docs/PLAN.md), ten ADRs in [`docs/decisions/`](docs/decisions/index.md), the
commands for driving it by hand in [`docs/CHEATSHEET.md`](docs/CHEATSHEET.md), and the working
conventions in [`AGENTS.md`](AGENTS.md).

This section is refreshed every iteration, so it records where the project actually is.

## Scope

### Subset

- `int`, `char`, and `void`
- Functions with recursion and forward declarations
- single-dimension arrays that decay to a pointer only at a call boundary
- `if`/`else`, `while`, `for`, `break`, `continue`, `return`
- C precedence with real short-circuiting;
- string literals

### Out of Subset

Out of scope, each with its own diagnostic rather than a parse error:

- The preprocessor, `struct`, `switch`, `do`/`while`, the ternary, compound assignment, bitwise
  operators, floating point, multi-dimensional arrays, pointer variables, `&`, `*`, variadics, and
  `sizeof`.
- Constructs C leaves undefined are also rejected, since undefined behavior cannot be differentially
  tested ([ADR 0008](docs/decisions/0008-reject-undefined-behavior.md)).

The full grammar is in [`docs/architecture.md`](docs/architecture.md#the-language-subset).

## Prerequisites

- Apple Silicon Mac
- Xcode Command Line Tools (`xcode-select --install`)
- Rust stable
- Nightly is needed only for `cargo-fuzz`

## Building

1. `cargo build`: build `rustycc`. The build script compiles `runtime/shim.c` with `clang`, so the
   Xcode Command Line Tools have to be installed first.
2. `cargo test`: the whole suite — unit tests, the token-stream snapshot, and the runtime shim
   compiled, linked, and run.
3. `cargo fmt --check && cargo clippy --all-targets -- -D warnings`: the lint gates CI enforces.

To see what the compiler makes of a file:

1. `./target/debug/rustycc program.c --dump-tokens`: print each token with the source range it came
   from.
2. `./target/debug/rustycc program.c --dump-ast`: print the syntax tree the parser built.
3. `./target/debug/rustycc program.c --dump-annotations`: print the types, conversions, bindings,
   frame inventories, and interned string literals analysis recorded.
4. `./target/debug/rustycc --check program.c`: run the whole front end and answer with an exit code,
   printing nothing when the program is accepted.
5. `./target/debug/rustycc --check broken.c`: on a rejected file, print a diagnostic with the
   offending line and a caret, and exit non-zero.
6. `./target/debug/rustycc program.c -S -o program.s`: write the ARM64 assembly and no binary.
7. `./target/debug/rustycc program.c -o program && ./program`: compile, link, and run it.

A program that calls `print_int`, `print_char`, or `print_string` declares them itself — there is no
preprocessor, so there is no header to include — and the driver links the shim in automatically.

The toolchain is pinned in `rust-toolchain.toml`, so `cargo` installs the right compiler on its own.
Complete instructions for building and running every tier of tests are a Phase 5 deliverable
([#38](https://github.com/sid-ak/rusty_c_compiler/issues/38)).

The documentation site builds too:

1. `uv venv && uv pip install -r requirements-docs.txt`: install MkDocs.
2. `uv run mkdocs serve`: serve locally with live reload.
3. `uv run mkdocs build --strict`: build, failing on a broken link or a page missing from the `nav`.

## Documentation

[Architecture](docs/architecture.md) is the place to start — the passes, the grammar, the codegen
strategy, and the testing architecture. Then [Implementation Plan](docs/PLAN.md),
[Decisions](docs/decisions/index.md), the original [Proposal](docs/PROPOSAL.md), and
[AGENTS.md](AGENTS.md) for how to work in the repo.
