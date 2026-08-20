# Phase 2: The AST and the recursive-descent parser

A walkthrough of what Phase 2 built and why each choice was made, written for a reader with no
background in Rust and none in compilers. It assumes [Phase 1](Phase-1-Explanation.md), which built
the lexer this stage reads from, but re-introduces anything it leans on.

## The problem this phase solves

Phase 1 produced a lexer. A lexer reads the raw characters of a file and chops them into meaningful
pieces called tokens — it turns `int x = 5;` into five things: the keyword `int`, the name `x`, the
symbol `=`, the number `5`, the symbol `;`. That is all it does. It has no idea this is a variable
declaration. It does not know that `x` is being declared, or that `5` is what it starts out holding.
It is like reading a sentence and identifying the individual words without understanding the grammar
holding them together.

Phase 2 is the grammar step. It takes that flat list of tokens and discovers the structure hiding
inside it — which piece belongs to which, what is nested inside what. The output is a tree.

## Why a tree, and why it matters so much

Consider `1 + 2 * 3`. As a flat list of tokens that is just five things in a row. But anyone who
learned order of operations knows it means `1 + (2 * 3)`, which is 7, and not `(1 + 2) * 3`, which
is 9. The flat list does not record that anywhere. A tree does:

```
        +
       / \
      1   *
         / \
        2   3
```

Read it from the bottom: multiply 2 and 3 first, because they sit deeper in the tree, then add 1.
The nesting is the order of operations. There is no separate rule needed; the shape carries the
meaning.

This structure is called an Abstract Syntax Tree, or AST. "Abstract" because it discards the things
that existed only to help a human read — the whitespace, and the parentheses themselves, once their
grouping has been recorded in the shape. The program that builds it is called a parser, and it is
the bulk of this phase.

## Part one: defining what a tree node looks like

The first file, `src/ast.rs`, does no work at all. It only describes shapes. It says an expression is
one of ten things — a number, a character, a string, a name, a unary operation, a binary operation,
an assignment, an array index, a function call, or a post-increment. And a statement is one of ten
things — a block, an `if`, a `while`, a `for`, a `return`, and so on.

That sounds like paperwork, and it is the opposite. This file is the contract between three separate
stages. The parser writes trees in this shape. The semantic analyzer, coming next, reads them. The
code generator after that reads them too. Settling the shape before writing a line of parsing code
is what stops those three from drifting apart.

Three decisions inside this file are worth explaining, because they are the ones that took thought.

### The tree never changes after it is built

The next phase's job is to work out the type of every expression — "this addition produces an
`int`", "this variable lives eight bytes into the function's memory". The obvious place to record
that is a blank field on every node, filled in later:

```
expression node:
  kind: Addition
  left: ...
  right: ...
  resolved_type: <empty for now, filled in by the analyzer>
```

That is what most compilers do. This project deliberately does not, for a reason specific to how it
is being built. Parser tests here work by dumping a tree to text and comparing it against a saved
copy of what that text should be — a technique called snapshot testing. If a later stage writes into
the tree, every one of those saved copies changes, and you find yourself re-approving dozens of
files for a change that had nothing to do with parsing. Across five phases built one after another,
that churn compounds until the earlier tests stop meaning anything, because nobody reads a diff they
have been trained to rubber-stamp.

So instead: every node gets a unique number, a `NodeId`. The analyzer will build a separate lookup
table — node 47 has type `int`, node 48 has type `char` — and the tree itself is never touched
again. This was settled before the phase began, in
[ADR 0004](../decisions/0004-immutable-ast-with-side-table-annotations.md); Phase 2 is the first
phase that has to honour it.

The interesting part is how it is enforced. Writing "do not add mutable fields" in a comment is
worthless, because someone will do it anyway in six months and the comment will not stop them.
Instead there is this test:

```rust
fn require_sync<T: Sync>() {}
require_sync::<Program>();
```

Rust has a concept called `Sync`: a type is `Sync` when it is safe for two threads to look at it
simultaneously. Anything that can be quietly modified while somebody holds a reference to it is not
`Sync` — Rust's standard "quietly modifiable" types are specifically marked that way. So demanding
that the whole tree is `Sync` turns sneaking mutable state into a node into a compile error. Not a
failing test. The code does not build at all, at the moment the mistake is made. The type system is
doing the enforcing, which is a large part of why this project is written in Rust
([ADR 0002](../decisions/0002-rust-as-implementation-language.md)).

### Parentheses vanish

`((((1))))` produces exactly the same tree as `1`. There is no "parenthesized expression" node type.
Parentheses exist to tell the parser how to group things, and once the tree records the grouping
they have done their job and have nothing left to say. Keeping them would mean every later stage has
to remember to look through them, and one day one of them would forget.

