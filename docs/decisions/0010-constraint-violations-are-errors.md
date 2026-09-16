# ADR 0010 — Constraint violations are errors, not warnings

- Status: Accepted
- Date: 2026-09-15

## Context

[ADR 0001](0001-subset-of-c-with-clang-as-oracle.md) makes `clang` the oracle: where this compiler
and `clang` disagree about a program, `clang` is right and this compiler has a bug. Building the
invalid-program corpus in Phase 3 turned up two programs where that framing does not quite hold.

```c
int a[2] = {1, 2, 3};   // three initializers for two elements
int b[0];               // an array of nothing
```

Both are rejected here. Neither is rejected by `clang -O0 -std=c99`. The first draws a warning
(`-Wexcess-initializers`) and then quietly drops the third value; the second is accepted outright,
as a GNU extension, and only mentions itself under `-pedantic` (`-Wzero-length-array`).

The C99 standard is not ambiguous about either. An initializer may not provide a value for
something outside the object being initialized (6.7.8p2), and an array's size expression "shall have
a value greater than zero" (6.7.5.2p1). Both are constraints, and a program that violates a
constraint is not a C program. What `clang` does instead is a deliberate kindness to existing code:
it diagnoses and carries on, because refusing to build a large old codebase over an excess
initializer would help nobody.

That kindness is not free to copy. Accepting a constraint violation means deciding what the program
then means — which value to drop, how much storage an array of nothing gets — and those decisions
are invented rather than specified.

## Decision

A constraint violation is an error here, even where `clang` diagnoses it as a warning or accepts it
as an extension.

This narrows the accepted language rather than widening it. Every program this compiler accepts is
still one `clang` accepts, which is the direction that keeps differential testing meaningful: the
corpus compares programs both compilers build, and a program rejected here never reaches it.

The oracle relationship is unchanged and worth restating precisely, because this ADR could be
misread as weakening it. `clang` remains the authority on what an accepted program *does*. It is not
the authority on what this subset accepts — the grammar in
[architecture.md](../architecture.md) and the checks in Phase 3 are. A disagreement about behavior
is a bug. A disagreement about acceptance, in this direction only, is a decision, and every one of
them is written down in the file that provokes it and mirrored in the architecture document.

## Consequences

The invalid-program corpus can contain programs `clang` builds. Each such file states in its own
header that `clang` accepts it, and why this compiler does not, so the cross-check test can tell an
intentional deviation from a compiler bug instead of treating every disagreement as one.

A reader coming from real C may be surprised. `int a[0]` compiles everywhere they have tried it.
The error message and this record are what make that a decision they can look up rather than a
defect they have to guess at.

The risk is that this becomes a licence to reject anything inconvenient. It is not: the test that
enforces the cross-check fails when a file in `tests/programs/invalid/` is accepted by `clang`
without a recorded deviation, so adding one is a visible act with a reason attached, not a quiet
loosening of the standard being held to.

## Alternatives considered

Match `clang` exactly, warning where it warns and accepting what it accepts. Keeps the oracle
relationship simple at the cost of having to define what an over-long initializer or a zero-length
array means. Rejected: the definitions would be invented, and this subset exists to be small and
explicable rather than compatible with every extension.

Compile the corpus with `-pedantic-errors` so `clang` rejects these too, and keep "clang rejects
everything we reject" literally true. Attractive, and it does work for the zero-length array. It
does not work for the excess initializer, which stays a warning, and it would change the oracle's
configuration to fit the answer wanted from it. Rejected: the deviation is real, and recording it is
more honest than tuning the flags until it disappears.
