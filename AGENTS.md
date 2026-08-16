# Rusty C Compiler — Agent Context

A compiler for a subset of C, written in Rust, emitting ARM64 assembly for Apple Silicon macOS. This
file is how to work in this repo. What the system is and why it is shaped this way lives in
[`docs/architecture.md`](docs/architecture.md); what gets built when lives in
[`docs/PLAN.md`](docs/PLAN.md).

## Getting oriented

- Read [`docs/architecture.md`](docs/architecture.md) first — it is the full design and the language
  subset grammar.
- Read [`docs/PLAN.md`](docs/PLAN.md) for the phase order and each phase's exit criteria.
- Read the [GitHub issues](https://github.com/sid-ak/rusty_c_compiler/issues) — work is tracked
  there as five milestones, one per phase, each with an epic issue holding its task checklist.
- Compare against the repo, which is the ground truth. `ls src/` against the directory tree in
  `docs/architecture.md` gauges progress; `git log` reveals where inside a phase the work sits.
- Before starting, fetch the issue being worked on and read it in full, including its epic.

## Governance

Context lives in `AGENTS.md` files, which are tool-neutral — there is no `CLAUDE.md` in this repo. A
module gains its own scoped `AGENTS.md` only when its surface needs explaining beyond what this file
and the architecture cover; agents read the nearest file in the tree, so the closest one wins. Keep
module-specific detail out of this file.

## Decisions (ADRs)

The locked decisions are recorded as ADRs, indexed in [`docs/decisions/`](docs/decisions/index.md).
They are binding and are the source of truth for what they cover — read the relevant record before
changing what it governs, and do not restate its rules here. A decision that turns out to be wrong
is superseded by a new ADR, never edited away.

## Architectural invariants

The invariants in [`docs/architecture.md`](docs/architecture.md) are binding, not advisory. Read the
relevant section before changing what it governs, and do not restate its rules here. The three that
get violated by accident, named so they can be checked in review:

- No pass panics on user input.
- The AST is not mutated after parsing.
- The code generator does no type reasoning.

Changing the accepted language means changing the grammar in `docs/architecture.md` first, then the
passes. A pass that accepts something the documented grammar does not is a defect even if its tests
pass.

## Environment

- Rust stable, pinned in `rust-toolchain.toml`. Do not add a nightly-only dependency to the compiler
  itself; nightly exists here solely for `cargo-fuzz`.
- Xcode Command Line Tools are required, not optional — the driver shells out to `clang` to assemble
  and link. `xcode-select --install` if `clang --version` fails.
- Apple Silicon only. The emitted code is ARM64 Mach-O and the tests execute it, so there is no
  meaningful way to run the suite on another architecture.
- Fuzzing needs `cargo install cargo-fuzz` and a nightly toolchain:
  `rustup toolchain install nightly`.
- The documentation site runs on `uv`: `uv venv && uv pip install -r requirements-docs.txt`, then
  `uv run mkdocs serve`.

## Development

- Tests come first, not alongside. For any issue: write the tests that describe the expected
  behavior and watch them fail, then write the code until they pass. Writing or updating tests for
  code you change is mandatory even when nobody asked.
- Test the error paths as first-class behavior. Every diagnostic you add gets a test that provokes
  it and asserts its message and span, not merely that something failed.
- Assert on structure, not on coincidence. A precedence test asserts the AST shape; a codegen test
  asserts the program's observable behavior. `1+2*3 == 7` would also pass with precedence wrong for
  a compensating reason.
- Rustdoc comments are mandatory on every module, type, function, and test, including test helpers.
  A test's docstring states the behavior it pins. Keep to one line unless the why is non-obvious.
  `#![deny(missing_docs)]` on the crate makes this a build failure rather than a review comment.
- Never reach for `unwrap`, `expect`, `panic!`, or unchecked indexing on a path reachable from user
  input. Return a `Diagnostic`. The `clippy::unwrap_used`, `clippy::expect_used`, `clippy::panic`,
  and `clippy::indexing_slicing` lints are denied in `src/` and allowed in tests.
- Add a new subset-C test program to `tests/programs/` together with its entry in
  `tests/programs/COVERAGE.md`; a program without a matrix entry fails CI.

## Testing

1. `cargo test`: the whole suite — unit, snapshot, golden-program, and differential.
2. `cargo test --test differential`: the clang-oracle suite alone. Each program is its own test, so
   `cargo test --test differential -- arrays` scopes to one area.
3. `cargo test --lib`: the fast in-crate unit tests, no compilation or linking of C.
4. `cargo insta review`: triage snapshot diffs interactively; `cargo insta accept` after verifying
   the change is intended. Never accept a snapshot you have not read.
5. `cargo +nightly fuzz run lex`: one fuzz target; also `parse` and `frontend`. Run at least 15
   minutes per target before calling a front-end phase done.
6. `RUSTYCC_KEEP_TEMPS=1 cargo test --test differential`: retain the generated `.s` for a failing
   program so it can be inspected.

A failing differential test is a compiler bug until proven otherwise. Do not adjust the expectation
or move the program out of the corpus to make the suite green — `clang` is the oracle.

## Style

- `cargo fmt` formats; `cargo clippy --all-targets -- -D warnings` lints. Both must be clean, and
  both run in CI. Do not `#[allow]` a lint without a comment saying why.
- Type-driven over stringly-typed: an enum with a variant per case rather than a string compared at
  the use site. This is most of why the implementation language is Rust.
- Errors are values. The compiler returns `Result` and accumulates diagnostics; it does not abort on
  the first problem.
- Factor on the second occurrence, not the third. Two near-identical blocks — the same setup, guard,
  or error construction — is a defect to fix in the same pass, not a nitpick.

## Documentation

- `docs/` is published as an MkDocs site configured by `mkdocs.yml`. Serve it locally with
  `uv run mkdocs serve`; build it with `uv run mkdocs build --strict`, which fails on a broken
  internal link.
- A new page must be added to the `nav` in `mkdocs.yml` or the strict build fails.
- Diagrams live in `docs/assets/` as SVG, hand-written and committed, so they render on GitHub and
  in the site without a build step.
- Keep the architecture document current with the code in the same change, not afterwards. It is the
  first thing anyone reads, and a stale one is worse than none.
- `README.md` has a "Where it stands today" section that is the project's current-state record.
  Refresh it in the same change as any iteration that moves the project — phase status, what exists
  in the repo, and the build and run instructions once they work. Describe what is true, never what
  is intended; a README claiming a capability the repo does not have is a defect, not a rounding
  error. Keep it to the state itself, without pinning it to a date or a commit hash, so it reads as
  current rather than as a stale timestamp.

## PRs

- Branch from `main`, naming the branch with its GitHub issue number first, e.g. `8-lexer-scanner`
  for issue #8, so the branch links back to its issue.
- PR title format: `[<module>] <Title>` — e.g. `[sema] Add scope stack and symbol table`.
- Run the full gate green before handing off:
  `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && uv run mkdocs build --strict`.
- Close the issue from the PR body with `Closes #N`, and tick the box on the phase epic.
- Never commit; only stage. Do not run `git commit`. At the end of a set of changes, `git add` the
  relevant files and leave them staged for the human to review — but only if nothing is currently
  staged. If anything is already staged, do not stage at all; leave the working tree as-is.
