# ADR 0006 — A fixed-arity runtime shim instead of printf

- Status: Accepted
- Date: 2026-08-10

## Context

A compiled program needs observable output or differential testing has nothing to compare beyond an
exit status, and an 8-bit exit status is a very narrow channel through which to test a compiler.

Excluding the preprocessor does not exclude the standard library: C permits declaring an external
function directly in source, so `extern int printf(const char *, ...);` with no `#include` is legal
and would link. The obstacle is not the declaration but the call. `printf` is variadic, and variadic
argument passing under AAPCS64 has its own rules distinct from the ordinary convention — arguments
go through a different path on Apple platforms specifically. Implementing that correctly is real
work that teaches ABI conformance twice over rather than once.

There is a second, subtler obstacle. `printf` is buffered. If one binary flushes at a different
point than the other, the comparison sees a difference that has nothing to do with either compiler.

## Decision

Compiled programs get three fixed-arity runtime functions, implemented in
[`runtime/shim.c`](https://github.com/sid-ak/rusty_c_compiler/blob/main/runtime/shim.c), compiled
once by `clang` into an object file, and linked into every test binary:

```c
void print_int(int n);
void print_char(char c);
void print_string(char *s);
```

They are built on `write(2)` with no `stdio` dependency. `print_string` is a loop over `print_char`
to the null terminator. Both `rustycc` and `clang` link the same object when building the same test
program.

## Consequences

Variadic argument passing stays out of scope entirely, and the ordinary AAPCS64 path is the only
calling convention the code generator implements.

Output is unbuffered and written in the order the program produces it, so buffering can never be the
cause of a differential mismatch. Any difference in stdout is a difference between the compilers.

Linking the identical object on both sides is what makes the comparison apples to apples. There is
no second runtime implementation that could itself be wrong.

`print_string` takes a `char *` rather than a `char`, which is why string literals and
array-to-pointer decay had to enter the subset at all — the signature is what a string literal
actually decays to, so passing one is well-typed rather than a special case. See
[ADR 0007](0007-array-decay-only-at-parameter-boundary.md).

The cost is that programs cannot format output. Printing a labelled value takes two calls rather
than one format string. For test programs this is a non-issue and arguably an improvement, since
there is no format-string behavior to get wrong on either side.

## Alternatives considered

Declare and call `printf` directly. Rejected on variadic AAPCS64 complexity and on buffering
non-determinism.

Implement the shim in the subset itself, compiled by `rustycc`. Appealing, and it would remove the C
dependency. Rejected because it needs a syscall mechanism the subset has no way to express, and
because a runtime compiled by the compiler under test cannot serve as neutral ground for comparing
that compiler against another.

Compare exit status only, with no output at all. Rejected: eight bits per program is far too narrow
to catch the arithmetic, string, and array bugs the corpus is meant to surface.
