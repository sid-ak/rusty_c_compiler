# The grammar, in full

This is Extended Backus-Naur Form (EBNF). `=` defines a rule; `|` is "or"; `[ ... ]` is optional
(zero or one); `{ ... }` repeats (zero or more); `"..."` is a literal token the parser expects to
see verbatim.

```ebnf
--8<-- "grammar/syntax.ebnf"
```

The grammar lives in its own file,
[`grammar/syntax.ebnf`](https://github.com/sid-ak/rusty_c_compiler/blob/main/grammar/syntax.ebnf),
so it can be read, diffed, and referenced on its own terms.

The bottom half — from `expr` down to `primary` — is deliberately repetitive. It is not ten rules
because ten rules were needed; it is ten rules because operator precedence is encoded as nesting
depth. `assignment` sits at the top (lowest precedence, binds loosest) and `primary` sits at the
bottom (highest precedence, binds tightest), with every level in between matching C's precedence
table exactly. When the parser reads `1 + 2 * 3`, walking this grammar top-down forces `2 * 3` to
group together before `1 +` touches it, purely because `multiplicative` is nested inside `additive`.
No precedence table or ad-hoc sorting logic is needed anywhere in the code — the parser functions
mirror this grammar one-to-one, so the shape of the grammar _is_ the shape of the parser.

## Semantics that cross pass boundaries

These are the rules more than one pass has to agree on, so they are pinned in one place rather than
left implicit in each pass's own code:

- `char` promotes to `int` in every arithmetic, comparison, and logical context. Storage is one
  byte; computation is 32-bit.
- Arrays decay to a pointer to their first element only when passed as a function argument. There is
  no other pointer-producing expression in the subset.
- A parameter written `int a[]` has pointer type inside the function body, and indexing it is the
  only legal use.
- String literals have type pointer-to-`char`, are null-terminated, and are legal only as an
  argument, as the initializer of a `char` array, or parenthesized within those.
- `&&` and `||` short-circuit and yield `0` or `1`.
- Falling off the end of `main` returns `0`. Falling off the end of any other non-`void` function is
  a semantic error (see [semantic analysis](#semantic-analysis)).
- Process exit status is the low 8 bits of `main`'s return value; test expectations are written
  against the masked value.

## Out of scope

Rejected with a specific diagnostic rather than mis-parsed: the preprocessor,
`struct`/`union`/`enum`/`typedef`, `switch`, `do`/`while`, the ternary operator, compound assignment
(`+=` and friends), bitwise and shift operators, `float`/`double`/`long`/`unsigned`,
multi-dimensional arrays, declared pointer variables, `&` address-of, `*` dereference, variadic
functions, and `sizeof`. This is the complete boundary of the subset — nothing outside it is
silently accepted, and nothing inside it is silently rejected.

## Two small additions with no ADR of their own

Most of what is and is not in the subset traces to a specific ADR. Two small resolutions did not
warrant one:

`void` was added to the type list. The original project proposal names only `int` and `char`, but
the [runtime shim](#the-runtime-shim)'s functions return `void`, so it became unavoidable. Its scope
is kept deliberately narrow: return types and the `(void)` parameter list, never a variable or
parameter type.

`++`/`--` and `break`/`continue` were included. The proposal names only operator categories and four
statement forms, but idiomatic `for` loops and loop-heavy test programs need these — they cost
little in any pass, and leaving them out would make the test corpus read like nobody's real C. The
alternative was writing `i = i + 1` everywhere.
