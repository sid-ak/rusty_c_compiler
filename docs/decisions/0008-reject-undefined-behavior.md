# ADR 0008 — Reject undefined behavior rather than admit it

- Status: Accepted
- Date: 2026-08-10

## Context

C leaves a number of situations undefined: falling off the end of a non-`void` function, signed
integer overflow, division by zero, indexing outside an array, reading an uninitialized variable. A
conforming compiler may do anything at all in those cases, and two conforming compilers may do
different things.

This project's correctness criterion is that `rustycc` and `clang -O0` agree — see
[ADR 0001](0001-subset-of-c-with-clang-as-oracle.md). A program with undefined behavior breaks that
criterion in both directions. If the two compilers disagree, that proves nothing, because both are
free to do anything. If they agree, that also proves nothing, because agreement was never required.
Either way the test carries no information, while looking exactly like a test that does.

## Decision

Undefined behavior is kept out of the system in two ways.

Where it is statically detectable, the analyzer rejects it. A non-`void` function whose control flow
can reach its closing brace is a semantic error, with `main` excepted since C defines an implicit
`return 0` there. This makes the subset deliberately stricter than C.

Where it is not statically detectable — division by zero, signed overflow, out-of-bounds indexing,
uninitialized reads — it is excluded by construction from the test corpus. The random program
generator guards divisors against zero, bounds arithmetic so overflow cannot occur, clamps
subscripts, and initializes before use. Hand-written corpus programs are held to the same rule.

## Consequences

Every program in the corpus has exactly one correct behavior, so a differential mismatch is
unambiguous evidence of a compiler bug. This is what makes the acceptance criterion mean something.

The random generator becomes possible at all. A generator that emitted arbitrary well-typed C would
produce mostly undefined programs, and its failures would be unclassifiable. Restricting it to
defined behavior is what turns it from a curiosity into the thing that finds real codegen bugs.

The generated corpus can be validated independently: running it under
`clang -O0 -fsanitize=undefined` must produce no reports. If it does, the generator's guards are
wrong, and that is caught before any mismatch is blamed on the compiler.

The cost is that `rustycc` rejects some valid C. A program relying on falling off the end of a function
compiles under `clang` and not here. This is recorded as an intentional deviation in the
invalid-program corpus, where each rejected program either is also rejected by `clang` or carries a
written reason for the divergence, so the stricter-than-C surface stays deliberate and small rather
than growing by accident.

Runtime undefined behavior is excluded by convention rather than enforcement. Nothing stops someone
adding a corpus program that divides by zero; the guard is the rule and the review, not the
compiler.

## Alternatives considered

Admit undefined behavior and compare anyway. Rejected: the tests would look like tests and prove
nothing, which is worse than having no test.

Define the behavior ourselves — for example, specify that falling off the end returns 0. Tempting,
and it would let those programs into the corpus. Rejected because `clang` would not be bound by that
definition, so every such program would become a permanent expected mismatch, and permanent expected
mismatches are how a suite stops being trusted.

Detect runtime undefined behavior by emitting checks. Rejected: it changes the semantics of the
generated code relative to `clang -O0`, which is the thing being compared against.
