# Acceptance Report

## What is being claimed

The project proposal set one criterion for functional completeness, and it is a criterion about
behavior rather than about features:

> every program in the curated suite, covering every supported language feature, produces identical
> behavior under `rustycc` and under `clang -O0`

This document records the run that meets it. It is a record rather than a statement of intent: the
numbers below were read off a real run, and every command that produced them is in
[the cheatsheet](../CHEATSHEET.md) for anyone who wants to produce them again.

## What "identical behavior" means here

Each program is built twice — once by `rustycc`, once by `clang -O0 -std=c99 -Wall` — with both
binaries linked against the same runtime object, so the comparison is between two compilers and not
between two runtimes. Both are then run with no arguments, with empty input, and with a wall-clock
limit, and three things are compared:

- Everything written to standard output, byte for byte.
- Everything written to standard error.
- How the program ended — an exit status, which is the low eight bits of what `main` returned, or
  death by a signal, which is kept distinct from returning that same number.

A program one compiler would not build is reported separately from one that behaved differently,
and a program that never finished is reported separately from both. Collapsing those three into a
single "failed" is how a report stops telling anyone where to look.

TABLE-GOES-HERE

## Undefined behavior, and why the corpus avoids it

`clang` is an oracle only for programs whose behavior C actually defines. For a program that divides
by zero, overflows an `int`, reads past the end of an array, or reads a variable that was never
assigned, the standard permits any behavior at all — so two compilers disagreeing about one would be
evidence of nothing, and two agreeing would be evidence of less.

Every program in the corpus therefore stays inside defined behavior, and that is checked rather than
asserted: `clang` is run over each one with `-Wall -Wextra`, and every warning it produces has to be
one the program's own header declares it is provoking on purpose. Seven programs in the corpus do
declare one:

| Warning | Declared by | What the program is testing |
| --- | --- | --- |
| `-Wlogical-op-parentheses` | `arithmetic.c`, `precedence.c` | that `&&` binds tighter than `\|\|`, which is the thing the warning exists to point at |
| `-Wdangling-else` | `control_flow.c`, `dangling_else.c` | that an `else` with two unbraced `if`s in front of it belongs to the nearer one |
| `-Wconstant-conversion` | `char_arithmetic.c`, `char_comparison.c` | that a value above 127 stored in a `char` is negative on this target, which is implementation-defined rather than undefined |
| `-Wunused-value` | `empty_statements.c` | that an expression statement whose value is discarded is a statement the grammar has |

Adding the parentheses `-Wall` asks for would delete the first two tests outright. Declaring the
warning instead keeps the program honest and keeps a *new* warning — the kind that means something is
actually wrong — from arriving unnoticed among the expected ones.

The generated corpus makes the same guarantee by construction, through interval arithmetic over
every expression it builds, and that reasoning is checked by a third party: a sample of the
generated programs is compiled with `clang`'s undefined-behavior sanitizer and run, and the
sanitizer reports nothing.

## Where this compiler is stricter than C

Four programs in `tests/programs/invalid/` are real C that `clang` builds and this compiler rejects
on purpose. They are listed with their reasons in
[the architecture document](../architecture.md#where-this-subset-is-stricter-than-c), and a test
fails if that list and the corpus ever disagree.

All four narrow the accepted language rather than widening it, which is the direction that keeps the
comparison meaningful: a program this compiler accepts is still a program `clang` accepts. A
deviation in the other direction — something accepted here that `clang` rejects — would have no
oracle at all.

## What this does not claim

Worth stating plainly, because an acceptance report is exactly the document people read too
generously:

- It claims nothing about programs outside the subset. The subset is small and deliberately so.
- It claims nothing about performance. Every value lives on the stack and is loaded for the instant
  it is used, which is roughly an order of magnitude slower than what `clang -O0` emits.
- It claims nothing about programs whose behavior C leaves undefined, for the reason above.
- The fuzz result is a statement about what has been run, not a proof that no crashing input exists.
  Fifteen minutes per target found nothing; a longer run might.
