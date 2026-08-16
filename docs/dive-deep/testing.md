# Testing Strategy

## Test tiers

Each tier catches what the tier below it cannot:

- Unit test: an in-crate `#[cfg(test)]` module exercising one component in isolation. Where most of
  the lexer's and analyzer's coverage lives.
- Snapshot test: a rendered structure — token stream, AST dump, assembly text, diagnostic rendering —
  compared against a checked-in expected file, via `insta`. Snapshots are how parser tests assert AST
  shape, which is the only reliable way to test operator precedence: asserting that `1+2*3` evaluates
  to `7` would also pass if precedence were wrong for a compensating reason.
- Golden-program test: a `.c` file in `tests/programs/` with a recorded expected exit code and
  stdout. Used while the compiler is the only thing that can run the corpus, before code generation
  is far enough along to differentially test.
- Differential test: the same `.c` file compiled by both `rustycc` and `clang -O0`, both run, outputs and
  exit status compared. Replaces the recorded expectations of golden tests once an oracle is
  available.

## Differential testing against clang

Every program in the corpus is built twice — once by `rustycc`, once by `clang -O0 -std=c99 -Wall` —
with both linked against the same `shim.o`. Both binaries run with identical argv, empty stdin, and a
wall-clock timeout. The harness compares stdout byte for byte, compares stderr, and compares exit
status masked to the low 8 bits, distinguishing death by signal from normal exit. Each program is its
own `#[test]`, so a failure names the offending program rather than collapsing the whole suite into
one red line, and a mismatch report includes both outputs plus the path to the retained `.s` so it can
be diagnosed without re-running anything by hand.

The harness itself is test code, and test code that cannot fail proves nothing — so it has self-tests
that inject a deliberately wrong compiler output on each comparison axis and assert the harness
reports a mismatch.

Hand-written programs only cover the combinations someone thought to write down. A seeded generator
of random well-typed subset-C, restricted to defined behavior (see [semantic
analysis](#semantic-analysis)), feeds the same harness and covers the combinations nobody thought
of. A failing seed's program is minimized and checked in as a permanent fixture, so a bug the fuzzer
found once can never silently regress.

## Fuzzing

`cargo-fuzz` targets run raw bytes through the lexer, the parser, and the whole front end including
semantic analysis. The property under test is the no-panic invariant (see [pipeline](#the-pipeline)):
arbitrary input must produce diagnostics or success, never a crash and never a hang. Crashes found are
minimized and checked in as ordinary unit tests, so they are guarded by the normal `cargo test` run
rather than only by re-fuzzing. `cargo-fuzz` needs a nightly toolchain; nightly is pinned for that job
alone while the compiler itself builds on stable.

## Acceptance

The system-level acceptance criterion is a single run: every program in the curated suite, covering
every supported language feature, producing identical behavior under `rustycc` and under `clang -O0`.
The project is functionally complete when that run is green with zero known mismatches, and not
before.
