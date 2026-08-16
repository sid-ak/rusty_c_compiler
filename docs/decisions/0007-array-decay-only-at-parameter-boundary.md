# ADR 0007 — Array-to-pointer decay only at the function-parameter boundary

- Status: Accepted
- Date: 2026-08-10

## Context

The proposal puts pointers out of scope beyond simple array indexing, and puts one-dimensional
arrays in scope. Those two statements conflict at exactly one place: C has no way to pass an array
to a function. An array argument decays to a pointer to its first element, and the parameter is a
pointer, not an array.

Without resolving this, arrays are confined to the function that declares them. No program can sum
an array in a helper, sort one, or fill one — which removes most of the realistic programs the test
corpus is supposed to contain. The runtime shim's `print_string(char *)` has the same shape and the
same problem, since a string literal is an array of `char` that decays when passed.

Resolving it the other way — admitting pointers generally — means pointer variables, address-of,
dereference, pointer arithmetic, and pointer types in the analyzer and the backend. That is a large
expansion of the semantics for a subset that is otherwise deliberately small.

## Decision

Array-to-pointer decay is supported at exactly one place: the function-argument position. An array
passed as an argument is passed as a pointer to its first element, matching real C. A parameter
written `int a[]` has pointer type inside the function body, and indexing it is the only legal use.

Everywhere else, pointers remain out of scope. There are no pointer variable declarations, no
address-of `&`, no dereference `*`, and no pointer arithmetic. An array used in arithmetic —
`a + 1` — is a semantic error rather than pointer arithmetic.

The type model still carries a `Ptr` type, because parameters and string literals genuinely have
pointer type; the restriction is on what expressions can produce one, not on what the analyzer can
represent.

## Consequences

Arrays become usable in the way real programs use them: passed to helpers, mutated in place by a
callee and observed by the caller. The test corpus can contain sum, reverse, and sort over an array
across a function boundary, which is where interesting codegen bugs live.

String literals work with no special case at the use site. A literal has type `char *`, which is
what `print_string` takes, so it is an ordinary well-typed argument rather than a hole punched in
the type system. See [ADR 0006](0006-fixed-arity-runtime-shim.md).

Decay is materialized by semantic analysis as an explicit node rather than inferred in the backend,
so the code generator emits an address at argument positions because it was told to, not because it
worked out that it should. See
[ADR 0004](0004-immutable-ast-with-side-table-annotations.md).

The subset is now stricter than C in a visible way: `int *p = a;` is rejected. This is a documented
restriction rather than a bug, and it is one of the cases the invalid-program corpus records
explicitly as an intentional deviation.

## Alternatives considered

Exclude arrays from function boundaries entirely. Keeps the promise about pointers literally, and
makes arrays nearly useless. Rejected: it would gut the corpus.

Pass arrays by value, copying them. Not C. Rejected: it would make `rustycc` and `clang` disagree on
any program that mutates an array argument, which is precisely the differential mismatch the project
exists to detect.

Support pointers fully. The honest general solution, and the natural later extension. Rejected for
this scope on the size of the resulting semantics — pointer arithmetic, aliasing, and null-ness all
arrive together.
