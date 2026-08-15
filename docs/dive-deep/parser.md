# Parser

The implementation is recursive descent: one function per grammar production, calling each other in
the shape of the grammar itself. Declarations and statements are parsed by straightforward
descent — `parse_if`, `parse_while`, `parse_block` — because their structure is driven by leading
keywords.

## Expressions, handled separately by precedence climbing

Writing one function per precedence level (`parse_additive` calls `parse_multiplicative`, which calls
`parse_unary`, and so on) is the textbook approach, and the grammar in [the language
subset](#the-language-subset) is written in exactly that layered shape so the code and the grammar can
be read against each other. Assignment and prefix unary operators are right-associative; the binary
levels are left-associative; postfix `[]`, `()`, `++`, and `--` are handled by a loop after the
primary expression so they chain in any combination — `a[i]++` and `f(x)[0]` both fall out of the
same loop with no special-casing.

## Error recovery is panic-mode

On a syntax error, the parser records a diagnostic, then skips tokens until the next `;` or `}` at
the current brace depth and resumes. The rule that makes this safe is that every recovery step
provably consumes at least one token, so recovery cannot spin in place. Brace depth is tracked so a
missing `}` does not silently swallow the rest of the file into one giant recovery skip.

## Constructs outside the subset get their own diagnostic

Rather than a generic parse error, seeing `struct`, `switch`, `#include`, a `*` declarator, `?:`, or
`+=` produces "unsupported in this C subset: X". A user who writes valid C that this compiler simply
does not implement should be told that, not told their program is malformed.

## A nesting-depth limit

Recursive descent recurses, so deeply nested parentheses would otherwise overflow the call stack —
which is a crash, and crashes are the one thing the front end is not allowed to produce. The limit
turns that into a diagnostic instead. This is planned for rather than discovered, because the Phase 5
fuzzer finds an unbounded stack within seconds of running.
