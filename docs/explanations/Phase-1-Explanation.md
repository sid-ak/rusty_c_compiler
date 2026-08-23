# Phase 1: Foundation, diagnostics, and the lexer

A walkthrough of what Phase 1 built and why each choice was made, written for a reader with no
background in Rust and none in compilers. Terms are defined the first time they appear and then used
normally. The [architecture](../architecture.md) says what the finished system is; the
[plan](../PLAN.md) says what each phase is meant to deliver; this says what actually got built and
what the reasoning was.

## The problem this phase solves

A compiler turns text a person wrote into instructions a machine runs. Before any of that can start,
two much duller questions have to be answered, and answering them badly poisons everything
downstream.

The first: how does the compiler read text at all? Not "open the file" — that part is easy — but how
does it get from a wall of characters to something with structure? The answer is a stage called the
lexer, and it is Phase 1's centrepiece.

The second: when something is wrong, how does the compiler say so? Every later stage needs to report
problems, and if each one invents its own way of doing it, the user gets four different styles of
error message from one program. So the error-reporting machinery gets built once, first, before
anything has errors to report.

Phase 1 also settles a question that shapes the entire project, which is worth starting with because
everything else follows from it.

## Deciding what "correct" means

Most projects can dodge this. A compiler cannot. If you write a compiler and it produces a program,
how do you know the program is right?

The usual answer is: write test programs, work out by hand what each should print, and check. That
works, but it has a hole in it — you are checking the compiler against your own understanding of
what the code should do, and your understanding is exactly what might be wrong. If you
misremembered how C's `%` behaves on negative numbers, you will write the wrong expected answer and
the test will pass.

This project takes a different route, and it is the reason the input language is a subset of real C
rather than a small language invented for the occasion. Because the input is genuinely C, `clang` —
a mature, heavily used, independently written C compiler — can compile the exact same file. Compile
a program with both, run both, compare what happens. Agreement is evidence that does not depend on
trusting anyone's assumptions, because nobody wrote down the expected answer. `clang` produced it,
just by being run.

That technique is called differential testing, and it is this project's actual definition of
correct. It is recorded as
[ADR 0001](../decisions/0001-subset-of-c-with-clang-as-oracle.md). An ADR — Architectural Decision
Record — is a short document capturing one decision, what was rejected, and why; the project has
nine of them, and they are binding rather than advisory.

Two consequences land in Phase 1. The subset has to be a real subset, so nothing outside it can be
quietly accepted. And there has to be a way for a compiled program to produce output that both
compilers agree on, which is what the runtime shim below is for.

## Part one: positions, and the errors reported at them

The file is `src/diagnostics.rs`. Nothing in it is exciting on its own; all of it is used by every
stage that follows.

### A position is a byte offset, not a line and column

The obvious way to record where something is in a file is a line number and a column number. This
project does not do that. It records a `Span` — a pair of byte offsets marking the start and end of
a range in the file:

```rust
pub struct Span {
    pub start: usize,
    pub end: usize,
}
```

The reason is that line and column numbers are only useful when printing a message to a human. They
are expensive to compute (you have to know how many newlines came before), awkward to compare, and
they change meaning if the file is split into lines differently. A byte offset is a single number.
Comparing two of them tells you which came first. Joining two spans to cover both is arithmetic.

Line and column are worked out at the very last moment, when a message is actually being printed,
and nowhere else. That conversion is the job of `SourceMap`, which scans the file once when it is
created and records where every line begins. Looking up an offset is then a binary search through
that list — repeatedly halving the search range rather than counting newlines from the top of the
file each time.

### One shape for every error, from every stage

A diagnostic — the general word for a compiler's report about your program — is a value here, not an
exception:

```rust
pub struct Diagnostic {
    pub kind: DiagnosticKind,   // which stage reported it
    pub message: String,        // the one-line description
    pub span: Span,             // where in the source
    pub notes: Vec<String>,     // extra context beneath the message
}
```

The `kind` field records which stage found the problem — lexer, parser, semantic analyzer, or the
compiler complaining about itself. That is stored rather than baked into the wording so that a test
can assert "the lexer reported this" without matching on message text, which would break every time
someone improved the phrasing.

The `notes` are the part that makes a message worth reading. The message says what is wrong; a note
says what to do instead. Reporting that an integer is too large is fine; adding "the maximum is
2147483647; write INT_MIN as -2147483647 - 1" tells you how to fix it.

