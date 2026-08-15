# Rusty C Compiler

A compiler for a well-defined subset of C, written in Rust, emitting ARM64 assembly for Apple
Silicon macOS and linking it into a real native executable.

C was chosen over a toy language so that `clang` can serve as the testing oracle: every corpus
program is compiled by both compilers, run, and compared. That comparison is the definition of
correct.

## Status

Design and planning are complete; implementation has not started. There is no `Cargo.toml` and no
`src/` yet, so nothing builds or runs from a clean checkout. All five phases — lexer, parser,
semantic analysis, code generation, differential testing — are open, tracked as
[GitHub issues](https://github.com/sid-ak/rusty_c_compiler/issues) under one milestone each.

What exists is the documentation: the design and subset grammar in
[`docs/architecture.md`](docs/architecture.md), the phased plan in [`docs/PLAN.md`](docs/PLAN.md),
nine ADRs in [`docs/decisions/`](docs/decisions/index.md), and working conventions in
[`AGENTS.md`](AGENTS.md).

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

Not yet applicable. Once Phase 1 lands the entry points are `cargo build` and `cargo test`, with the
CLI contract `mycc program.c -o program`; full instructions are a Phase 5 deliverable
([#38](https://github.com/sid-ak/rusty_c_compiler/issues/38)).

The documentation site builds today:

1. `uv venv && uv pip install -r requirements-docs.txt`: install MkDocs.
2. `uv run mkdocs serve`: serve locally with live reload.
3. `uv run mkdocs build --strict`: build, failing on a broken link or a page missing from the `nav`.

## Documentation

[Architecture](docs/architecture.md) is the place to start — the passes, the grammar, the codegen
strategy, and the testing architecture. Then [Implementation Plan](docs/PLAN.md),
[Decisions](docs/decisions/index.md), the original [Proposal](docs/PROPOSAL.md), and
[AGENTS.md](AGENTS.md) for how to work in the repo.
