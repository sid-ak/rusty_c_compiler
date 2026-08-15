# Pragmatic Testing

This document maps the testing vocabulary and technique catalog from Rex Black's _Pragmatic Software
Testing: Becoming an Effective and Efficient Test Professional_ onto this project's testing strategy
— what [`architecture.md`](architecture.md#testing-architecture) and [`PLAN.md`](PLAN.md) describe,
and a few concrete suites the corpus could adopt directly. It draws on the book's general framework
of technique names and categories rather than quoting it directly, since the goal here is
application, not summary. Several of the mappings below point at specific places in `PLAN.md` where
a phase's own "Tests" section already calls for a technique from the book, sometimes without naming
it.

## How the book organizes the subject

Black's book, like most of the professional testing literature it draws from, organizes technique
around a few recurring axes worth naming up front because the rest of this document is structured
around them:

- Test levels: "How much of the system is under test at once."
    - Unit, integration, system, acceptance
- Test types: "What property is being checked."
    - Functional, structural, non-functional, and change-related (confirmation and regression)
- Technique categories: "How the individual test cases get chosen."
    - Specification-based (black-box), structure-based (white-box), and experience-based
- Cross-cutting practices: "How a team decides what to test first and what to do with what it
  finds."
    - Risk-based prioritization, static testing (review without execution), and defect management.

## Test levels

The project's four test tiers, defined in [`architecture.md`](architecture.md#test-tiers), map
fairly cleanly onto the book's test levels, with one twist worth naming: this project's "system
under test" is a five-stage internal pipeline before it is ever a whole program, so an internal
level exists here that a typical application doesn't have.

- Unit level: the in-crate `#[cfg(test)]` tests per `PLAN.md` phase — one test per token category in
  Phase 1, one test per grammar production in Phase 2, table-driven positive/negative tests per
  semantic rule in Phase 3.
- An internal pass-boundary level with no single standard name in the book, but functionally an
  integration level: snapshot tests of the AST dump, the annotated AST, and the emitted assembly
  check that one compiler pass handed the next pass the right shape of data, without yet running the
  resulting program. This is integration testing between the stations described in
  [the pipeline](architecture.md#the-pipeline) rather than between external systems.
- System level: golden-program tests (Phase 4) and differential tests (Phase 5) both run a whole
  compiled _C_ program end to end and check its observable behavior — this is the level at which the
  thing under test stops being "the compiler's internals" and becomes "a program someone actually
  ran."
- Acceptance level: Phase 5's exit criterion — the full curated corpus passing differentially
  against `clang -O0` with zero known mismatches — is stated in `PLAN.md` as the literal,
  non-negotiable definition of "done" for the project. That is an acceptance test in the textbook
  sense: a stakeholder-facing pass/fail gate tied to the original proposal, not an implementation
  detail.

## Test types

- Functional testing — does the software do what it's supposed to — is carried almost entirely by
  differential testing: `clang` supplies the expected behavior for every program, so there is no
  separately maintained oracle to go stale. See
  [ADR 0001](decisions/0001-subset-of-c-with-clang-as-oracle.md).
- Structural (white-box) testing — does the _code_ itself get exercised — is present in spirit
  (`PLAN.md`'s "every grammar production has at least one positive test," "every check listed above
  has a passing positive and negative test") but without a _measured_ coverage discipline behind it.
  See [Control Flow Testing](#control-flow-testing) below for a concrete proposal to close that gap.
- Change-related testing (confirmation and regression) is architecturally load-bearing here, not an
  afterthought: the reason the AST is immutable with side-table annotations
  ([ADR 0004](decisions/0004-immutable-ast-with-side-table-annotations.md)) is specifically so that
  parser snapshots from an earlier phase never need to be rewritten because a later phase changed
  behavior — regression protection is designed into the data model, not bolted on as a test policy.
  Every fuzz crash becoming a permanent unit test (`PLAN.md` Phase 5) is confirmation testing in its
  purest form: fix the bug, then keep re-running the exact case that found it, forever.
- Non-functional testing — performance, usability, security, portability — is deliberately absent.
  This is not an oversight; it falls out of [ADR 0003](decisions/0003-single-target-arm64-macos.md)
  and [ADR 0005](decisions/0005-stack-spilling-instead-of-register-allocation.md), both of which say
  plainly that this project accepts slow, single-platform output in exchange for a smaller, more
  legible correctness surface. In the book's risk-based framing (below), that is exactly the right
  call for a compiler whose stated goal is to learn correctness-critical construction, not to ship a
  production toolchain.

## Specification-based (black-box) techniques

These are techniques that design test cases from a description of expected behavior — the grammar,
the C standard's rules, a spec — without looking at the implementation.

### Equivalence partitioning and boundary value analysis

The book treats these two together because in practice they are: partition the input space into
classes that should behave the same way, then specifically test the boundaries between classes,
because that is where off-by-one mistakes live. This project already applies both, concretely:

- `PLAN.md` Phase 4 calls for "large constants near `i32::MIN`/`i32::MAX`" — a direct boundary-value
  case on the `int` domain.
- The same phase calls for testing every non-commutative operator "with asymmetric operands," naming
  the specific reason: "a transposed lowering passes on symmetric operands, so symmetric cases prove
  nothing here." That is boundary/equivalence-class reasoning applied not to the _language_ but to
  the _code generator's own register convention_ — see
  [the six-step lowering shape](architecture.md#code-generation) and its `w0`/`w1` operand-order
  discussion.
- Nine-argument functions are called out specifically in both Phase 2 and Phase 4 because nine is
  the first arity that crosses AAPCS64's eight-register boundary — a boundary value on the _calling
  convention_, not on an arithmetic type. This is the same technique applied to a structural
  boundary rather than a numeric one.

### Decision table testing

A decision table enumerates combinations of conditions and the expected outcome for each
combination, which is most useful exactly where several independent rules can interact. Semantic
analysis's check list in `PLAN.md` Phase 3 — undeclared identifier, redeclaration, arity mismatch,
non-lvalue assignment, and a dozen more — is tested one rule at a time. A decision table becomes
useful at the _interactions_: for example, "indexing a non-array, non-pointer expression" and
"non-integer subscript" are two separate rules, but a program can violate both at once (`x[y]` where
`x` is an `int` and `y` is a `char *`), and a table is the natural way to make sure the diagnostic
reported in that case is deliberate rather than whichever check happens to run first.

### State transition testing

This technique models a system as states and the transitions between them, then tests both valid
transitions and — often more valuable — attempted invalid ones. The compiler's control-flow analysis
has two genuine state machines worth modeling this way:

- The scope stack ([semantic analysis](architecture.md#semantic-analysis)): states are nesting
  depths, transitions are entering and leaving a function body, a block, or a `for` init clause. The
  existing tests ("shadowing in a nested block resolves to the inner binding, and the outer binding
  is visible again after the block") are already state-transition tests in substance; naming them as
  such makes it easy to check the state diagram systematically — for example, does it cover
  re-entering a sibling block after leaving one, or a `for` loop nested directly inside another
  `for` loop's init clause?
- The loop-context stack that tracks valid `break`/`continue` targets during code generation: states
  are "outside any loop," "inside one loop," "inside a nested loop," and transitions are entering
  and leaving each. `break` and `continue` are each a transition-dependent operation — legal in some
  states, illegal in others — which is precisely what this technique is for.

## Structure-based (white-box) techniques

These design test cases by looking at the implementation itself — control flow, data flow — rather
than only at its specification. This is the category where the project's current design has the most
room to grow, and where the two techniques below are worth calling out individually.

### Control Flow Testing

Control flow testing designs test cases from the code's control-flow graph: which statements ran
(statement coverage), which branches of a decision were taken (branch/decision coverage), and which
combinations of conditions inside a compound decision were exercised (condition coverage). It exists
because a test suite can pass every specification-based case and still never have executed some
branch of the actual code — an error-handling `else` that nothing in the corpus happens to trigger,
for instance.

This project's own source is an unusually good fit for this technique: a hand-written recursive
descent parser and a hand-written code generator are both, by construction, long chains of `match`
arms and conditionals — one per grammar production, one per expression form. `PLAN.md`'s instinct to
require "one test per statement form" and "every grammar production has at least one positive test"
is control flow testing without the name or the measurement. What is missing is the measurement: a
coverage tool turns "we believe every branch is tested" into a number that can regress and be
caught. `cargo-llvm-cov` is the natural fit here — it is LLVM source-based coverage, works cleanly
on macOS and Apple Silicon, and needs no extra infrastructure beyond what the project's toolchain
([ADR 0009](decisions/0009-clang-as-assembler-and-linker.md)) already assumes.

### Data Flow Testing

Data flow testing designs test cases from where a variable is defined (assigned) and where it is
used, checking specifically for anomalies: a use with no preceding definition on some path, a
definition that is never used on any path, or two definitions with no use between them. Formal
coverage criteria in this family include all-defs (every definition is followed by at least one use
on some tested path) and all-uses (every definition-to-use pair is exercised).

Two distinct places in this project connect to this technique, and they pull in opposite directions,
which is worth being explicit about rather than glossing over:

- Applied to the _C programs being compiled_, a use-with-no-definition — reading an uninitialized
  variable — is exactly the anomaly this technique targets. This project does not test for it, and
  that is a deliberate design decision, not a gap: uninitialized reads are undefined behavior in C,
  so neither compiler is bound to agree with the other on one, and the differential harness would
  learn nothing from disagreeing. See [ADR 0008](decisions/0008-reject-undefined-behavior.md). Data
  flow testing on the corpus is a technique this project consciously routes around rather than
  adopts.
- Applied to _the compiler's own internals_, the technique fits cleanly but has no counterpart in
  the project's test design. Semantic analysis's annotation side table
  ([ADR 0004](decisions/0004-immutable-ast-with-side-table-annotations.md)) is, structurally, a
  def-use relation: analysis _defines_ a resolved type, a resolved binding, or a frame slot for a
  given `NodeId`, and code generation _uses_ that entry by looking it up. A missing definition is a
  correctness bug of exactly the kind data flow testing is built to catch — the code generator
  asking for an annotation that was never recorded, which without a completeness check fails as a
  panic or (worse) a silent wrong answer rather than a clean diagnostic. See
  [Proposed test suites](#proposed-test-suites) for a concrete "all-defs" check over this relation.

## Experience-based techniques

These rely on a tester's own judgment and pattern recognition — knowing where bugs tend to hide —
rather than a systematic derivation from a spec or the code. The clearest name in this family is
error guessing, and this project already practices it without naming it: the "operand order" tests
in `PLAN.md` Phase 4 exist because the author of the
[`w0`/`w1` operand convention](architecture.md#code-generation) specifically anticipated the bug
class that convention is designed to prevent — a transposed operand that assembles cleanly and is
invisible on symmetric input. A test written because someone can picture exactly how the code would
fail, before it fails, is error guessing in the textbook sense.

Two related ideas from the book are worth naming because they explain _why_ this project's testing
strategy is layered rather than just large:

- The pesticide paradox: a fixed test suite, run unchanged, eventually stops finding new bugs, the
  same way a pest population develops resistance to a fixed pesticide. This is the direct
  justification for `PLAN.md` Phase 5's random program generator and the `cargo-fuzz` targets — both
  exist specifically because the hand-written corpus, however careful, only covers the combinations
  someone thought to write down.
- Defect clustering: defects tend to concentrate in a small fraction of a system rather than spread
  evenly. The project's testing effort is visibly weighted toward exactly the place this project's
  own design documents flag as highest-risk — the binary-operation lowering and its operand-order
  convention (see [ADR 0005](decisions/0005-stack-spilling-instead-of-register-allocation.md)) gets
  named, targeted tests, while lower-risk areas get the standard one-test-per-form treatment.

## Risk-based testing

The book frames test prioritization as a function of risk: likelihood of a defect times the impact
if it occurs. This project makes that trade-off explicitly, in writing, more than once, which makes
it an unusually clean example:

- Correctness risk is treated as unconditionally the highest priority.
  [ADR 0001](decisions/0001-subset-of-c-with-clang-as-oracle.md) exists to make correctness
  externally verifiable at all, and [ADR 0008](decisions/0008-reject-undefined-behavior.md) exists
  specifically to keep the differential comparison meaningful by excluding inputs where a mismatch
  would carry no information.
- Performance risk is explicitly deprioritized.
  [ADR 0005](decisions/0005-stack-spilling-instead-of-register-allocation.md) chooses a strategy
  known in advance to produce slower code, in exchange for removing a defect class (register
  clobbering) that the ADR itself describes as "nearly impossible to localize" once it occurs. That
  is a risk calculation stated in the open, not an accident of scope.
- Portability risk is deprioritized by [ADR 0003](decisions/0003-single-target-arm64-macos.md) for
  the same reason: a second target multiplies the surface that needs testing without changing what
  the project is actually trying to learn.

## Static testing

Static testing checks software without executing it — reviews, walkthroughs, and inspections in the
book's vocabulary, plus tool-driven static analysis. This project has an automated form of it in
place already: `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` run on every push
(see [`AGENTS.md`](https://github.com/sid-ak/rusty_c_compiler/blob/main/AGENTS.md#style)) and catch
a class of problem — style drift, common Rust correctness footguns — before any test executes at
all. `AGENTS.md`'s PR process, which routes every change through review before merge, is the
human-driven counterpart. The ADRs themselves are close to a design-review artifact in the book's
sense: each one records the alternatives that were considered and rejected before code was written,
which is what a design review is trying to produce even when it happens as a live conversation
instead of a document.

## Defect and incident management

The book treats what happens _after_ a test fails — how the failure is captured, triaged, and
verified as fixed — as its own discipline, separate from writing the test that found it. Two pieces
of this project's design are, in substance, an incident-management process, even though neither is
labeled that way:

- A differential mismatch report — the offending program's path, both compilers' outputs, both exit
  codes, and the path to the retained `.s` file (see
  [testing architecture](architecture.md#testing-architecture)) — is a structured incident report
  generated automatically, with exactly the information a human would otherwise have to reconstruct
  by hand before starting to debug.
- `PLAN.md` Phase 5's rule that every fuzz crash is minimized and checked in as a permanent
  regression test is the project's defect lifecycle policy stated as an exit criterion: found,
  minimized, fixed, and never allowed to regress silently.

## Proposed test suites

Two concrete suites, directly answering "what would it look like to build a suite around each of
these techniques specifically," rather than treating them as instances that already happen to exist
inside other suites. Each is scoped to slot into the phase structure `PLAN.md` defines.

### A control-flow coverage tier

A CI step, gated behind `cargo-llvm-cov`, producing branch coverage for `src/parser/`, `src/sema/`,
and `src/codegen/` specifically — the three modules that are structurally chains of decisions over
grammar productions and expression forms, and therefore the ones where "we have a test for every
form" is a claim worth actually measuring rather than asserting. This would not replace any existing
tier; it would sit alongside `cargo test` in CI as a report, the same way `clippy` sits alongside
`fmt`, and would give `PLAN.md`'s phase exit criteria ("every grammar production has at least one
positive test") an actual number to point at instead of a checklist entry.

### An annotation completeness (data flow) tier

A single targeted test that runs semantic analysis over the entire `tests/programs/` corpus and
asserts every AST node that requires a type, a binding, or a frame slot has one recorded in the
annotation side table — an all-defs check over the def-use relation between semantic analysis and
code generation described in
[ADR 0004](decisions/0004-immutable-ast-with-side-table-annotations.md). This is cheap to write (it
is a structural walk, not a new corpus) and directly protects the invariant
[code generation](architecture.md#code-generation) depends on: that the backend never has to reason
about types because analysis already recorded the answer. A missing annotation caught here is a
clean test failure; the same bug caught downstream is a panic or a silently wrong instruction.

## What this project honestly doesn't cover

In the interest of describing what is true rather than what would sound complete: this project's
testing strategy, as designed, does not include performance testing, usability testing, security
testing, a formal risk matrix connecting likelihood and impact to specific priority tiers, or
maintenance-testing concerns like re-running the full suite after an unrelated dependency bump. Each
of these is a deliberate scoping decision consistent with the project's goal — described in
[architecture.md](architecture.md#what-this-compiler-does) — of learning compiler construction on
one platform, not shipping a production toolchain, and none of them contradicts the mapping above.
Where the book's framework and this project's actual priorities disagree, that disagreement is the
design decision, not an omission from this document.
