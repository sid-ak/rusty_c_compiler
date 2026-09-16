# Testing Strategy

## Test tiers

Each tier catches what the tier below it cannot:

- Unit test: an in-crate `#[cfg(test)]` module exercising one component in isolation, kept in that
  module's own `tests.rs` file (`src/lexer/tests.rs` for the lexer) so the source file holds only
  the code. Where most of the lexer's and analyzer's coverage lives.
- Snapshot test: a rendered structure — token stream, AST dump, assembly text, diagnostic rendering —
  compared against a checked-in expected file, via `insta`. Snapshots are how parser tests assert AST
  shape, which is the only reliable way to test operator precedence: asserting that `1+2*3` evaluates
  to `7` would also pass if precedence were wrong for a compensating reason.
- Golden-program test: a `.c` file in `tests/programs/` with a recorded expected exit code and
  stdout in its own header. The values were recorded from `clang`, so the header is an independent
  answer rather than a note of what this compiler happened to do — and it is the faster of the two
  program tiers to read when something breaks, because it says out loud what the program is supposed
  to print.
- Differential test: the same `.c` file compiled by both `rustycc` and `clang -O0`, both run, outputs
  and exit status compared. This is the tier the acceptance criterion is stated in terms of: a
  recorded expectation says a program still does what it did, and a live oracle says it does what C
  says it should.
- Generated corpus: the differential comparison again, over programs a seeded generator wrote. The
  hand-written corpus covers the combinations someone thought to write down; this covers the ones
  nobody did.

## Differential testing against clang

Every program in the corpus is built twice — once by `rustycc`, once by `clang -O0 -std=c99 -Wall` —
with both linked against the same `shim.o`. Both binaries run with identical argv, empty stdin, and a
wall-clock timeout. The harness compares stdout byte for byte, compares stderr, and compares exit
status masked to the low 8 bits, distinguishing death by signal from normal exit. Each program is its
own `#[test]`, so a failure names the offending program rather than collapsing the whole suite into
one red line, and a mismatch report includes both outputs plus the path to the retained `.s` so it can
be diagnosed without re-running anything by hand.

Three outcomes are kept apart, because folding them together hides which one happened. Two binaries
that behaved differently is a mismatch. One compiler that would not build the program at all is a
hole in the subset if it was this one, and a broken corpus entry if it was `clang`. A program that
never finished is a timeout, reported as such before its output is compared — a killed program's
output is however much escaped before the signal, so two programs that both ran forever will differ
there too, and saying "stdout differs" sends whoever reads it looking for a wrong answer that was
never produced.

The harness itself is test code, and test code that cannot fail proves nothing — so the comparison
is a pure function over two records of a run, with no compiler and no process behind it, and its
self-tests hand it a deliberately wrong answer on each axis and assert it says so. A trailing
newline on its own counts: a harness that trimmed before comparing would pass that test and then
miss a missing newline in every program in the corpus.

## The generated corpus

A seeded generator of random well-typed subset-C feeds the same harness. The hard part is not
producing C, it is producing C with a right answer: `clang` is an oracle only for programs whose
behavior the standard defines, so the generator rules out undefined behavior by construction rather
than by filtering afterwards.

Every expression is built together with the interval of values it can take, computed in 64-bit
arithmetic, and an operator is emitted only if its result interval still fits in an `int`. A divisor
is always a positive literal, which rules out both division by zero and `INT_MIN / -1`. Every
variable carries the invariant that it holds a value within a fixed bound, and an assignment whose
interval does not fit is wrapped in a remainder by a positive literal. Subscripts are literals inside
the array or a `for` counter whose whole range is known to be. Nothing is read before it is written,
and no generated expression assigns to anything, so there is nothing to sequence. `clang`'s own
undefined-behavior sanitizer runs over a sample of the programs, which is the check on that reasoning
being right rather than only careful.

The same reasoning is applied to how long a program runs. Nothing else bounds a loop inside a loop
inside a function called from a loop, and a generated program that does not finish has no output to
compare — so each statement is charged the product of the loop bounds around it, each call is
charged whatever the callee was estimated at, and past a budget the generator stops offering loops
and calls.

A seed reproduces its program byte for byte, so a failure is a number rather than a story about a
run that already finished. A failing seed's program is minimized and checked in as a permanent
fixture, so a bug found once can never silently regress.

## Fuzzing

`cargo-fuzz` targets run raw bytes through the lexer, the parser, and the whole front end including
semantic analysis. The property under test is the no-panic invariant (see [pipeline](#the-pipeline)):
arbitrary input must produce diagnostics or success, never a crash and never a hang. Each target
asserts a little more than "it did not crash" — the token stream always ends in `Eof` and every span
points inside the input, a tree always dumps, a program is always either accepted or reported on —
because a target that only checks for panics is blind to a pass that returns something nonsensical
without crashing.

Semantic analysis gets a target of its own rather than being assumed safe because the two passes in
front of it are. It has its own recursion, its own indexing, and a side table keyed by node id, and
it is handed trees the parser built while recovering from an error — the one shape it never sees in
ordinary use.

Crashes found are minimized and checked into `fuzz/regressions/`, where the ordinary `cargo test`
run picks them up, rather than living only in a fuzz corpus that is machine-specific, regenerated,
and never consulted before a merge. `scripts/fuzz.sh` runs one target for a fixed time, building the
seed corpus out of `tests/programs/`, `tests/programs/invalid/`, `tests/adversarial/`, and the
regressions — so every run starts from everything already written down, and no second copy of those
files has to be kept in step. `cargo-fuzz` needs a nightly toolchain; nightly is pinned for that job
alone while the compiler itself builds on stable.

## Acceptance

The system-level acceptance criterion is a single run: every program in the curated suite, covering
every supported language feature, producing identical behavior under `rustycc` and under `clang -O0`.
The project is functionally complete when that run is green with zero known mismatches, and not
before.