### Reporting more than one problem

A `DiagnosticBag` collects diagnostics as a stage runs. A stage records a problem and keeps going
rather than stopping, so a file with four mistakes reports four of them.

The bag hands them back sorted by position, and this is deliberate rather than incidental. A stage
does not necessarily find problems in the order they appear in the file — a later stage might
resolve a forward reference and only then notice something wrong with an earlier line. Sorting at
the end means the user always reads their mistakes top to bottom, however the compiler happened to
stumble across them. The sort is stable, meaning two problems at the identical position stay in the
order they were found.

### The caret line

The rendered form of a diagnostic looks like this:

```
/tmp/errors.c:1:9: error: expected digits after '0x'
int a = 0x;
        ^~
```

The path, line, and column; the message; the offending source line reproduced; and a line of carets
underneath pointing at the exact characters at fault. That layout deliberately matches what `clang`
prints, so the two compilers' output is comparable by eye.

Three details in the caret line took actual thought, and each is a bug that would otherwise appear
eventually:

1.  Tabs. If the source line is indented with tab characters and the caret line pads with spaces,
    the caret drifts away from what it is pointing at — and drifts differently depending on how wide
    the terminal draws a tab. So the padding reproduces a tab as a tab. Widen and narrow the
    terminal and the caret keeps tracking.
2.  Multi-byte characters. A column is counted in bytes, matching what `clang` reports, but a
    single character outside ASCII occupies several bytes. Counting those as several columns would
    push the caret too far right. The padding skips the continuation bytes of a multi-byte
    character, so one character costs one column.
3.  Spans that run past the end of a line. A span can cover several lines. Underlining all of it
    would run the carets off the end of the one line being printed, pointing at nothing. So the
    underline is clamped to the end of the line, and is never narrower than a single `^` — a
    zero-width span, like "something is missing here", still has a position worth indicating.

## Part two: the token vocabulary

The file is `src/lexer/token.rs`. It defines what the lexer produces.

A token is one meaningful piece of a program. `int x = 5;` is five tokens: the keyword `int`, the
identifier `x`, the operator `=`, the integer literal `5`, and the punctuation `;`. The lexer's whole
job is to produce that list. It has no opinion about what any of it means — it does not know this is
a declaration, only that these are five distinct recognizable things.

A token here is a kind plus a span: what it is, and where it came from.

### Literals carry their decoded value

`TokenKind` has a variant per kind of token, and the ones that carry data carry it already decoded:

```rust
IntLit(i32),        // 0xff arrives as 255
CharLit(u8),        // '\n' arrives as the byte 10
StrLit(Vec<u8>),    // "a\tb" arrives as three bytes, with a real tab in the middle
```

That is a decision, not an accident. The alternative is to store the raw source text on the token
and let a later stage work out what `0xff` and `\t` mean. Doing it once, here, in the one place that
already has the source text in hand, removes an entire category of bug where two stages disagree
about what an escape sequence means. By the time the code generator emits a string, it emits stored
bytes; it never re-reads a backslash.

### The two tables that cannot drift apart

There are two places where the same knowledge is needed in both directions, and both are written
once.

The keyword table needs to answer "is this word a keyword?" when scanning, and "how is this keyword
spelled?" when printing an error. Writing those as two lists guarantees that one day someone adds a
keyword to one and not the other. Instead there is a small macro — a piece of Rust that generates
code at compile time — which takes one list of keyword-and-spelling pairs and produces both
directions from it. Adding a keyword is one line.

The escape table is the same problem. The lexer reads it left to right to turn `\n` into a newline
byte; the printer reads it right to left to turn a newline byte back into `\n` when quoting a
literal in an error message. One table, read both ways, so they cannot disagree about what `\v` is.
A test walks every entry and checks the round trip.

### A test that catches a missing case at compile time

There is a nice trick in this file worth calling out, because it is a pattern Rust makes possible
and the project reuses later.

A test needs a sample of every kind of token, to check they all have a printable spelling. The
danger is that someone adds a new token kind and forgets to add it to the sample list, so it goes
untested. The test defends against that by looping over the samples and matching each one against an
exhaustive list of every variant. Rust requires a match to cover every possibility, so adding a
variant without adding a sample stops the code from compiling. The test does not fail — the build
fails, immediately, at the place the mistake was made.

## Part three: the scanner

The file is `src/lexer/mod.rs`. This is the code that reads characters and produces tokens.

