# Parser

The implementation is recursive descent: one function per grammar production, calling each other in
the shape of the grammar itself. Declarations and statements are parsed by straightforward
descent — `if_statement`, `while_statement`, `block` — because their structure is driven by leading
keywords, and the three top-level forms share a `type ident` prefix that is read once before
anything looks at what follows it, so no rule needs to see further than the next token.

## Expressions, handled by precedence climbing

The grammar in [the language subset](#the-language-subset) writes the binary operators as a chain of
nested rules, `logical_or` down to `multiplicative`, because nesting depth is how a grammar spells
precedence. Writing one function per level is the textbook way to mirror that, and it produces six
bodies that differ only in which operators they match and which function they call next — six places
to keep in step every time the precedence table moves.

The parser instead uses precedence climbing: one function, one table mapping a token to its operator
and its binding strength, and a recursive call whose minimum precedence stands in for descending a
level. `1 + 2 * 3` groups as `1 + (2 * 3)` for exactly the reason the grammar says it does — `*`
outranks `+`, so the recursive call for the right-hand operand takes it and the loop does not. The
correspondence to the grammar is the numbers in the table: they are those nested rules counted from
the outside in.

Everything else keeps the grammar's shape. Assignment and the prefix operators are
right-associative; the binary levels are left-associative, because the result so far becomes the
left operand of the next operator; and postfix `[]`, `()`, `++`, and `--` are a loop after the
primary expression rather than rules of their own, so they chain in any combination — `a[i]++` and
`f(x)[0]` both fall out of that loop with no case for either.

Parentheses leave no node behind. They exist to group, and once the tree records the grouping there
is nothing left for them to say, so `((((1))))` is the same tree as `1` — and `(a) = 1` is assignable
for the same reason it is in C.

## Error recovery is panic-mode

On a syntax error the parser records a diagnostic and abandons the construct it was in. The nearest
enclosing loop — over the items of a file, or the statements of a block — catches that, skips to the
next `;` or `}`, and carries on, so a file with four mistakes reports four of them.

Two rules make that safe. Every recovery step consumes at least one token: the loop remembers where
the failed construct started, and if recovery skipped nothing at all it consumes one token itself,
so a construct that fails on its very first token still moves the parser forward. And brace depth is
counted while skipping, so a `{ ... }` inside the wreckage is stepped over as a unit, while a `}`
that closes the block the parser is actually inside is left where it is — swallowing that one is how
a single missing brace turns into the rest of the file disappearing.

## Constructs outside the subset get their own diagnostic

Someone writing real C that this compiler happens not to implement should be told that, not told
their program is malformed. Every such construct is reported as "unsupported in this C subset: X",
and which pass notices it is an accident of how it is spelled:

- A word. `struct`, `switch`, `sizeof`, `float` and the rest are ordinary identifiers to the lexer,
  so the parser holds the list — it is C89's keyword set minus this subset's ten — and consults it
  in the single place a token is turned down. Every rejection anywhere in the parser therefore says
  "unsupported" instead of "expected a type" when that is the truer answer.
- A shape. A `*` between a type and the name it declares is a pointer declarator, which is caught
  where declarations read their `type ident` prefix.
- Two tokens that together spell one operator. `+=` and `<<` are not tokens, because the subset has
  neither, so the lexer hands over their halves. The expression parser recognizes the adjacent pair,
  which is what makes `a += b` one diagnostic naming compound assignment rather than a complaint
  about an unexpected `=`.
- Punctuation with no token at all. `#`, `?`, `:`, `^`, `~`, and a lone `&` or `|` never reach the
  parser, so [the lexer](#the-lexer) reports them.

## A nesting-depth limit

Recursive descent recurses, so deeply nested parentheses would otherwise run the call stack out —
which is a crash, and crashes are the one thing the front end is not allowed to produce. So the
parser counts how deep the tree it is building has got, and past 128 levels reports instead of
going deeper.

What it counts is levels of the tree, not the parser's own calls and not brackets. The difference
matters because the parser is not the only thing that walks the tree: printing it, freeing it, and
later type checking and code generation all recurse once per level. Most shapes deepen the tree and
the parser's call stack together — parentheses, blocks, `!!!x`, `a = b = c` — and pass through one
function that counts the level. Two do not. `1 + 1 + 1` and `a[0][0][0]` are built by a loop, one
level deeper per operator with the parser's own depth staying flat, so each pass of those loops
charges its level explicitly. Without that, a 10,000-term sum parsed cleanly and then crashed the
moment anything walked the result.

The number is a measured stack budget rather than a taste in style. In an unoptimized build the
costliest shape, nested blocks, needs about 550 KB of stack to parse, print, and free at the limit —
inside the 2 MiB the test harness gives a thread and far inside the 8 MiB the binary's main thread
has. A test builds every deep shape tens of thousands of levels deep and parses, prints, and frees
each on a deliberately undersized 1 MiB stack, so a change that fattens a frame, or a new shape that
escapes the count, fails there instead of turning back into the crash the limit exists to prevent.
Real C reaches nothing close to it either way.

This is planned for rather than discovered, because the Phase 5 fuzzer finds an unbounded stack
within seconds of running. Until then the same property is checked the cheap way, by cutting every
program in the corpus short at every byte and parsing each fragment — a prefix of a valid program is
exactly the shape of input a parser mishandles, and there are thousands of them for free.
