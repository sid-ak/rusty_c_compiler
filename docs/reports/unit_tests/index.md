# Unit Test Reports

One report per unit of the compiler: what the unit is, how it was tested and why that way, what is
covered and what deliberately is not, every test it has, and the outcome of running them.

They are written for a reader who has not seen the code. Each opens by saying what the unit is in
plain terms before it says anything about how it is tested, because a test strategy only makes sense
once you know what is being defended.

## How to read one

Every report has the same shape:

- **Unit** — what this piece of the compiler is, what it owns, and what it deliberately does not
  know about.
- **Test Methodology** — the techniques used, and the reasoning for choosing them over the
  alternatives. This is the part worth reading; the rest is bookkeeping.
- **Test Coverage** — what is covered, and the gaps that are deliberate, with the reason each one
  belongs to a different unit's report.
- **Automated Test Code** — every test in the unit, with the behavior it pins.
- **Actual Outputs** — the result, and where the unedited evidence for it is checked in.

## A note on the tables

The table of tests in each report is generated from the tests themselves, by
`scripts/test_inventory.py`, out of the doc comment every test carries. Those comments are mandatory
in this repository and `#![deny(missing_docs)]` makes an absent one a build failure, so the
description of a test already exists next to the test; copying it into a document by hand is how a
document starts disagreeing with the code it describes.

The generation is checked rather than trusted. `scripts/test_inventory.py --check` runs as part of
the documentation build and fails it if any table is out of date, if any file of tests is claimed by
two reports, or — the case that matters most — if a file of tests exists that no report claims at
all. A unit added without a report is exactly the omission an index of reports cannot otherwise
notice.

## The reports

Start with the environment if you intend to reproduce anything; otherwise the reports follow the
order data moves through the compiler.

- [00 — Development and Test Environment](00-environment.md): every version, and how to recreate
  both environments from a clean machine.
- [01 — Diagnostics](01-diagnostics.md): positions in a file, error messages, and the caret
  renderer every other pass reports through.
- [02 — The Syntax Tree](02-ast.md): the node types the parser builds and the later passes read,
  and the deterministic dump the parser's own tests are written against.
- [03 — Lexer: The Token Model](03-lexer-token-model.md): the vocabulary — every keyword, operator,
  punctuator, and literal form the language has.
- [04 — Lexer: The Scanner](04-lexer-scanner.md): turning raw bytes into that vocabulary, including
  every way the bytes can be wrong.
- [05 — Parser: Expressions](05-parser-expressions.md): precedence and associativity, which is the
  part of a parser that is wrong most often and visibly least often.
- [06 — Parser: Statements and Recovery](06-parser-statements-recovery.md): declarations,
  statements, and carrying on sensibly after a syntax error.
- [07 — Semantic Analysis: The Type Model](07-sema-type-model.md): what a value is, how big it is,
  and what it may be used as.
- [08 — Semantic Analysis: The Scope Stack](08-sema-scopes.md): which declaration a name refers to,
  and where each variable will live.
- [09 — Semantic Analysis: The Analyzer](09-sema-analyzer.md): the thirty-one checks, and everything
  recorded for the code generator to read.
- [10 — Code Generation: The Emitter](10-codegen-emitter.md): assembly as a document — sections,
  symbols, labels, and directives.
- [11 — Code Generation: Frames and Lowering](11-codegen-frame-and-lowering.md): where every value
  lives, and the instructions that move it.
- [12 — The Driver and the Command Line](12-driver-and-cli.md): turning assembly text into a file
  that runs, and the flags that ask for each stage.
- [13 — The Runtime Shim](13-runtime-shim.md): the three output functions every compiled program
  links against, and the one piece of C in this project that `rustycc` does not compile.
- [14 — The Differential Harness](14-differential-harness.md): the test infrastructure itself —
  tested, because it is what everything else is measured against.

## Where the rest of the evidence is

These reports cover the unit and integration tiers. Two things they deliberately do not cover live
elsewhere:

- The acceptance run — every corpus program under both compilers, the generated corpus, and the
  fuzz targets — is in [the acceptance report](../acceptance.md).
- Which language features the test corpus actually reaches is in
  [`tests/programs/COVERAGE.md`](https://github.com/sid-ak/rusty_c_compiler/blob/main/tests/programs/COVERAGE.md),
  as a table per feature and a table per pair of features that have to agree with each other.
