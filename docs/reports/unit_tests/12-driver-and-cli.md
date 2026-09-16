# Unit Test Report — Compiler Driver / CLI

## Unit

Source under test: `src/cli.rs` (`Options`, the `clap`-derived argument parser, and `Stage`, the
pipeline-stage enum the debug flags select), `src/lib.rs` (`compile()` — the in-process compiler
entry point that touches no files and spawns no processes — `run()` — the file-I/O and
diagnostic-rendering wrapper around it — and `Error`, the invocation-level error type), and
`src/main.rs` (the `rustycc` binary's `main`, which is intentionally minimal: parse argv, call
`run`, map the result to an exit code). Also `tests/cli.rs`, the black-box integration suite that
drives the compiled binary as a real subprocess. This unit is the seam between "the compiler as a
library" and "the compiler as a command a user runs," and its tests are specifically about that
seam — argument parsing correctness, the file-I/O boundary, and the binary's process-level contract
(exit codes, stderr) — not about compilation logic itself, which belongs to the lexer/parser units.

## Date

2026-08-23

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

**Approach: white-box unit testing of the argument-parsing and dispatch logic (equivalence
partitioning over CLI flags, plus a self-check of the `clap` derive macro's structural validity),
paired with black-box integration testing of the actual compiled binary as an external process —
deliberately kept as two separate test tiers because they validate two different contracts: what
`Options`/`compile`/`run` do as Rust functions, versus what `rustycc` does as a program a user
invokes from a shell.**

1. **Structural self-validation of the `clap` derive.** `command_definition_is_valid` calls
   `Options::command().debug_assert()`, which is `clap`'s own mechanism for catching a malformed
   `#[arg]` attribute (conflicting short flags, an invalid group, etc.) at test time rather than
   only at first real invocation. This is included because a CLI definition is exactly the kind of
   code that "looks right" in a diff but is only checked by the framework at the moment it's
   actually parsed — putting that check in the test suite turns a runtime-only failure mode into a
   `cargo test` failure.

2. **Equivalence partitioning over the mutually-exclusive debug flags.**
   `each_debug_flag_selects_its_stage` sweeps all 4 explicit stage flags (`--dump-tokens`,
   `--dump-ast`, `--check`, `-S`) individually against the `Stage` each should select, and
   `default_stage_is_an_executable` covers the implicit 5th partition (no flag at all →
   `Stage::Executable`) — together these are a complete partition of the "which stage does this
   invocation stop at" input space. `debug_flags_conflict_with_each_other` is the boundary/negative
   case: two stage flags together must be a usage error, not silently pick one — this directly
   exercises the `group = "stop_after"` constraint declared in the `clap` attributes, proving the
   declared mutual exclusion is actually enforced by the derived parser rather than merely intended.

3. **Contract testing between the two ways an `Options` value can be constructed.**
   `for_source_matches_the_parsed_equivalent` asserts that `Options::for_source` (the constructor
   library callers use, e.g. from tests in other units) produces a value equivalent — same `input`,
   same resolved `stage()` — to parsing the corresponding argv. This is a consistency test across an
   API surface with two entry points that must never disagree, which is exactly the kind of drift
   that is invisible from either constructor's own unit tests alone and only shows up when both are
   compared directly.

4. **Required-field and default-value testing.** `input_file_is_required`
   (equivalence partition: no positional argument is a hard usage error, not a silent no-op — this
   matters because a compiler that silently does nothing on a bad invocation is a much worse failure
   mode than a startup crash) and `keep_temps_defaults_off` (boundary test on a boolean flag's
   default state, both unset and explicitly set) round out the `Options` coverage.

5. **Equivalence partitioning over `compile`/`run`'s outcome space** in `src/lib.rs`:
   `empty_source_compiles` (the trivial-but-real case — an empty translation unit is valid C, so this
   also indirectly guards against an off-by-one that would reject empty input) and
   `missing_input_is_a_read_error` (the file-I/O failure partition, asserted to come back as a typed
   `Error::Read` naming the exact path — not a panic, and not an undifferentiated I/O error) are the
   two outcome classes `run()` can produce independent of the C program's own correctness.
   `read_error_message_names_the_path` and `rejection_message_agrees_in_number` are `Display`-impl
   tests for `Error`, each targeting a specific, easy-to-regress detail: that the path appears
   verbatim in a read error, and that the "error"/"errors" pluralization agrees with the actual
   count at both the singular and plural boundary (1 vs. 3) — a classic off-by-one/pluralization
   boundary test.

6. **Black-box integration testing of the real binary** (`tests/cli.rs`), which is the only tier in
   this unit — or arguably in the whole project so far — that spawns `rustycc` as an actual
   subprocess (via `Command::new(RUSTYCC)`, using Cargo's `CARGO_BIN_EXE_rustycc` to find the freshly
   built binary) rather than calling library functions directly. This is a deliberate methodology
   choice, not an oversight: it is the only way to verify the *process-level* contract — exit code,
   what goes to stdout vs. stderr, and the literal text a user would see — none of which is
   observable by calling `run()` in-process. `no_arguments_prints_usage_and_fails` and
   `missing_input_file_reports_a_readable_error` are both boundary/negative-space tests of this
   contract (no exit-0 on a bad invocation; the word "panicked" must never appear, which is a direct,
   automated check of the "no pass panics" architectural invariant at the process boundary — a
   stronger and more literal check than mere structural absence of `.unwrap()` calls could give).
   `compiler_is_callable_in_process` is the converse assertion, proving the *library* entry point
   works with zero file-system access and zero child processes — the property `src/lib.rs`'s own doc
   comment states is the reason `compile()` exists separately from `run()` — so this single test
   validates a specific architectural claim made in the source, not just generic behavior.

**Coverage assessment.** All 5 `Stage` values are reachable and individually tested. Both
`Options` construction paths (`clap` parsing and `for_source`) are tested and cross-checked for
agreement. Both outcome branches of `run()` (successful compile-and-print, and each `Error` variant)
are covered, including their `Display` output at a pluralization boundary. The binary's process-level
behavior (exit code, stderr content, the no-panic guarantee) is covered by real subprocess execution,
not simulated. What is out of scope here: this unit does not test `compile()`'s behavior for stages
past `Ast` (semantic analysis, code generation, or assembling/linking to a real executable), because
those stages do not exist in the compiler yet — `compile()`'s current implementation falls through to
`Ok(Artifacts::default())` for any stage past `Ast`, which is exercised incidentally by
`parses_the_documented_invocation`/`default_stage_is_an_executable` selecting `Stage::Executable`,
but there is intentionally no test asserting executable-producing behavior, since none exists to
test yet; this will need new tests once Phases 3–4 land.

## Automated Test Code

15 tests total: 8 in `src/cli.rs`, 4 in `src/lib.rs`, 3 in `tests/cli.rs` (`src/main.rs` itself has
no `#[cfg(test)]` module — by design, per its own doc comment, "everything else lives in the
library so integration tests can drive the compiler in process").

### `src/cli.rs` (8 tests)

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 1 | `command_definition_is_valid` | `Options::command().debug_assert()` | No panic (clap's own structural check passes) |
| 2 | `parses_the_documented_invocation` | argv `["program.c", "-o", "program"]` | `.input=="program.c"`, `.output==Some("program")`, `.stage()==Executable` |
| 3 | `default_stage_is_an_executable` | argv `["program.c"]` | `.stage() == Stage::Executable` |
| 4 | `each_debug_flag_selects_its_stage` | `--dump-tokens`, `--dump-ast`, `--check`, `-S` | `Stage::Tokens`, `Ast`, `Check`, `Assembly` respectively |
| 5 | `debug_flags_conflict_with_each_other` | argv with both `--dump-tokens` and `--check` | Parse fails (`is_err()`) |
| 6 | `input_file_is_required` | argv `["rustycc"]` (no input) | Parse fails |
| 7 | `keep_temps_defaults_off` | No flag vs. `--keep-temps` | `false` then `true` |
| 8 | `for_source_matches_the_parsed_equivalent` | `Options::for_source(...,Tokens)` vs. parsed `--dump-tokens` | Same `.input`, same `.stage()` |

### `src/lib.rs` (4 tests)

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 9 | `empty_source_compiles` | `compile(b"", "empty.c", Options::for_source(..., Tokens))` | `Ok(...)` |
| 10 | `missing_input_is_a_read_error` | `run(Options::for_source("no-such-file.c", Tokens))` | `Err(Error::Read{path,..})` with `path=="no-such-file.c"` |
| 11 | `read_error_message_names_the_path` | `Error::Read{path:"missing.c", cause: NotFound}` | `.to_string() == "cannot read 'missing.c': No such file or directory"` |
| 12 | `rejection_message_agrees_in_number` | `Error::Rejected{count:1}`, `{count:3}` | `"1 error generated"`, `"3 errors generated"` |

### `tests/cli.rs` (3 tests)

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 13 | `no_arguments_prints_usage_and_fails` | Run `rustycc` binary with no args | Non-zero exit; stderr contains "Usage"/"usage" |
| 14 | `missing_input_file_reports_a_readable_error` | Run `rustycc definitely-not-here.c` | Non-zero exit; stderr contains the path; stderr does NOT contain "panicked" |
| 15 | `compiler_is_callable_in_process` | `rustycc::compile(b"", "in-memory.c", Options::for_source(..., Tokens))` | `.expect(...)` succeeds — no file system access, no child process |

## Actual Outputs

Executed as part of `cargo test --lib` and `cargo test --test cli` (full unedited capture in
`reports/unit_tests/cargo_test_output.txt`):

```
test cli::tests::command_definition_is_valid ... ok
test cli::tests::default_stage_is_an_executable ... ok
test cli::tests::input_file_is_required ... ok
test cli::tests::keep_temps_defaults_off ... ok
test cli::tests::parses_the_documented_invocation ... ok
test cli::tests::for_source_matches_the_parsed_equivalent ... ok
test cli::tests::debug_flags_conflict_with_each_other ... ok
test cli::tests::each_debug_flag_selects_its_stage ... ok
test tests::empty_source_compiles ... ok
test tests::missing_input_is_a_read_error ... ok
test tests::read_error_message_names_the_path ... ok
test tests::rejection_message_agrees_in_number ... ok

     Running tests/cli.rs
test compiler_is_callable_in_process ... ok
test no_arguments_prints_usage_and_fails ... ok
test missing_input_file_reports_a_readable_error ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.38s
```

**Result: all 15 tests passed.** No failures. The `tests/cli.rs` suite genuinely exercises the
built `target/debug/rustycc` binary as a subprocess (confirmed by the 0.38s wall time, consistent
with process spawn overhead rather than in-process function calls). `cargo clippy --all-targets --
-D warnings` and `cargo fmt --check` reported no violations against any of `src/cli.rs`,
`src/lib.rs`, `src/main.rs`, or `tests/cli.rs`.
