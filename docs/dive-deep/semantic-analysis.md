# Semantic Analysis

Analysis runs as two sub-passes over the AST. Pass A walks the top level and records every global
variable and every function signature. Pass B then walks each function body with that table already
populated — which is what lets a function call another function defined later in the same file, the
ordinary C idiom, and something a single pass cannot do without a forward-reference hack.

## Resolution uses a scope stack

Globals sit at depth 0; a new scope is pushed for each function body, each nested block, and each
`for` init clause. Lookup walks outward from the innermost scope. Shadowing an outer name is legal;
redeclaring within the same scope is an error that names both the new and the original declaration
site. Scoping the `for` init clause separately is what makes `for (int i = 0; ...)` leave no `i`
behind after the loop ends.

## The annotations matter more than the errors

The analyzer's job is to answer every type question in the program exactly once, so the code
generator never has to ask one:

- A resolved type for every expression node.
- A resolved binding for every identifier: global symbol, parameter index, or local slot id.
- A per-function frame inventory — every local and parameter with its size and alignment, in
  deterministic order — which the code generator turns directly into stack offsets.
- String pooling: an interned string-literal table mapping each distinct literal's decoded bytes to
  one label, so two occurrences of `"hello"` share one entry in the read-only data section.
- Implicit conversions made explicit: a `char` widened to `int` becomes an actual cast node in the
  annotation, and an array decaying to a pointer at a call site becomes an actual decay node.

That last point is load-bearing for the backend: because promotions and decays are materialized here,
the code generator's expression lowering is a direct structural walk with no type inference of its
own. Every place the backend would otherwise have to reason "is this a `char`, and does it need
widening before this comparison?" has already been answered before code generation starts.

## Statically detectable undefined behavior is rejected here

A non-`void` function whose control flow can reach its closing brace is a semantic error, with `main`
excepted (C defines an implicit `return 0` there). The same class of runtime condition — division by
zero, signed overflow, out-of-bounds indexing, uninitialized reads — is excluded from the
generated-program corpus rather than statically checked, since none of those are decidable at compile
time in general. The reasoning for excluding undefined behavior from the system at all is [ADR
0008](decisions/0008-reject-undefined-behavior.md).