### It reads bytes, not text

Rust's normal string type guarantees its contents are valid UTF-8 — the standard way of encoding
text. The scanner deliberately does not use it. It reads a raw slice of bytes.

Two reasons. A C file is not guaranteed to be valid UTF-8; a stray byte from a corrupted paste is
perfectly possible. And this stage will later be handed deliberately random input by a fuzzer, a
tool that hunts for crashes by feeding a program arbitrary garbage. If the scanner used the
text type, a file with an invalid byte would fail to load before the compiler even started, and the
user would get a message about encoding rather than a message about their program. Reading bytes
means a malformed byte becomes an ordinary diagnostic pointing at the offending character.

### Two properties that hold for every input

These are stated in the module's own documentation, and they are the reason the scanner can be
trusted with garbage.

It terminates. Every step of the scan consumes at least one byte, so the position only ever
increases. There is no input on which the loop can spin in place forever. This is not a hope; it is
a consequence of the structure, because the first byte of a token is consumed before anything
decides what kind of token it is.

It reaches the end. A malformed construct produces a diagnostic and then resynchronizes — picks a
defensible place to resume — rather than stopping the scan. The token stream always ends with an
end-of-file marker, even after an error. This matters more than it looks: a lexer that gave up at
the first mistake would make the parser's own error recovery impossible to test, because the parser
would never see any tokens past that point.

Where it resumes is chosen per construct. An unterminated string resumes at the end of that line,
because a missing closing quote is far more likely than someone intending a string to span lines. An
unterminated block comment resumes at end of file, because there is nowhere else plausible and
guessing would silently turn the rest of the program into comment text.

### Maximal munch

When the scanner sees `<`, it has to decide whether that is a less-than operator or the start of
`<=`. The rule, which has a name, is maximal munch: at each position take the longest thing that
forms a valid token.

So `<=` is one token, never two. And a run like `a+++b` splits as `a`, `++`, `+`, `b` — at the
second character the longest match is `++`, then at the fourth the longest match is `+`. That
example is in the tests because the wrong split also produces three tokens, so counting them proves
nothing; you have to look at what they are.

### Keywords are recognized after the fact

A tempting way to find keywords is to check whether the input starts with `int`. That is wrong:
`integer` starts with `int`, and would lex as the keyword `int` followed by an identifier `eger`.

Instead the scanner reads the entire word first — every letter, digit, and underscore in a row — and
only then asks whether the finished word happens to spell a keyword. `integer` is read whole, looked
up, not found, and becomes one identifier. The bug cannot occur.

### Numbers are consumed whole before being judged

C writes integers in three bases: plain decimal, hexadecimal with an `0x` prefix, and octal with a
leading `0`. So `010` is eight, not ten, which surprises people but is genuinely what C means.

The interesting decision is about bad input. Given `0xZZ`, the naive approach reads `0x`, finds no
valid digits, reports, and stops — leaving `ZZ` to be scanned as an identifier. The user then gets a
confusing error followed by a phantom variable name. So the scanner consumes the whole alphanumeric
run first, and only then validates it. One diagnostic, covering the whole thing, pointing at all of
`0xZZ`.

Overflow gets similar care. Accumulating digits into a number that is already too large could wrap
around back into a valid-looking range, so the accumulation saturates — it sticks at the maximum
instead of wrapping. And when reporting, the message quotes the literal's own source text rather
than the accumulated number, because saturation means the accumulated number would be some unrelated
round figure that appears nowhere in the user's file.

### The token dump

`rustycc program.c --dump-tokens` prints the token stream, one per line, each with the source
positions it came from. It exists so the stage can be inspected on its own.

The kinds are printed in their internal debugging form rather than as source text, which is
deliberate: the point of the dump is to show what the scanner decided. Seeing `IntLit(255)` where
the source said `0xff` tells you the base was decoded. Seeing one `PlusPlus` rather than two `Plus`
tells you maximal munch worked.

One small thing that is easy to get wrong: the position column's width is measured from the data
rather than fixed. A fixed width wide enough for `9:9-9:12` stops lining up the moment a file
reaches four-digit line numbers — which is exactly the size of file where a dump is long enough that
alignment is what makes it readable.

## Part four: the runtime shim

A compiled program that cannot print anything is close to untestable. Comparing two compilers'
output requires there to be output; without it there is only a single numeric exit code, which is
one integer per program.

