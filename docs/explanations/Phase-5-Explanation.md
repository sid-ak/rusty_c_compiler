# Phase 5: Differential testing, fuzzing, and system acceptance

## Goal

Stop asking whether the compiler still does what it did, and start asking whether it does what C
says it should — by putting every test program to `clang` as well, running both results, and
comparing them. Then find out what arbitrary input does to the front end, and write down what the
answer was.

## Outline

- [What Was](#what-was)
- [Overview](#overview)
- [Components](#components)
    - [The Oracle Problem](#the-oracle-problem)
    - [The Harness](#the-harness)
    - [Testing The Test Harness](#testing-the-test-harness)
    - [Growing The Corpus](#growing-the-corpus)
    - [The Bug The Corpus Found](#the-bug-the-corpus-found)
    - [The Program Generator](#the-program-generator)
    - [Fuzzing](#fuzzing)
    - [Acceptance](#acceptance)
- [Learnings](#learnings)
- [Try It Out](#try-it-out)
- [What's Next?](#whats-next)

## What Was

[Phase 4](Phase-4-Explanation.md) left a working compiler. `rustycc program.c -o program` produced a
file that ran, and seven test programs covering the grammar produced the right answers.

"The right answers" is the part worth looking at. Each of those programs carried a comment at the
top saying what it should print and what status it should exit with:

```c
// expect-exit: 0
// expect-stdout: 22128532-175017911118952012331996567\n
```

Those numbers were obtained by compiling the program with `clang`, running it, and writing down what
came out. That is better than writing down what `rustycc` produced — which would only ever confirm
that the compiler still does whatever it did on the day someone looked — but it has a shelf life.
The moment a program changes, or a new one is written, somebody has to go and get the answer again.
And a comment is a claim about a program made at one point in time; nothing forces it to still be
true.

This phase replaces the comment with the thing that produced it.

## Overview

The idea is called differential testing, and it is the reason this project compiles C rather
than a language invented for the purpose. A language nobody else implements has no second opinion
available: whatever the compiler produces is, by default, the answer. C has dozens of
implementations, one of which is already installed on the machine — so any program written in it can
be put to two compilers at once.

The test then becomes:

1. Build the program with `rustycc`.
2. Build the same program with `clang -O0`.
3. Run both.
4. Compare what they printed, what they wrote to standard error, and how they exited.

If the two agree, that is evidence about correctness which does not depend on anyone's opinion about
what the program should do. Nobody wrote the expected answer down — `clang` produced it, just by
being run. If they disagree, one of them is wrong, and it is overwhelmingly likely to be the one
that was written last month by one person.

Four pieces make that work, and this phase built all four:

- A harness that does the building, running, and comparing, and reports a disagreement in enough
  detail to act on.
- A corpus wide enough that agreement means something — sixty-four programs, covering every
  feature in the grammar and every pair of features that has to agree about something.
- A generator that writes more programs, in shapes nobody would choose, and puts them through the
  same comparison.
- Fuzzing, which asks a different question entirely: not "is the answer right" but "does the
  compiler survive input that is not a program at all".

## Components

### The Oracle Problem

Testing anything requires knowing the right answer. For most software that means a person writing it
down; for a compiler it is worse than usual, because the right answer is not a value but a *program's
behavior*, and working it out by hand means being a compiler.

An oracle, in testing, is anything that can tell you the right answer without you having to work
it out. `clang` is this project's oracle, and the whole design leans on it — but only for programs
where the right answer exists.

That last clause is the catch, and it shapes everything below. C does not define what every program
does. Some programs it deliberately leaves undefined: dividing by zero, an arithmetic result too
large to store, reading past the end of an array, reading a variable that was never assigned. For
those, a compiler may do anything at all, and two compilers doing different things is not evidence
of a bug in either one. Worse, two compilers doing the *same* thing is not evidence of correctness
either, since neither was obliged to.

So every program in the corpus has to stay inside the part of the language C actually defines. This
is not a detail that came up once — it came up four separate times while writing the programs, in
ways that looked perfectly innocent:

- `29 % (12 % 4)` is a remainder by zero, because `12 % 4` is `0`.
- `(d = 4) + d` reads `d` and writes it in one expression with nothing to say which happens first.
- `int value = value + 0;` inside a block, shadowing an outer `value`, reads the *inner* one — which
  has not been given a value yet.
- A loop that never terminates and does nothing is undefined in its own right.

`clang` found every one of them, because it warns about exactly these. So the corpus test does not
merely check that `clang` compiles each program — it checks what `clang` *says* about each program,
and requires the program to declare any warning it deliberately provokes:

```c
// clang-warns: -Wlogical-op-parentheses
```

`arithmetic.c` writes `a > b || a > b && a < b` without parentheses on purpose: that is the
precedence rule being tested, and adding the parentheses `-Wall` asks for would delete the test.
Declaring the warning keeps the program honest and keeps a *new* warning — the kind that means
something is actually wrong — from arriving unnoticed among the ones that were expected.

### The Harness

`tests/harness/` is the shared plumbing and `tests/differential.rs` is the suite built on it. The
interesting decisions are about failure rather than success.

One test per program. Rust needs its test functions to exist at compile time, so a corpus
discovered from a directory cannot simply become a list of tests. The list is generated instead:
`build.rs` reads `tests/programs/`, writes out a macro naming every `.c` file in it, and both the
differential suite and the golden suite expand that macro into one `#[test]` each. Nobody maintains
a list, so nobody can forget to add to it — which was the one way a program could be added to the
corpus and silently never run.

Comparing is separate from running. The comparison is a plain function over two records of a
run — what each printed, what each wrote to standard error, how each ended — with no compiler and no
process behind it. That split exists entirely so the harness can be handed a wrong answer on
purpose, which is the next section.

Three outcomes, not two. A naive harness has "passed" and "failed". This one distinguishes:

- The two binaries behaved differently. That is a compiler bug until proven otherwise.
- One compiler would not build the program at all. If it was `rustycc`, that is a hole in the
  subset; if it was `clang`, the corpus entry is broken C. Neither is a wrong answer, and reporting
  them as one sends whoever reads the failure looking for a code generation bug that is not there.
- The program never finished. Reported before anything else, for a reason found the hard way: a
  program killed at the timeout has printed however much escaped before the signal arrived, so two
  programs that both ran forever almost always differ on their output too. The first version of the
  harness reported that as "stdout differs", which was true and completely misleading.

Output goes to files, not pipes. A pipe has a fixed-size buffer. When it fills, the program
writing to it stops until somebody reads — and a parent process that is waiting for the program to
finish before it reads will wait forever. That turns a chatty test program into a hang that looks
exactly like a compiler emitting a broken loop. Writing to a file has no buffer to fill.

A failure names a directory. The message carries the program, the disagreement, and the path to a
directory holding both binaries, both captures of their output, and the assembly `rustycc` produced.
A failure that can only be investigated by first reproducing it is most of the way to no report at
all.

### Testing The Test Harness

The differential suite is the project's definition of done, which makes it the one piece of test
code whose passing is taken as evidence about everything else. Test code that cannot fail proves
nothing about the runs it passes.

So `tests/harness_self_tests.rs` breaks it on purpose, one way at a time:

- A wrong answer on each comparison axis, handed to the comparison directly.
- A trailing newline, on its own, as the only difference. A harness that trimmed whitespace before
  comparing would pass every other test here and then quietly miss a missing newline in every
  program in the corpus.
- A program that died on a signal, against one that returned that same number. Folding those
  together would make a segmentation fault compare equal to `return 11`.
- Two fixtures that differ only in what they print, built and linked and run for real, so the path
  between a program printing something and the harness reading it is tested and not only the
  comparison.
- A program that returns `300`, which the operating system reports as `44` — the low eight bits are
  all a parent process can see — confirmed to compare equal on both sides.
- A program that never finishes, killed and reported as a timeout, in bounded time.
- A null dereference, built by `clang` alone since it is outside the subset, confirming the harness
  reads a signal death as one.
- A program `rustycc` rejects and a file that is not C at all, each reported against the right
  compiler.

### Growing The Corpus

Seven programs became sixty-four. The target was not the number; it was two properties:

- Every feature in the grammar appears in at least three programs, so no feature rests on one
  file continuing to exist.
- Every pair of features that has to agree about something — a storage width, a register, an order
  of evaluation — appears together in at least one. Recursion with arrays. `char` with promotion
  and comparison. Arrays across a function boundary with in-place mutation. Short-circuiting with
  side effects. Globals reached from inside a recursion.

`tests/programs/COVERAGE.md` records both as tables, and a program with no row there fails the
suite — on the grounds that a program nobody wrote down the purpose of has stopped being coverage
and become a file.

Two habits run through the programs themselves:

A wrong answer should be a different answer. `2 - 2` is `0` whichever way round a subtraction
reads its operands, so a compiler that has them backwards passes it. Every non-commutative operator
in the corpus gets asymmetric operands for that reason. The same logic applies to grouping:
`100 - 30 - 20` and `100 - (30 - 20)` differ, so both are written out.

Where an algorithm can be written twice, it is. Greatest common divisor, binary search,
factorial, primality — each appears as a loop and as a recursion, and the two are checked against
each other across a whole range of inputs rather than each against one recorded value. Two different
pieces of code reaching the same answer is a stronger statement than one piece agreeing with itself.

### The Bug The Corpus Found

Writing the programs found a real defect within the first hour, in a construct that had been
working — apparently — since Phase 4.

C says that when an array is initialized with fewer values than it has elements, the rest are zero:

```c
int a[4] = {5};   /* a[1], a[2] and a[3] are 0 */
```

For a global, that falls out of how globals are stored: they live in a region of the executable
that starts out zero, so writing `5` into the first slot is the whole job. For a local, the
storage is a piece of the function's stack frame — memory that was last used by whatever function
ran before this one, holding whatever that function left in it. The zeros have to be written, or
they are not there.

The compiler wrote only the values it had been given. So `array_initializers.c` printed:

```
7-1704277921 ...
```

where `clang` printed `700`.

Three things about this are worth more than the fix:

- Every existing test passed. Every one of them read back an element the initializer had
  actually mentioned. The bug lived entirely in the elements nobody had thought to look at.
- The comment was already right. The code path that copies a string literal into a `char` array
  did zero its tail, and its comment said it was doing "the same as a short brace list" — which the
  short brace list was not doing. Both now call one helper, so the two cannot disagree again.
- The first test written for the fix passed against the broken compiler. It summed all four
  elements, and the stack leftovers in that particular frame happened to cancel to zero. An
  aggregate — a sum, a count, a "contains" — cannot detect an omission it happens to balance. The
  test that stuck checks the untouched elements individually.

### The Program Generator

A hand-written corpus plateaus. Every program in it was written by someone who already had a theory
about what might be broken, so it finds the bugs that fit a theory and then stops finding anything.
This is a known enough phenomenon to have a name — the pesticide paradox — and the answer is to
generate programs nobody chose.

`tests/generator/` writes random subset C from a seed. The hard part is not producing C; it is
producing C with a right answer, because of the oracle problem above. A generator that emitted an
overflowing multiplication once every few hundred programs would produce a suite that failed
occasionally for no reason anybody could act on, which is worse than no suite.

So undefined behavior is ruled out by construction, not by filtering afterwards:

- Every expression is built together with the interval of values it can take, computed in 64-bit
  arithmetic. `a * b` where `a` is known to be in `[-1000, 1000]` and `b` in `[-64, 64]` produces an
  interval of `[-64000, 64000]`. An operator is emitted only if its interval still fits in a 32-bit
  `int`, so overflow cannot happen rather than being unlikely.
- A divisor is always a positive literal. That rules out division by zero by inspection, and also
  `INT_MIN / -1`, which overflows for the one value that has no positive counterpart.
- Every variable carries the invariant that it holds a value within a fixed bound. An assignment
  whose interval does not fit is wrapped in a remainder by a positive literal, which brings any
  `int` into range without a branch and without evaluating anything twice.
- A subscript is a literal inside the array, or a `for` counter whose entire range the generator
  already knows lies inside it.
- Every variable and every array element is initialized where it is declared, and no generated
  expression assigns to anything, so there is nothing whose order of evaluation could matter.

That is a chain of reasoning, and chains of reasoning are wrong sometimes. So a sample of the
generated programs is also compiled with `clang -fsanitize=undefined`, which instruments the program
to complain at runtime if it does any of these things. Finding nothing is the check on the reasoning
being right rather than only careful.

Four bounds in the generator exist because their absence was found the hard way.

Two were stack overflows, both from the same shape — a rule that generates its own contents from the
list it came from:

- Statement nesting, because a statement that opens a block generates that block from the same list
  of statements it came from.
- Call nesting, because a call's arguments come from the same expression grammar the call is a leaf
  of.

The third was not a crash. Names declared inside a block stayed in the generator's idea of what was
in scope after the closing brace, so it went on offering a variable that no longer existed and wrote
programs that did not compile — which the harness dutifully reported as `rustycc` refusing to build
them, an answer that was correct and about the wrong thing entirely.

The fourth is the one worth reading. The generator reasons about the values an expression can take
so that its programs have a defined answer; nothing made it reason about how much *work* it had
asked for. A soak of two and a half thousand programs found three that ran past the harness's
ten-second limit — a loop inside a loop inside a function called from a loop, where a call that is
one statement to look at is a hundred thousand to run. One of the three finished under `clang` and
not under `rustycc`, which is an honest difference between a compiler that allocates registers and
one that spills everything to the stack, and no use at all as a test: a program that does not finish
has no output to compare.

The fix is the same technique as the value intervals, applied to time. Each statement costs the
product of the loop bounds around it, a call costs whatever the callee was estimated at, and past a
budget the generator stops offering loops and calls.

A fifth correction was about volume rather than termination. The first version printed from
everywhere, and the same runaway shape produced seventeen megabytes of output — which the harness
then reported as a stdout difference between two programs that had both been killed at the timeout.
Now only the top level of `main` prints; everything computed inside a loop or a function folds into a
single global, which `main` prints at the end. The fold is order-sensitive, so a wrong value anywhere
still changes it.

Everything is seeded: the same seed produces byte-identical source, and a failure prints its seed.
That turns a failure from a story about a run that has already finished into a file.

### Fuzzing

Everything above asks whether the compiler produces the right answer for a program. Fuzzing asks
something else: what happens when the input is not a program.

A compiler is handed files by people, and people hand it broken ones constantly — a missing brace, a
half-finished line, occasionally a file that is not source code at all. The rule this project holds
to is that no pass may panic on user input: whatever arrives, the compiler either compiles it or
explains what is wrong with it. Crashing is not an acceptable third option, because a crash tells
the user nothing and, in a compiler that ran on untrusted input, would be a security problem.

`cargo-fuzz` generates input by mutating what it already has and keeping whatever reaches code it
has not reached before. Three targets:

- `lex` — raw bytes into the scanner.
- `parse` — raw bytes through the scanner and the parser.
- `frontend` — raw bytes through the whole front end including semantic analysis, which has its own
  recursion, its own indexing, and a side table keyed by node identity, and which is handed trees the
  parser built while recovering from an error. That is the one shape it never sees in ordinary use
  and the one most likely to break it.

Each target asserts a little more than "it did not crash", because a target that only checks for
panics is blind to a pass that returns something nonsensical without crashing. The token stream must
end in an end-of-file token and every span must point inside the input. A tree must dump. A program
must be either accepted or reported on, never neither.

The failure mode fuzzing finds in a recursive-descent parser is not usually a wrong tree — it is a
stack overflow. Ten thousand nested parentheses cost ten thousand nested function calls, and a
stack is finite. That was anticipated rather than discovered: the parser carries a depth limit that
turns deep nesting into an ordinary diagnostic, and semantic analysis carries its own, because it
walks the same tree a second time.

The seed corpus is not checked in. `scripts/fuzz.sh` builds it from what the repository already has
— the valid programs, the invalid ones, the adversarial inputs Phase 2 collected, and any past
crash — because a second copy of those files under `fuzz/` would be one more thing to keep in step,
and it would go stale the first time a program was added.

A crash the fuzzer finds does not stay in a fuzz corpus, which is machine-specific, regenerated, and
never consulted before a merge. It is minimized and checked into `fuzz/regressions/`, where the
ordinary test suite runs it on every change from then on.

### Acceptance

The project's stated definition of done, written before any of it was built, is one run: every
program in the curated suite, covering every supported feature, behaving identically under `rustycc`
and under `clang -O0`, with no known mismatches.

[`docs/reports/acceptance.md`](../reports/acceptance.md) records that run — the program count, the
result, the versions of everything involved, and the commit it describes — so the claim is a record
rather than a memory.

## Learnings

A test that cannot fail is not a test. This applies most sharply to the thing everything else is
measured against. The comparison function was split out from the running specifically so a wrong
answer could be handed to it, and the first three self-tests written that way each found a real gap
in what it checked.

An aggregate assertion cannot detect an omission. A sum, a count, a "contains" — each can be
satisfied by the wrong values as easily as the right ones. The first test for the zero-fill bug
summed four array elements and passed against the broken compiler because the leftovers cancelled.
Assert over each case, not over a fold of them.

Two compilers agreeing about an undefined program proves nothing. This is the one constraint
that shapes the whole phase: the corpus, the generator's interval arithmetic, and the decision to
check what `clang` warns about rather than only whether it succeeds. An oracle is only an oracle
where an answer exists.

Reason about cost the way you reason about values. The generator's interval arithmetic made its
programs *correct*; nothing made them *finish*. Both are properties of a generated program that a
test depends on, and only one of them had been thought about — which is why three programs in two
and a half thousand were useless as tests and looked like compiler bugs.

Say which thing went wrong, not that something did. A mismatch, a program one compiler would not
build, and a program that never finished are three different situations with three different next
steps. The first version of the harness collapsed the third into the first, and the resulting message
was true, unhelpful, and actively misleading about where to look.

Generating a list beats maintaining one. The corpus listing comes out of `build.rs` reading the
directory. There is no list to forget to add to, which removes a failure mode rather than testing
for it.

## Try It Out

Each of these is a real command with real output; see
[`docs/CHEATSHEET.md`](../CHEATSHEET.md) for the full set.

1. `cargo test --test differential`: the acceptance suite — every corpus program built twice, run
   twice, and compared.
2. `cargo test --test differential -- arrays`: one program, by name, since each is its own test.
3. `cargo test --test harness_self_tests`: the harness broken on purpose, one axis at a time.
4. `cargo test --test generated`: forty random programs through the same comparison.
5. `RUSTYCC_GENERATED_PROGRAMS=500 cargo test --test generated`: more of them.
6. `./scripts/fuzz.sh lex`: fifteen minutes on the lexer, seeded from every program in the
   repository. Needs `rustup toolchain install nightly && cargo install cargo-fuzz` first.

To see the comparison by hand, without the harness:

```bash
SHIM=$(find target/debug/build -name shim.o | head -1)
clang -O0 -std=c99 tests/programs/bubble_sort.c "$SHIM" -o /tmp/oracle
./target/debug/rustycc tests/programs/bubble_sort.c -o /tmp/ours
diff <(/tmp/oracle) <(/tmp/ours) && echo identical
```

## What's Next?

Nothing in the plan. All five phases are built, and the acceptance criterion the proposal set is
met.

What the structure leaves open is written down in
[architecture.md](../architecture.md#future-scope), and the differential suite is what makes each of
those approachable rather than terrifying. An intermediate representation between analysis and code
generation, real register allocation instead of spilling everything to the stack, a second target
architecture — every one of them is a change that rewrites how programs are compiled, and every one
of them has the same safety net: sixty-four programs and a generator, each of which still has to
produce exactly what `clang` produces afterwards.

That is the argument for having built this phase at all. A test suite is not only a way of finding
out that something is broken; it is what makes the next large change something a person is willing
to start.
