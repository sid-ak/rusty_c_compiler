# ADR 0001 — Compile a subset of C, with clang as the testing oracle

- Status: Accepted
- Date: 2026-08-10

## Context

The project needs a source language. The obvious choice for a first compiler is a small
purpose-built language: the grammar can be kept tiny, the semantics can be defined to be whatever is
convenient, and nothing external constrains the design.

That convenience is also the problem. A purpose-built language has no independent notion of correct.
The only specification is the author's intent, and the only test oracle is the author's expectation
of what a program should print — which is precisely the thing most likely to be wrong. Every test
would be a hand-recorded expected value, and a systematic misunderstanding of, say, operator
precedence or integer promotion would be recorded as the expectation and never caught.

## Decision

The source language is a subset of C, defined by the grammar in
[architecture.md](../architecture.md#the-language-subset). `clang -O0` is the testing oracle: the
same program is compiled by both compilers, both binaries are run, and their stdout, stderr, and
exit status are compared.

The subset is chosen to be small enough to implement in the available time — `int` and `char`,
one-dimensional arrays, the four statement forms, functions and recursion, arithmetic, comparison
and logical operators — while staying genuinely C, so that every program in the test corpus is a
valid C program that `clang` will accept unchanged.

## Consequences

Correctness becomes externally verifiable. A differential mismatch is unambiguous evidence of a bug,
and it points at a specific program. This is what makes the project's acceptance criterion
meaningful rather than self-referential.

Test authoring gets cheaper, not more expensive. A test program does not need a recorded expected
output at all; the oracle produces it. This is what makes a randomly generated program corpus
possible, which is the only realistic way to cover the combinations nobody thought to write by hand.

C's inconvenient details must be respected exactly: precedence and associativity, `char` promoting
to `int`, array-to-pointer decay, the low-8-bits rule on exit status. Getting any of them
approximately right shows up immediately as a mismatch. This is a cost during implementation and the
entire point during testing.

Undefined behavior becomes unusable as test material, since both compilers are free to do anything
and a mismatch would prove nothing. See [ADR 0008](0008-reject-undefined-behavior.md).

## Alternatives considered

A purpose-built toy language. Rejected: no external oracle, so testing degrades to asserting the
author's own assumptions.

A subset of an existing language with a reference interpreter, such as a Lisp or a stack language.
Workable, but the interpreter would be the oracle for semantics only, not for the native code
generation and ABI conformance that make up most of the interesting risk here.

Full C. Not credible in the timeframe; the preprocessor alone is a separate project, and structs,
unions, and the full pointer model would multiply the semantics without teaching anything the subset
does not already cover.