The obvious answer is C's `printf`. It is a poor fit here for two independent reasons, recorded as
[ADR 0006](../decisions/0006-fixed-arity-runtime-shim.md).

`printf` takes a variable number of arguments, and passing a variable number of arguments on ARM64
follows its own separate set of rules from passing a fixed number. Supporting that correctly is a
whole implementation effort of its own, spent entirely on making output prettier rather than on
testing the compiler better. And `printf` buffers its output, so two binaries could flush at
different moments and produce a difference in behaviour that has nothing to do with either compiler
being wrong.

So the project ships three tiny functions instead, each taking exactly one argument: `print_int`,
`print_char`, and `print_string`. They are written in ordinary C — `runtime/shim.c` is compiled by
`clang`, not by this compiler, so it is free to use the preprocessor and system headers the subset
excludes — and they write directly to the output using the lowest-level system call available.

Three details in that file are more interesting than the size suggests:

1.  Writing is retried. The system call that writes bytes is permitted to write fewer than asked,
    and to fail partway through if a signal arrives. A single call is not enough, so there is a loop
    that keeps going until everything is written.
2.  `print_int` computes digits in unsigned arithmetic. The natural way to print a negative number
    is to negate it and print a minus sign — but negating the most negative possible `int` overflows,
    which C says is undefined behaviour, meaning the compiler may do anything at all. So the
    magnitude is computed with unsigned arithmetic, which is defined to wrap, and for that one value
    yields exactly the right answer. A naive implementation prints something wrong here rather than
    crashing, which is worse.
3.  The digit loop runs at least once, so that zero prints as `0` rather than as nothing.

It is compiled once, by the build script, and the identical object file is linked into both sides of
every comparison. There is deliberately no second implementation that could itself be wrong.

## Part five: the scaffolding that makes the rest possible

Several decisions in this phase are not about compiling anything. They are about making the next
four phases possible to work on.

The compiler is a library with a thin command-line program wrapped around it. `src/lib.rs` holds a
`compile` function that takes source bytes and returns either results or diagnostics, touching no
files and starting no processes. `src/main.rs` is barely twenty lines: read arguments, call the
library, turn the result into an exit code. The reason is testing — tests can drive the whole
compiler directly, in the same process, rather than starting a program and reading its output back
as text. Everything about that is faster and gives better failure messages.

The command line already has flags for stages that do not exist yet: `--dump-tokens`, `--dump-ast`,
`--check`, `-S`. They are declared as a mutually exclusive group, so asking to stop in two places at
once is caught as a usage error. Fixing the shape of the interface early means later phases fill
things in rather than redesigning.

The lint configuration is worth mentioning because it enforces an architectural rule mechanically. A
central invariant of this project is that no stage crashes on user input — every problem becomes a
reported diagnostic. Rust has several operations that crash when things go wrong: `unwrap`,
`expect`, an explicit `panic`, and indexing a list with a position that might be out of range. All
four are configured as build failures inside the compiler's own source. They are permitted in tests,
where a crash is simply a failing test, which is the point. So the invariant is enforced by the
build rather than by someone noticing in review.

Similarly, every module, type, function, and test is required to carry a documentation comment, and
a missing one is a build failure rather than a review comment.

The toolchain version is pinned exactly, rather than set to "latest stable". Otherwise a new Rust
release turns up first as an unexplained red build on a change that had nothing to do with it.

Continuous integration runs on an Apple Silicon machine, because the target of this compiler is
Apple Silicon and the tests eventually run the code it produces. Before anything else, a preflight
step checks that the C toolchain resolves, so a missing Xcode installation becomes one clear failure
rather than a confusing linker error buried inside another job.

## What this leaves in place

The pieces Phase 1 puts down are all consumed by what comes after, which is why the order matters.

`Span` and `Diagnostic` are how every later stage reports. The parser's error messages, the semantic
analyzer's type errors, and the code generator's internal limits all render through the same caret
machinery, which is what makes the compiler's output look like one program rather than four.

The token stream is the parser's input, and the token vocabulary was written to cover the whole
grammar rather than only what the scanner happened to recognize at the time — including a printable
spelling for every token, which is what lets the parser eventually say `expected ';', found '}'` in
the user's own notation.

The two properties the scanner holds to — terminates, always reaches the end — are the same two the
parser is held to in
[Phase 2](Phase-2-Explanation.md), and eventually the whole front end under fuzzing.

And the shim is the reason there is anything to compare at all when differential testing begins.
