# Lexer

The scanner reads `&[u8]` rather than `&str`. Source files are not guaranteed to be valid UTF-8, and
the lexer is a fuzz target that will be handed arbitrary bytes; operating on bytes means malformed
input produces a diagnostic rather than a decoding panic before the lexer even starts. Multi-character
operators are matched by maximal munch, so `<=` never lexes as `<` then `=`, and keyword recognition
runs after identifier scanning, so `integer` is one identifier rather than `int` followed by `eger`.

## Literals are decoded at lex time, not later

A string or character literal's escape sequences
(`\n`, `\t`, `\\`, and so on) are resolved into actual bytes and stored on the token, so the code
generator emits the stored bytes rather than re-parsing `\n` out of the original source text. Doing
this once, in the one place that already has the source in hand, removes a whole class of
"escapes handled inconsistently in two places" bugs.

## Every token carries a `Span`

A `Span` is a byte range into the source. Spans, not line and column numbers,
are what flows through the rest of the compiler; a `SourceMap` converts an offset to a line and
column only when a diagnostic is actually rendered, using a precomputed table of line-start offsets
so the conversion is a binary search rather than a rescan of the file.

## Error recovery: resynchronize, don't stop

On a malformed construct the lexer records a diagnostic and resynchronizes at a defined point — end
of line for an unterminated literal, end of file for an unterminated block comment — then keeps
scanning to `Eof`. A lexer that stops at the first error would make the parser's own error recovery
impossible to test, since the parser would never see tokens past the first lexical mistake.

## Punctuation the subset leaves out is named here, not passed on

`&`, `|`, `#`, `?`, `:`, `^`, and `~` are all real C, and none of them is a token of this grammar.
There is nothing for the lexer to hand the parser, so the parser could never report them — which
makes the lexer the only place the judgement can be made. Each is reported as "unsupported in this
C subset", the same phrasing [the parser](#the-parser) uses for the constructs it catches, with a
note saying what to reach for instead. A byte that is not C at all, such as `@`, is still a stray
character; the distinction is between "this is C we do not implement" and "this is not C".
