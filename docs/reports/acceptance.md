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

## The run

| | |
| --- | --- |
| Date | 2026-09-16 |
| Machine | Apple Silicon Mac, macOS 26.6.2 (build 25G83), `arm64` |
| Oracle | Apple clang 17.0.0 (clang-1700.4.4.1), invoked as `clang -O0 -std=c99 -Wall` |
| Compiler under test | `rustycc` 0.1.0, built with rustc 1.97.1 |
| Fuzzing toolchain | `cargo-fuzz` 0.13.2 on nightly, which the compiler itself does not need |

The run was performed against commit `d1666f8`, which is the parent of the commit that adds this
file — this document is the last thing written, so the tree it describes is the one immediately
before it.

Every version above was read from the tools themselves; the unedited capture is in
[`unit_tests/00-environment/evidence_environment.md`](unit_tests/00-environment/evidence_environment.md).

### The curated corpus

This is the acceptance criterion itself.

| | |
| --- | --- |
| Programs compared | 64 |
| Agreed with `clang` on stdout, stderr, and exit status | 64 |
| Known mismatches | 0 |
| Wall clock | 26 seconds, one test per program, run in parallel |

`cargo test --test differential`. Every program in `tests/programs/` is built by both compilers,
both binaries are run, and the three axes are compared. Which language features those sixty-four
programs reach — every one in the grammar, in at least three programs each, and every pair that has
to agree about something in at least one — is recorded in
[`tests/programs/COVERAGE.md`](https://github.com/sid-ak/rusty_c_compiler/blob/main/tests/programs/COVERAGE.md).

Beside it, `cargo test --test codegen_exec` holds the same sixty-four programs to the exit code and
stdout recorded in their own headers, which were taken from `clang` rather than from this compiler.
Sixty-four of sixty-four.

And `cargo test --test invalid_programs` holds thirty-one programs to being rejected, each for the
rule it names, with what `clang` makes of each one recorded alongside.

### The generated corpus

This is what the curated corpus cannot do: cover the combinations nobody thought to write down.

| | |
| --- | --- |
| Programs generated and compared | 2,500 |
| Agreed with `clang` on stdout, stderr, and exit status | 2,500 |
| Known mismatches | 0 |
| Undefined behavior found by `clang -fsanitize=undefined` | 0, over the 12 sampled |
| Starting seed | 987654321 |
| Wall clock | 1,308 seconds |

`RUSTYCC_GENERATED_PROGRAMS=2500 RUSTYCC_GENERATED_SEED=987654321 cargo test --test generated`. A
plain `cargo test` runs forty of them, which is what a per-push check can afford; the nightly
workflow runs two thousand.

The first attempt at this run was not clean, and the reason is worth recording. Three of the 2,500
ran past the harness's ten-second limit — two under both compilers, and one that finished under
`clang` and not under `rustycc`. None was a wrong answer. All three were the generator writing a
loop inside a loop inside a function called from a loop, where a call that is one statement to read
is a hundred thousand statements to run. The generator now estimates the work it has asked for the
same way it estimates the values an expression can take, and stops offering loops and calls past a
budget. The run above is after that change.

### Fuzzing

Fifteen minutes per target, which is the documented minimum, from a seed corpus built out of
`tests/programs/`, `tests/programs/invalid/`, and `tests/adversarial/`.

| Target | Runs | Duration | Crashes | Timeouts | Inputs added to the corpus |
| --- | --- | --- | --- | --- | --- |
| `lex` | 1,118,857 | 901 s | 0 | 0 | 7,945 |
| `parse` | 573,429 | 901 s | 0 | 0 | 6,919 |
| `frontend` | 492,883 | 901 s | 0 | 0 | 7,020 |

Two million one hundred and eighty-five thousand runs, no crash and no hang. `fuzz/artifacts/` is
empty, and so is `fuzz/regressions/` — there is nothing to minimize and check in.

This is a statement about what has been run, not a proof that no crashing input exists. What makes
it more than nothing is the seed corpus: the fuzzer did not start from random bytes, it started from
every program in the repository and mutated outward, which is where an input that is nearly C but
not quite comes from.

### The rest of the suite

| | |
| --- | --- |
| Tests | 510 |
| Passed | 510 |
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | clean |

The unedited capture is in [`unit_tests/evidence_all.md`](unit_tests/evidence_all.md),
and what each of those tests pins is in [the unit test reports](unit_tests/index.md).


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
