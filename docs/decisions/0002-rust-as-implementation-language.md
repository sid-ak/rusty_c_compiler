# ADR 0002 — Rust as the implementation language

- Status: Accepted
- Date: 2026-08-10

## Context

A compiler is mostly two activities: building and walking trees, and managing a lot of small,
interlinked data with distinct shapes. The implementation language determines how much of the effort
goes into that and how much goes into fighting the language.

The traditional choice is C or C++, which is what most compiler literature assumes. The functional
choice is OCaml or Haskell, which model ASTs particularly well. Both were considered against the
project's actual goal, which is to learn compiler construction, not to learn a new host language or
to debug memory management.

## Decision

The compiler is implemented in Rust, built with Cargo.

## Consequences

An AST is a tree of variants, which is exactly what a Rust enum is. A `match` over `Expr` is
exhaustive by construction, so adding a node type produces a compile error at every site that must
now handle it. During a build where the AST changes repeatedly — every phase adds nodes — this turns
"did I update every walker?" from a review question into a build failure.

Memory safety removes an entire category of bug that is otherwise a first compiler's dominant time
sink: dangling pointers into an arena, double frees on an AST, and use-after-free of a symbol table
entry simply cannot happen. The effort stays on compiler logic.

`Result` and the absence of exceptions push errors into the type system, which suits a compiler that
must accumulate many diagnostics and never abort. The no-panic invariant in
[architecture.md](../architecture.md#the-pipeline) is enforceable because the escape hatches —
`unwrap`, `expect`, `panic!`, unchecked indexing — are individually lintable and denied in `src/`.

Cargo gives testing, snapshot testing, fuzzing (`cargo-fuzz`), formatting, and linting without
assembling a toolchain by hand. The differential harness is ordinary `#[test]` code rather than a
shell script.

The costs are real. Borrow checking makes graph-shaped data awkward, which is part of why the AST is
a tree with side-table annotations rather than a mutable graph — see
[ADR 0004](0004-immutable-ast-with-side-table-annotations.md). Compiler literature is written in C
and C++, so example code needs translating rather than copying. And `cargo-fuzz` needs a nightly
toolchain, so the fuzz job is pinned separately from the compiler build.

## Alternatives considered

C or C++. Closest to the literature and to the domain's tradition, and the natural choice for a
self-hosting compiler later. Rejected because manual memory management on tree-shaped data is where
a first compiler loses its time, and none of that time teaches compiler construction.

OCaml or Haskell. Algebraic data types and pattern matching are arguably an even better fit for an
AST, and both have strong compiler-writing traditions. Rejected on the strength of the surrounding
tooling — Cargo's integrated test, snapshot, and fuzz story is the deciding factor for a project
whose thesis is that testing is the interesting part.

Python. Fastest to write, and the AST modelling is fine. Rejected: no exhaustiveness checking, so
the "did every walker get updated?" problem returns as a runtime error, and emitting a native
binary from it is no easier.
