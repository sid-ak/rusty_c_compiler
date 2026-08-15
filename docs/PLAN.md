# Implementation Plan — Rusty C Compiler

This plan turns [PROPOSAL.md](PROPOSAL.md) into five sequential, independently verifiable phases.
Each phase ends in a demonstrable artifact and a set of tests that must pass before the next phase
starts. Nothing in a later phase is required to validate an earlier one.

The design being built is in [architecture.md](architecture.md) — the pipeline, the language subset
grammar, the code generation strategy, and the testing architecture all live there. This document is
only the order of work and the gate at each step.

## Table of contents

- [Definition of done](#definition-of-done)
- [Phase 1 — Foundation, diagnostics, and lexer](#phase-1--foundation-diagnostics-and-lexer)
- [Phase 2 — AST and recursive-descent parser](#phase-2--ast-and-recursive-descent-parser)
- [Phase 3 — Semantic analysis](#phase-3--semantic-analysis)
- [Phase 4 — ARM64 code generation and the driver](#phase-4--arm64-code-generation-and-the-driver)
- [Phase 5 — Differential testing, fuzzing, and system acceptance](#phase-5--differential-testing-fuzzing-and-system-acceptance)
- [Dependency graph](#dependency-graph)
- [Traceability to the proposal](#traceability-to-the-proposal)
- [Issue map](#issue-map)

## Definition of done

A phase is complete when its own exit criteria below are met and the repo-wide gates in
[`AGENTS.md`](https://github.com/sid-ak/rusty_c_compiler/blob/main/AGENTS.md) are green on an Apple
Silicon CI run. The test tiers referenced
throughout — unit, snapshot, golden-program, differential — are defined in
[architecture.md](architecture.md#test-tiers). The architectural invariants each phase must preserve,
including the no-panic rule and the immutable AST, are stated in
[architecture.md](architecture.md#the-pipeline).

## Phase 1 — Foundation, diagnostics, and lexer

Goal: a working Cargo project that can turn a source file into a token stream, with a diagnostic
system good enough to serve every later phase, plus the CI and runtime-shim infrastructure the whole
project depends on.

### Deliverables

1. Cargo project `mycc` with a `lib.rs` + thin `main.rs` split, so integration tests can call the
   compiler as a library rather than shelling out.
2. `src/diagnostics.rs`:
    - `Span { start: usize, end: usize }` over byte offsets into the source.
    - A `SourceMap` that converts a byte offset to 1-based line and column, computed once by
      precomputing line-start offsets.
    - `Diagnostic { kind, message, span, notes }` and a renderer producing
      `file.c:LINE:COL: error: MESSAGE` followed by the offending source line and a caret.
    - `DiagnosticBag` that accumulates multiple diagnostics so a pass can report more than one error.
3. `src/lexer/token.rs`: `TokenKind` enum covering identifiers, the keyword set (`int`, `char`,
   `void`, `if`, `else`, `while`, `for`, `return`, `break`, `continue`), integer literals, character
   literals, string literals, every operator and punctuator in the grammar, and `Eof`. `Token`
   pairs a `TokenKind` with a `Span`.
4. `src/lexer/mod.rs`: a hand-written scanner over `&[u8]`, producing `Vec<Token>` plus any
   diagnostics. It never panics and always terminates, including on unterminated constructs and on
   non-UTF-8 bytes.
5. `runtime/shim.c` implementing `void print_int(int)`, `void print_char(char)`, and
   `void print_string(char *)` using only `write(2)`, with `print_string` looping over `print_char`
   to the null terminator.
6. `.github/workflows/ci.yml` running fmt, clippy, and test on `macos-14`.

### Lexer details worth pinning now

- Integer literals: decimal, hex (`0x`), and octal (leading `0`), fitting in `i32`; overflow is a
  diagnostic, not a wrap.
- Character literals: single character or an escape from `\n \t \r \0 \\ \' \" \a \b \f \v`; empty,
  multi-character, or unterminated literals are diagnostics.
- String literals: same escape set, unterminated is a diagnostic; the lexer stores the decoded bytes,
  not the raw source text, so the code generator does not re-parse escapes.
- Comments: both `//` and `/* */`; an unterminated block comment is a diagnostic.
- Maximal munch for multi-character operators, so `<=`, `==`, `&&`, `++` are never mis-split.
- Longest-match keyword recognition happens after identifier scanning, so `integer` lexes as one
  identifier and not `int` + `eger`.

### Tests

- Unit test per token category: one test per keyword group, identifiers, each literal form, each
  operator, each punctuator.
- Span correctness: for a multi-line fixture, assert the line and column of every token.
- Maximal munch: `a<=b`, `a<-b`, `a++ +b`, `a+++b` each produce the documented token sequence.
- Error cases, each asserting a specific diagnostic and that lexing still reaches `Eof`:
  unterminated string, unterminated char, empty char literal, multi-char literal, unknown escape,
  unterminated block comment, integer overflow, stray `@`/`$`/`#`, lone `&` and lone `|`.
- Whitespace and comment placement: comment between a token and its operator, comment at EOF without
  a trailing newline, CRLF line endings, tabs affecting column numbers.
- Snapshot test of the full token stream for a representative program.
- Shim test: compile `runtime/shim.c` with `clang -c`, link it with a tiny C `main` that calls all
  three functions, run it, assert stdout.

### Pragmatic Testing

See [`PRAGMATIC_TESTING.md`](PRAGMATIC_TESTING.md) for the full concept catalog; the pieces below
apply directly to this phase.

- Unit level: one test per token category is this project's unit level in the book's
  [test levels](PRAGMATIC_TESTING.md#test-levels) sense.
- Equivalence partitioning and boundary value analysis: the maximal-munch cases above and integer
  overflow are boundary cases on the token space, the same discipline
  [that section](PRAGMATIC_TESTING.md#equivalence-partitioning-and-boundary-value-analysis) applies
  to arithmetic later.
- This phase's requirement that the lexer never panics and always terminates is the no-panic
  invariant Phase 5's fuzzing later holds the whole front end to — see the pesticide paradox in
  [experience-based techniques](PRAGMATIC_TESTING.md#experience-based-techniques) for why fuzzing
  still matters after full unit coverage here.
- The scanner's maximal-munch and keyword-recognition logic is the same match-arm-heavy shape the
  [control-flow coverage tier](PRAGMATIC_TESTING.md#proposed-test-suites) targets — worth including
  `src/lexer/` alongside the modules already named there, if that tier gets adopted.

### Exit criteria

- `cargo test` green; a fixture file of every lexable construct round-trips to the expected token
  stream with correct spans.
- `mycc --dump-tokens program.c` prints a token stream for a hand-written sample and prints a
  rendered diagnostic with a caret for a deliberately broken sample.
- CI green on `macos-14`.

## Phase 2 — AST and recursive-descent parser

Goal: token stream to AST for the entire grammar, with C-correct precedence and associativity and
graceful, recovering error reporting.

### Deliverables

1. `src/ast.rs`: the node types. Every node carries a `Span`. Shape:
    - `Program { items: Vec<Item> }`, `Item = FuncDef | FuncDecl | GlobalVar`.
    - `Stmt = Block | If | While | For | Return | Break | Continue | ExprStmt | LocalVar | Empty`.
    - `Expr = IntLit | CharLit | StringLit | Ident | Unary | Binary | Assign | Index | Call |
      PostfixIncDec`.
    - `TypeSpec { base: Int | Char | Void, array_len: Option<u32>, is_unsized_array: bool }`.
    - Nodes are plain data with no resolved-type field; Phase 3 attaches types via a side table keyed
      by node id, so the AST stays immutable and snapshot-stable.
2. `src/parser/mod.rs`: recursive descent over declarations and statements.
3. `src/parser/expr.rs`: precedence climbing for binary operators, right-associative handling for
   assignment and unary, and a postfix loop for `[]`, `()`, `++`, `--`.
4. Error recovery: on a syntax error, record a diagnostic and skip tokens to the next `;` or `}` at
   the current brace depth, then continue. One bad statement yields one error, not a cascade, and
   never an infinite loop — every recovery step is proven to consume at least one token.
5. A deterministic AST pretty-printer (an S-expression dump) used for snapshot tests and exposed as
   `mycc --dump-ast`.
6. Targeted diagnostics for the constructs listed as out of scope in
   [architecture.md](architecture.md#out-of-scope): seeing `struct`, `switch`, `#include`,
   `*`-as-declarator, `?:`, or `+=` produces "unsupported in this C subset: X" rather than a generic
   parse error.

### Tests

- Precedence and associativity, asserted on the AST dump, not on evaluation:
  `1+2*3`, `1*2+3`, `1-2-3`, `a=b=c`, `-x*y`, `!a&&b`, `a||b&&c`, `a<b==c<d`, `a[i]+1`, `f(x)*2`,
  `-f(x)[i]`, `a[i][` (error), `1+ +2`, `1++ +2`.
- One test per statement form, and per nesting combination that has historically bitten
  recursive-descent parsers: dangling `else` binds to the nearest `if`; `for` with each of the eight
  combinations of present/absent init, condition, and step; empty block; block-in-block; single-
  statement bodies without braces.
- Declaration forms: global scalar with and without initializer, global array with size, array with
  brace initializer, array parameter `int a[]`, `void` parameter list, forward declaration followed
  by definition, function with zero through nine parameters (nine crosses the eight-register ABI
  boundary that matters in Phase 4).
- Error and recovery tests, each asserting the diagnostic and that parsing continues:
  missing semicolon, unbalanced parenthesis, unbalanced brace at EOF, `if` with no condition,
  keyword used as identifier, garbage token mid-expression, two errors in one function producing
  exactly two diagnostics.
- No-panic property test: a small corpus of truncated prefixes of each valid fixture (truncate at
  every token boundary) is parsed; none may panic. This is a cheap precursor to Phase 5 fuzzing.
- Snapshot tests of the full AST dump for a representative program per feature area.

### Pragmatic Testing

See [`PRAGMATIC_TESTING.md`](PRAGMATIC_TESTING.md) for the full concept catalog; the pieces below
apply directly to this phase.

- Control flow testing: "one test per statement form" and "every grammar production has at least one
  positive test" above is
  [control flow testing](PRAGMATIC_TESTING.md#control-flow-testing) without the name — that section
  names this exact phase.
- Equivalence partitioning and boundary value analysis: zero-through-nine-parameter functions are a
  boundary case on the calling convention this phase sets up and Phase 4 crosses — see
  [that section](PRAGMATIC_TESTING.md#equivalence-partitioning-and-boundary-value-analysis).
- The truncation-corpus no-panic property test above is a cheap, manual precursor to Phase 5's
  `parse` fuzz target — see the pesticide paradox in
  [experience-based techniques](PRAGMATIC_TESTING.md#experience-based-techniques).

### Exit criteria

- `mycc --dump-ast` produces a correct dump for every program in `tests/programs/`.
- Every grammar production has at least one positive test and, where it can fail, one negative test.
- Truncation corpus parses without a single panic.

## Phase 3 — Semantic analysis

Goal: reject every program the code generator cannot correctly compile, and annotate the AST with
everything the code generator needs so that Phase 4 contains no type reasoning.

### Deliverables

1. `src/sema/types.rs`: `Ty = Int | Char | Void | Array(Box<Ty>, u32) | Ptr(Box<Ty>) |
   Func { ret, params }`, plus the promotion and compatibility rules from
   [architecture.md](architecture.md#semantics-that-cross-pass-boundaries).
2. `src/sema/scope.rs`: a scope stack. Globals at depth 0; each function body, each block, and each
   `for` init form a new scope. Shadowing an outer-scope name is legal; redeclaring in the same
   scope is an error.
3. `src/sema/mod.rs`, a single pass in two sub-passes:
    - Pass A collects all global declarations and function signatures, so calls to functions defined
      later in the file resolve.
    - Pass B walks each function body resolving identifiers, checking types, and recording results.
4. Analysis output, consumed by Phase 4:
    - Resolved type for every expression node.
    - Resolved binding for every identifier: global, parameter index, or local slot id.
    - Per-function local frame inventory: every local and parameter with its size and alignment, which
      Phase 4 turns into stack offsets.
    - Interned string-literal table mapping each distinct literal to a label.
    - Implicit `char`-to-`int` promotions made explicit as inserted cast nodes, so codegen never has
      to infer a widening.
5. Checks, each with a dedicated diagnostic:
    - Undeclared identifier; undeclared function; use before declaration in the same scope.
    - Redeclaration in the same scope; conflicting redeclaration of a function signature; a definition
      disagreeing with an earlier declaration.
    - Call arity mismatch; call argument type mismatch; calling a non-function; using a function name
      as a value.
    - `return` with a value in a `void` function; bare `return` in a non-`void` function; a non-`void`
      function whose control flow can reach the closing brace (`main` excepted).
    - Indexing a non-array and non-pointer; non-integer subscript; array used in arithmetic (only
      decay-at-call and indexing are legal).
    - Assigning to a non-lvalue, to an array name, or to a function.
    - `void` used as a variable or parameter type; array of `void`; zero-length or negative array size.
    - Array initializer longer than the declared size; non-constant global initializer.
    - `break` or `continue` outside a loop.
    - Multiple definitions of the same function.
6. Multiple errors are reported in source order in one run; analysis does not stop at the first.

### Tests

- One positive test per rule showing the legal form is accepted, one negative test per bullet above
  showing the exact diagnostic. These are table-driven from a single fixture list so the pattern is
  written once.
- Scope resolution: shadowing in a nested block resolves to the inner binding, and the outer binding
  is visible again after the block; `for`-init variable is not visible after the loop; a parameter is
  shadowable by a body local; a global is shadowable by a local.
- Signature checking: forward declaration then matching definition passes; then mismatching return
  type, mismatching arity, and mismatching parameter type each fail.
- Type annotation correctness: snapshot the annotated AST for a program mixing `char` and `int`, and
  assert the inserted promotion nodes appear exactly where the rules say.
- Decay: `int a[10]; f(a);` types the argument as pointer-to-int; `a + 1` at statement level is
  rejected; `a[i]` types as `int`.
- Frame inventory: for a function with mixed `char`, `int`, and array locals across nested blocks,
  assert the reported inventory has the expected count, sizes, and alignments.
- Multi-error: a program with four distinct errors yields exactly four diagnostics in source order.

### Pragmatic Testing

See [`PRAGMATIC_TESTING.md`](PRAGMATIC_TESTING.md) for the full concept catalog; the pieces below
apply directly to this phase.

- Decision table testing: the check list above is tested one rule at a time; a decision table is the
  right tool once interacting rules need checking together (for example, indexing a non-array with a
  non-integer subscript at once) — see
  [decision table testing](PRAGMATIC_TESTING.md#decision-table-testing), which names this exact
  phase.
- State transition testing: the scope stack (globals, function body, block, `for`-init) is a genuine
  state machine — the scope-resolution tests above are already state-transition tests in substance;
  see [state transition testing](PRAGMATIC_TESTING.md#state-transition-testing) for the transitions
  worth checking explicitly.
- Data flow testing: the annotation side table this phase writes (types, bindings, frame slots) is a
  def-use relation with code generation as the consumer. An all-defs completeness check over it —
  every node code generation looks up was actually recorded here — is a concrete, cheap addition once
  this phase lands; see [data flow testing](PRAGMATIC_TESTING.md#data-flow-testing) and the
  [annotation completeness suite](PRAGMATIC_TESTING.md#proposed-test-suites).
- Uninitialized reads are deliberately not one of the checks above; the corpus excludes them by
  construction instead, since they are undefined behavior — see the corpus-side half of
  [data flow testing](PRAGMATIC_TESTING.md#data-flow-testing).

### Exit criteria

- Every check listed above has a passing positive and negative test.
- `mycc --check program.c` exits 0 for every valid program in `tests/programs/` and non-zero with an
  accurate message for every file in `tests/programs/invalid/`.
- Cross-check on the invalid corpus: for each rejected program, `clang -O0 -std=c99` also rejects it,
  or the deviation is explicitly recorded as one of the intentional restrictions in an
  [ADR](decisions/index.md).

## Phase 4 — ARM64 code generation and the driver

Goal: annotated AST to a native macOS executable, using the stack-spill strategy described in
[architecture.md](architecture.md#code-generation).

### Deliverables

1. `src/codegen/emit.rs`: assembly text buffer, section management, unique label generation, and
   symbol naming (Mach-O leading underscore).
2. `src/codegen/frame.rs`: per-function stack layout. Prologue saves `x29`/`x30` with
   `stp x29, x30, [sp, #-N]!`, sets `x29`, and reserves slots; frame size is rounded to a 16-byte
   multiple; every local, parameter spill, and temporary gets a fixed `x29`-relative offset. Epilogue
   restores and returns via `ret`.
3. `src/codegen/expr.rs`: expression lowering. Every expression evaluates into `w0`/`x0`; binary
   operators evaluate the left side, spill it to its temp slot, evaluate the right side, `mov` it
   into `w1`, and reload the left side into `w0`, so every instruction reads left in `w0` and right
   in `w1` — the operand convention in
   [architecture.md](architecture.md#code-generation). Covers:
    - Integer and character literals, including `movz`/`movk` sequences for constants that do not fit
      an immediate field.
    - Loads and stores: `ldr`/`str` for `int`, `ldrsb`/`strb` for `char`.
    - `add`, `sub`, `mul`, `sdiv`, and `msub`-based remainder.
    - Comparisons via `cmp` + `cset` with the right condition code.
    - `&&` and `||` lowered to branches so short-circuiting is real, not simulated.
    - Unary minus, logical not, unary plus, prefix and postfix `++`/`--`.
    - Array indexing: base address plus index scaled by element size.
    - Address-of-array-for-decay at call sites only.
    - String literals as `adrp` + `add` against a `__TEXT,__cstring` label.
4. `src/codegen/stmt.rs`: statement lowering — `if`/`else`, `while`, `for`, `return`, `break`,
   `continue` (with a loop-context stack of break/continue labels), blocks, and expression
   statements.
5. Calling convention: integer and pointer arguments in `x0`–`x7`; ninth and later arguments passed
   on the stack with the alignment AAPCS64 requires; `char` arguments promoted to `int`; return value
   in `w0`/`x0`; stack pointer kept 16-byte aligned at every call.
6. Globals: initialized scalars and arrays in `__DATA,__data`, uninitialized in `__bss`, with correct
   `.p2align` and size directives.
7. `src/driver.rs`: write the `.s` file to a temp directory, invoke `clang -c` to assemble and
   `clang` to link — including `runtime/shim.c`'s object — and clean up intermediates. Flags:
   `-o <out>`, `-S` to stop after assembly, `--emit-asm-to <path>`, `-c`, and `--keep-temps`.
   Toolchain invocation failures surface the child process's stderr rather than a bare exit code.
8. `mycc program.c -o program` works end to end, matching the proposal's CLI contract.

### Tests

- Assembly snapshot tests per construct, so a regression shows up as a readable diff rather than a
  wrong number: one snapshot each for arithmetic, comparison, short-circuit, `if`, `while`, `for`,
  call, array index, global access, string literal.
- Assemble-cleanly tests: every emitted `.s` in the snapshot set is fed to `clang -c` and must
  assemble with no warnings — this catches malformed directives that a snapshot alone would not.
- Execution tests, the main body of the phase: each golden program in `tests/programs/` is compiled
  by `mycc`, run, and checked against a recorded exit code and stdout. Coverage must include:
  - Arithmetic: precedence, integer division and remainder sign behavior, unary minus, large
    constants near `i32::MIN`/`i32::MAX`.
  - Operand order: every non-commutative operator exercised with asymmetric operands — `10 - 3` is
    `7` and not `-7`, `10 / 3` is `3`, `10 % 3` is `1`, and both `1 < 2` and `2 < 1` are checked. A
    transposed lowering passes on symmetric operands, so symmetric cases prove nothing here.
  - Control flow: nested `if`/`else`, `while` with `break` and `continue`, all `for` variants,
    deeply nested loops.
  - Functions: zero through nine arguments (crossing the register/stack boundary), recursion
    (factorial, fibonacci, Ackermann at a small bound), mutual recursion, forward-declared calls,
    `void` functions.
  - Arrays: local and global, initialization, iteration, sum/reverse/sort over an array, array passed
    to a function and mutated in place.
  - `char`: storage, promotion in arithmetic, comparison against literals, `char` arrays as strings.
  - Short-circuit: an `&&` whose right operand has a side effect that must not happen.
  - Interaction: a program combining recursion, arrays, and `print_string`.
- Frame and ABI stress: a function with many locals forcing a frame larger than the immediate-offset
  range of `ldr`/`str`, verifying the offset-materialization path.
- Driver tests: `-o` respected; `-S` emits assembly and no binary; missing input file, unreadable
  input file, and a failing link each produce a clear message and a non-zero exit; temp files are
  removed unless `--keep-temps`.

### Pragmatic Testing

See [`PRAGMATIC_TESTING.md`](PRAGMATIC_TESTING.md) for the full concept catalog; the pieces below
apply directly to this phase.

- Error guessing: the operand-order tests above exist because the `w0`/`w1` convention's failure mode
  was anticipated before it happened, not discovered after — see
  [experience-based techniques](PRAGMATIC_TESTING.md#experience-based-techniques), which names this
  exact phase.
- Equivalence partitioning and boundary value analysis: `i32::MIN`/`i32::MAX` constants and the
  nine-argument register/stack boundary are the two boundary cases the tests above are built around —
  see
  [that section](PRAGMATIC_TESTING.md#equivalence-partitioning-and-boundary-value-analysis).
- State transition testing: the loop-context stack (`break`/`continue` legality changing with loop
  nesting) is a state machine worth testing on its transitions, not only its steady states — see
  [state transition testing](PRAGMATIC_TESTING.md#state-transition-testing).
- Risk-based testing: the performance-vs-correctness trade-off behind stack spilling
  ([ADR 0005](decisions/0005-stack-spilling-instead-of-register-allocation.md)) is this phase's own
  risk call, stated in the open — see [risk-based testing](PRAGMATIC_TESTING.md#risk-based-testing).
- Control flow testing: `src/codegen/`'s lowering is the same match-arm-per-expression-form shape the
  [control-flow coverage tier](PRAGMATIC_TESTING.md#proposed-test-suites) targets.

### Exit criteria

- Every golden program compiles, links, runs, and matches its recorded exit code and stdout.
- The emitted assembly for every golden program assembles under `clang -c` without warnings.
- `mycc hello.c -o hello && ./hello` demonstrably works from a clean checkout.

## Phase 5 — Differential testing, fuzzing, and system acceptance

Goal: replace recorded expectations with `clang` as the oracle, prove robustness against adversarial
input, and satisfy the proposal's system-level acceptance criterion.

### Deliverables

1. `tests/differential.rs` — the harness described in
   [architecture.md](architecture.md#differential-testing-against-clang):
    - Discovers every `.c` file under `tests/programs/`.
    - Builds each one twice: once via `mycc`, once via `clang -O0 -std=c99 -Wall`, both linked against
      the same `runtime/shim.o`.
    - Runs both binaries with identical argv, empty stdin, and a wall-clock timeout.
    - Compares stdout byte for byte, compares stderr, and compares exit status (masked to the low 8
      bits, with signal-death distinguished from normal exit).
    - On mismatch, reports the program path, both outputs, both exit codes, and the path to the
      retained `.s` so the failure is debuggable without re-running by hand.
    - Runs as one `#[test]` per program via a generated test list, so a single failure names the
      offending program rather than collapsing the suite into one red line.
2. A test-corpus expansion pass driven by feature-coverage review: every feature in the grammar
   appears in at least three differential programs, and every pair of features that can interact
   (recursion + arrays, `char` + promotion + comparison, arrays + function boundary + array
   mutation, short-circuit + side effects, nested loops + `break`/`continue`) appears in at least
   one. Target: 60+ programs.
3. A generator-based differential mode: a small random program generator emitting well-typed subset-C
   restricted to defined behavior — no division by zero, no signed overflow, no out-of-bounds
   indexing, no uninitialized reads — feeding the same compare-against-clang harness. Seeded and
   reproducible; a failing seed is checkable in as a fixture. This is what finds the codegen bugs a
   hand-written corpus misses.
4. `fuzz/` with `cargo-fuzz` targets:
    - `lex`: raw bytes to the lexer; must never panic and must always terminate.
    - `parse`: raw bytes through lexer and parser; must never panic.
    - A third target over the full front end through semantic analysis, since Phase 3 introduces its
      own indexing and recursion.
    - Recursion-depth guard: deeply nested parentheses or blocks must produce a "nesting too deep"
      diagnostic rather than a stack overflow. Fuzzing will find this, so it is planned for.
    - A seed corpus built from `tests/programs/`, and a documented minimum run (15 minutes per target
      locally, plus a scheduled longer CI run). Any crash found is minimized and checked in as a
      regression test.
5. The final system test the proposal calls for: a single `cargo test --test differential` run over
   the curated suite covering every supported feature, all green, which is the acceptance criterion
   for functional completeness.
6. Documentation: a `README.md` covering build, usage, the supported subset, the architecture, and
   how to run each tier of tests; plus a short write-up of the known deviations from C, if any
   survive.

### Tests

The harness is itself test code, so it needs its own verification:

- Harness self-tests: a deliberately wrong compiler output must make the harness fail — inject a
  program where `mycc` output is stubbed as wrong and assert the harness reports a mismatch. A
  harness that cannot fail proves nothing.
- Timeout path: a program with an intentional infinite loop is killed and reported as a timeout, not
  a hang.
- Exit-code masking: a program returning `300` from `main` compares equal under both compilers at
  `300 & 0xFF`.
- Fuzz regression tests: every crash the fuzzer found is a permanent unit test.

### Pragmatic Testing

See [`PRAGMATIC_TESTING.md`](PRAGMATIC_TESTING.md) for the full concept catalog; the pieces below
apply directly to this phase.

- Test levels: this phase is where testing moves to the system and acceptance levels — the
  differential suite going green is the project's literal, stated definition of "done" — see
  [test levels](PRAGMATIC_TESTING.md#test-levels).
- Pesticide paradox: the random program generator and `cargo-fuzz` targets above exist specifically
  because the hand-written corpus alone plateaus — see
  [experience-based techniques](PRAGMATIC_TESTING.md#experience-based-techniques), which names this
  exact phase.
- Defect and incident management: a differential mismatch report and the fuzz-crash-becomes-
  permanent-regression-test rule above are this phase's incident-management process, made concrete —
  see
  [defect and incident management](PRAGMATIC_TESTING.md#defect-and-incident-management).
- This phase, where CI and fuzz infrastructure matures, is also the natural place to actually adopt
  the [proposed test suites](PRAGMATIC_TESTING.md#proposed-test-suites) (structural coverage,
  annotation completeness) as CI checks rather than leaving them as proposals.

### Exit criteria

- 100% of the curated corpus passes differentially against `clang -O0`; zero known mismatches.
- All three fuzz targets run the documented minimum with zero crashes from a clean corpus.
- CI runs fmt, clippy, unit tests, and the full differential suite on every push, green.
- `README.md` lets someone with a clean Apple Silicon Mac and Xcode Command Line Tools build the
  compiler and run all tests from the instructions alone.

## Dependency graph

```
Phase 1 (diagnostics, lexer, shim, CI)
   └─> Phase 2 (AST, parser)
          └─> Phase 3 (types, scopes, checks)
                 └─> Phase 4 (ARM64 codegen, driver)
                        └─> Phase 5 (differential, fuzz, acceptance)
```

Two things can start early and are worth doing so:

- The `tests/programs/` corpus can be written from Phase 1 onward — it is just C, and it is exactly
  what `clang` will validate. Writing programs before the compiler can run them keeps the corpus
  honest.
- The fuzz targets from Phase 5 can be pointed at the lexer as soon as Phase 1 lands and at the
  parser as soon as Phase 2 lands, rather than waiting for Phase 5.

## Traceability to the proposal

| Proposal section | Phase |
| --- | --- |
| Lexer | 1 |
| Parser | 2 |
| Language subset | 2 (syntax), 3 (semantics), 4 (lowering) |
| Semantic analysis | 3 |
| Code generation | 4 |
| Command-line driver | 1 (skeleton), 4 (full) |
| Lexer / parser / sema / codegen module tests | 1 / 2 / 3 / 4 |
| Differential testing against clang | 5 |
| Fuzz testing | 5 |
| Final system test | 5 |
| Runtime shim (`print_int`/`print_char`/`print_string`) | 1 (built), 4 (linked) |
| Array decay at function boundary | 3 (rule), 4 (lowering) |
| Stack-spill codegen strategy | 4 |
| String literals in `__TEXT,__cstring` | 1 (lexing), 3 (interning), 4 (emission) |

## Issue map

Work is tracked in GitHub issues under one milestone per phase. Each phase has an epic issue holding
a checklist of its tasks.

| Phase | Epic | Tasks |
| --- | --- | --- |
| 1 — Foundation, diagnostics, and lexer | [#1](https://github.com/sid-ak/rusty_c_compiler/issues/1) | #6, #11, #7, #8, #9, #12, #10 |
| 2 — AST and parser | [#2](https://github.com/sid-ak/rusty_c_compiler/issues/2) | #13, #14, #15, #16, #17, #18 |
| 3 — Semantic analysis | [#3](https://github.com/sid-ak/rusty_c_compiler/issues/3) | #19, #20, #21, #22, #23, #24 |
| 4 — Code generation and driver | [#4](https://github.com/sid-ak/rusty_c_compiler/issues/4) | #25, #26, #27, #28, #29, #30, #31, #32 |
| 5 — Differential testing and acceptance | [#5](https://github.com/sid-ak/rusty_c_compiler/issues/5) | #33, #34, #35, #36, #37, #38, #39 |
