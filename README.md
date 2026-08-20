# Rusty C Compiler

A compiler for a well-defined subset of C, written in Rust, emitting ARM64 assembly for Apple
Silicon macOS and linking it into a real native executable.

C was chosen over a toy language so that `clang` can serve as the testing oracle: every corpus
program is compiled by both compilers, run, and compared. That comparison is the definition of
correct.

## Status

The front end reads. `rustycc` builds and runs from a clean checkout, turns a C source file into a
syntax tree covering the whole subset grammar, and prints it with `rustycc program.c --dump-ast`. A
program it cannot read comes back as diagnostics with a caret under the offending text — one per
mistake, in source order, and a construct that is real C this subset simply does not implement is
told apart from one that is malformed.

It does not yet check what a program means or generate code, so it cannot produce an executable:
`rustycc program.c -o program` accepts its arguments and runs the stages that exist.

In the repo:

- `src/diagnostics.rs` — spans over byte offsets, a source map that resolves one to a line and
  column, the caret renderer every pass reports through, and a bag that collects diagnostics and
  returns them in source order.
- `src/lexer/` — the token set and keyword table, and a scanner over raw bytes that always
  terminates, never panics, decodes literals once, and resynchronizes after a malformed construct so
  a file with four mistakes reports four of them.
- `src/ast.rs` — the node types the parser builds and the two later passes read, each carrying a
  span and an identity, with no field for anything a later pass works out; plus the deterministic
  S-expression dump that parser tests assert against.
- `src/parser/` — recursive descent over declarations and statements, precedence climbing over
  expressions, panic-mode recovery that provably consumes a token per step, and a nesting limit that
  turns a hostile input into a diagnostic rather than a stack overflow.
- `tests/programs/` — five subset-C programs, one per feature area, with a coverage matrix that CI
  holds them to. They are the parser's snapshots now and the differential corpus later.
- `runtime/shim.c` — `print_int`, `print_char`, and `print_string` on `write(2)`, compiled once by
  the build script into the object both compilers will link in differential testing.
- `.github/workflows/ci.yml` — fmt, clippy, and test on an Apple Silicon runner, behind a preflight
  that checks the C toolchain resolves.

Semantic analysis, code generation, and the differential suite are open, tracked as
[GitHub issues](https://github.com/sid-ak/rusty_c_compiler/issues) under one milestone per phase.
The design and subset grammar are in [`docs/architecture.md`](docs/architecture.md), the phased plan
in [`docs/PLAN.md`](docs/PLAN.md), nine ADRs in [`docs/decisions/`](docs/decisions/index.md), the
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
2. `./target/debug/rustycc broken.c --dump-tokens`: on a malformed file, print a diagnostic with the
   offending line and a caret, and exit non-zero.

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