There is a pleasant side effect. In real C, `(a) = 1` is legal — you may assign to a parenthesized
variable. Because the parentheses leave no trace, the parser sees a plain variable on the left and
accepts it, with no special case written anywhere. The correct behaviour falls out of the design
rather than being added to it.

### Not everything gets an identity

Not every piece of the tree carries a `NodeId`. The rule is that a node is something a later stage
can have an opinion about. Statements and expressions qualify. The type written in a declaration —
the `int` in `int x;` — does not, and neither does the name being declared; those are parts of the
declaration that spells them out, not things in their own right.

The same reasoning removed an identity originally given to blocks, the `{ ... }` groupings. A
function's body belongs to that function, and a nested block belongs to the statement holding it.
Nothing was left for a second identity to name, so it went.

## Part two: the parser

Two files. `src/parser/mod.rs` handles declarations and statements; `src/parser/expr.rs` handles
expressions, which are shaped differently enough to be worth reading separately.

### The basic technique

The technique is called recursive descent, and the name describes it exactly. You write one function
per rule in the language's grammar, and each function calls the functions for the rules nested
inside it. The function for `if` calls the function for expressions to read the condition, then
calls the function for statements to read the body — and that body might be another `if`, so the
statement function calls the `if` function again.

"Recursive" because functions end up calling themselves, directly or in a circle. "Descent" because
you start at the top, a whole program, and work downward to the smallest pieces. The functions call
each other in the same shape the grammar has, which is why
[the grammar file](../architecture.md#the-language-subset) and the parser code read as mirrors of
one another.

Good behaviour falls out of that shape for free. The classic ambiguity in C is the dangling else:

```c
if (a) if (b) x; else y;
```

Which `if` owns that `else`? C says the nearest one, the inner `if (b)`. In recursive descent you
get this without writing a rule: when the inner `if` finishes its body and sees an `else` sitting
there, it takes it, because it is the function currently running. The outer `if` never gets the
chance. It is a consequence of the call stack rather than a decision anyone implemented.

### Expressions, and why they are handled differently

Precedence is the hard part. The grammar expresses it by nesting rules ten deep — the loosest-binding
operator on the outside, down through each level, to the tightest-binding things at the centre. The
textbook approach is one function per level, mirroring it exactly.

That is not what happened here, and it is the one place this phase deliberately departed from the
obvious approach. Those six functions would be near-identical: same loop, same structure, differing
only in which operator symbols they check for and which function they call next. That is six places
that must be edited together every time anything about precedence moves, which is precisely the kind
of duplication this project treats as a defect rather than a style preference.

The alternative is a technique called precedence climbing. One function, plus a table:

```rust
TokenKind::PipePipe => (BinOp::Or, 1),       // ||  binds loosest
TokenKind::AmpAmp   => (BinOp::And, 2),      // &&
TokenKind::EqEq     => (BinOp::Equal, 3),    // ==
// ...
TokenKind::Star     => (BinOp::Multiply, 6), // *   binds tightest
```

The single function loops, and each time it takes an operator it calls itself with a minimum
strength one higher than the operator it just consumed. That recursive call therefore accepts
anything that binds more tightly and refuses anything looser — which is exactly what "descend a
level" meant in the ten-function version. The numbers in the table are those nested grammar rules,
counted from the outside in. Same behaviour, one place to change it.

Associativity comes out of the same function. The loop makes `100 - 10 - 1` group leftward, because
each result becomes the left-hand operand of the next operator. Assignment has to go the other way,
since `x = y = 9` must mean `x = (y = 9)`, so that one is handled by a recursive call rather than by
the loop.

All of this is tested on the shape of the tree, never on results. That distinction matters more than
it sounds: asserting that `1 + 2 * 3` equals 7 would also pass if precedence were broken in a way
that happened to produce 7 for that particular input. Asserting the tree shape cannot be fooled that
way, which is the whole reason the dump exists.

### Error recovery: reporting more than one mistake

If a file has four typos, the user wants four messages, not one. But the moment the parser hits
something it does not understand, it is lost — it has no idea where the broken construct ends and
the next good one begins.

The standard solution is called panic-mode recovery, and it is cruder than the name suggests. On an
error: record a message, then throw tokens away until you reach something that reliably marks the
end of a construct — in C, a `;` or a `}` — and start fresh. The broken statement is sacrificed;
everything after it still gets checked.

In Rust this is expressed through the type system. Every parsing function returns either the thing
it parsed or a marker meaning "I gave up, and I have already reported why". Rust's `?` operator
makes that marker travel upward automatically, so a failure deep inside an expression climbs out to
the enclosing loop without every function in between having to check for it.

Two details keep this from going wrong, and both are real failures that had to be designed against.

The first is looping forever. If a construct fails on its very first token, recovery might skip
nothing at all — and then the loop tries the same token again, fails again, and never stops. The fix
is explicit rather than hoped for: the loop records where the failed construct started, and if
recovery skipped literally nothing, it consumes one token itself. Every cycle is guaranteed to move
forward, so the file always ends.

The second is brace depth. Consider a missing semicolon at the end of a function:

```c
int f(void) { return 1 }   // no semicolon
int g(void) { return 1; }
```

Recovery reaches the `}` that closes `f`. If it swallows that brace, it believes it is still inside
`f`, and `g` is eaten alive — one missing semicolon destroys the rest of the file. So the rule is:
while skipping, count braces. A complete `{ ... }` inside the wreckage is stepped over as a unit.
But a `}` that closes the block the parser is actually standing in is left exactly where it is, for
the block parser to find. There is a test named after this scenario.

### Telling "not supported" apart from "wrong"

This project compiles a deliberate subset of C. `struct`, `switch`, pointers, `+=`, the `? :`
operator — all real C, all deliberately excluded, and the full list is in
[the language subset](../architecture.md#the-language-subset). Somebody who writes `struct point p;`
has not made a mistake. They have written valid C that this compiler does not implement, and telling
them "expected a type, found 'struct'" is both misleading and unhelpful.

So everything outside the subset now reports `unsupported in this C subset: X`. Getting that right
meant handling four separate situations, because how it is caught depends entirely on how it is
spelled.

Words, such as `struct`, `switch`, `sizeof`, and `float`. To this lexer these are ordinary
identifiers — they are not keywords in this grammar, so they arrive looking exactly like a variable
name would. The parser holds a list of them, and this is the part worth noticing: the list is
consulted in the single place the parser ever turns a token down. That one location means every
rejection anywhere in the parser automatically upgrades to the better message whenever the better
message is the true one, without a check being added at a dozen call sites. The list itself is
C89's complete set of 32 keywords minus this subset's ten, and a test asserts that the two lists
partition those 32 exactly — so a word real C reserves can never quietly be accepted as a variable
name.

Shapes, such as the `*` in `int *p;`. It sits between the type and the name. Because all three
top-level declaration forms share a "read a type, then read a name" prefix that is written once,
there is a single place to notice the `*` and name it as a pointer declarator.

Two tokens spelling one operator, such as `+=` and `<<`. This one is subtle. Since the subset has no
`+=`, the lexer has no token for it, so it hands over a `+` and then an `=` as separate things. The
parser would ordinarily read `a + = b` and complain about an unexpected `=`, which explains nothing.
So the expression parser checks whether the two tokens were written touching — using their byte
positions in the source file — and if so names the operator. That adjacency check is why `a < -b`
still parses normally while `a << b` gets called out.

Punctuation with no token at all: `#`, `?`, `:`, `^`, `~`, and a lone `&` or `|`. These never reach
the parser, because there is nothing to hand it. The lexer is the only stage that ever sees them, so
it has to be the one to report. This is the change that reached back into Phase 1's messages, which
the last section returns to.

### The stack limit, and the part that was wrong at first

Recursive descent recurses. Feed it two thousand nested opening parentheses and the function calls
pile up until the call stack — a fixed-size region of memory holding one entry per function call
currently in progress — runs out. That is a hard crash. And "no stage crashes on user input" is one
of this project's three stated invariants; a crash gives the user nothing, and unlike a diagnostic it
cannot be caught and reported.

So there is a counter. Every recursive entry point passes through one function that increments a
depth count and, past a limit, reports "nesting is too deep" instead of descending further.

That limit was first set to 256, reasoning by analogy with clang's default. Then it was measured,
and it was wrong: 256 levels overflows a stack of one megabyte. Each level costs roughly four
kilobytes in an unoptimized build, far more than estimated. The default stack a thread gets in the
test harness is two megabytes, so at 256 the guard sat at roughly half the available stack —
passing only because of that default, and one slightly fatter function away from failing.

In a sense that is worse than having no guard: a limit tuned so finely that it only just fits
relocates the crash rather than removing it. The limit is now 128, using about seven hundred
kilobytes — comfortable inside two megabytes, and a small fraction of the eight the real program
gets.

Rather than trusting that, there is a test that starts a thread with a deliberately undersized
512-kilobyte stack and parses input deep enough to reach the limit. It was verified to actually bite
by temporarily setting the limit back to 256, at which point it aborts with a stack overflow. So if
anyone later makes a parsing function hold more local data, that test fails loudly instead of the
crash quietly returning.

## Part three: proving it works

### The corpus

`tests/programs/` holds five complete, working C programs — arithmetic, control flow, functions,
arrays, strings. These are not fragments. They are real programs that print output and return an
exit code.

Writing them during this phase rather than later is deliberate, because each one is used three
separate times. Now they are what the parser's snapshot tests run against. In the code generation
phase they are the programs that have to compile and run correctly. In the final phase they are
handed to `clang` as well, both binaries are run, and the outputs compared — which is this project's
[definition of correct](../architecture.md#testing-architecture). Three uses for one file.

`COVERAGE.md` beside them records what each program is for and which language feature each one
exercises, and a test enforces it: add a `.c` file with no row in the matrix and the build fails.
Without that, a corpus slowly stops being a record of what is covered and becomes a pile of files
nobody can account for.

### Snapshot tests

The tree is dumped to indented text and compared against a saved copy:

```
(binary +
  (int-lit 1)
  (binary *
    (int-lit 2)
    (int-lit 3)))
```

Change the parser and any difference shows up as a readable diff rather than one cryptic failed
assertion.

All five of these were read before being accepted, which the project's conventions require and which
is not ceremony. In the control-flow tree the thing being checked was that the `else` had attached to
the inner `if`; in the arithmetic tree, that `*` sat below `+`, that `100 - 10 - 1` grouped leftward,
and that `x = y = 9` grouped rightward. Those are exactly the things that look fine at a glance and
are wrong.

Two properties are also checked directly rather than assumed: every corpus program parses without
complaint, and dumping the same tree twice produces byte-identical output. The second sounds
trivial and is not — a snapshot test is only meaningful if the thing being snapshotted depends on
nothing except its input.

### The truncation corpus

This is the cheapest good idea in the phase. A later phase will run a fuzzer — a tool that throws
random and mutated input at the compiler for minutes at a time, hunting for crashes. That needs a
separate toolchain and real wall-clock time.

In the meantime: take every program already in the corpus, cut it short at every single byte offset,
and parse each fragment. `int main(void) { ret` is a fragment. So is `int main(void) { "unclos`. A
prefix of a valid program is precisely the shape that breaks parsers — a construct opened and never
closed — and there are thousands of them for free, because the programs are already written. The
whole thing runs in under a second as part of the ordinary test suite.

What is asserted is intentionally weak: each fragment has to finish, and may complain about
whatever it likes. A fragment is not a program, so there is no right answer to check against. The
only wrong answers are crashing and hanging. One extra test stops that from being vacuous, by
confirming that a half-finished program does get reported rather than silently accepted.

Alongside it sits `tests/adversarial/`, ten hand-written files aimed at how a parser breaks rather
than at what it supports: two thousand nested parentheses, a file of nothing but operators, a
60-kilobyte identifier, an empty file, a file containing only a comment. They are checked in as
files rather than generated in code so that a crash found once can never quietly come back.

## The Phase 1 messages that changed

Three lexer error messages that Phase 1 had pinned with tests were changed, which is worth being
explicit about since it reaches backwards into finished work.

The forcing issue was `#include`. The task required it to report as unsupported, and only the lexer
can do that — `#` is not a token in this grammar, so nothing about it ever reaches the parser.
Previously `#` was reported as a stray character, which is not true; `#include` is C, it is simply C
this compiler does not have.

Once that was open, `&` and `|` were phrased as "'&' is not supported in this C subset" while the
parser was about to start saying "unsupported in this C subset: 'struct'". Two phrasings for one
idea, differing only by which stage happened to notice. They were unified onto a single shared
constructor.

The line that was kept: a byte that is not C at all, such as `@`, is still a stray character. The
distinction being drawn is between "this compiler does not implement that" and "that is not a
program".

## What this leaves in place

The AST is now the contract the next two stages read, and because nothing may write into it, it will
be the same tree in the final phase that it is today.

The dump is the assertion mechanism every later parser test uses, and it is also the thing a person
reaches for when a program is behaving strangely and the question is what the compiler thinks was
written.

The two properties the parser holds to — reports everything and keeps going, and never crashes —
are the same two the lexer holds to, and the same two the whole front end is eventually held to
under fuzzing.

What the parser deliberately does not do is decide whether a program makes sense. It will happily
build a tree for a program that uses an undeclared variable, calls a function with the wrong number
of arguments, or assigns an array to an integer. Every one of those is well-formed C text and
badly-formed C meaning, and separating those two questions is what
[semantic analysis](../architecture.md#semantic-analysis) is for.
