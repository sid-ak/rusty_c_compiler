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

2026-09-16

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
not simulated. Every stage a flag can select is driven end to end, including the default one:
`compiling_and_running_a_program_works_end_to_end` compiles a program, links it, runs it, and checks
what it printed, which is the contract the original proposal states in one sentence.

The driver's own half is covered by what it leaves behind rather than by what it produces. Its
intermediates are removed however the run ends — including when the link fails, which is the path
nobody remembers to tidy by hand — and kept when the caller asks for them. Two compilations running
at once are checked not to write over each other, because a fixed temporary path is the kind of
mistake that passes every test run one at a time.

What is out of scope here: whether the program the driver produced computes the right answer. That
is the code generation units' question, and ultimately the differential suite's.

## Automated Test Code

The tests live in `src/cli/tests.rs`, `src/driver/tests.rs`, and `src/tests.rs` — the modules' own
test files — with the black-box suite in `tests/cli.rs`. (`src/main.rs` itself has
no `#[cfg(test)]` module — by design, per its own doc comment, "everything else lives in the
library so integration tests can drive the compiler in process").

<!-- inventory: src/cli/tests.rs, src/driver/tests.rs, src/tests.rs, tests/cli.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `command_definition_is_valid` | The clap definition itself is well formed; clap asserts this, and a broken `#[arg]` otherwise only shows up at runtime. |
| 2 | `parses_the_documented_invocation` | The documented contract `rustycc program.c -o program` parses into input and output paths. |
| 3 | `default_stage_is_an_executable` | With no debug flag, the run goes all the way to an executable. |
| 4 | `each_debug_flag_selects_its_stage` | Each debug flag stops the pipeline at its own stage. |
| 5 | `debug_flags_conflict_with_each_other` | The debug flags are mutually exclusive; asking to stop in two places is a usage error. |
| 6 | `input_file_is_required` | An input file is required, so a bare `rustycc` is a usage error rather than a silent no-op. |
| 7 | `keep_temps_defaults_off` | `--keep-temps` is off unless asked for. |
| 8 | `for_source_matches_the_parsed_equivalent` | `for_source` builds the same options argv would, so library callers get one code path. |
| 9 | `preflight_finds_the_toolchain` | A toolchain that is present reports itself as present. |
| 10 | `empty_source_compiles` | An empty translation unit is valid C and compiles without diagnostics. |
| 11 | `missing_input_is_a_read_error` | A missing input file is an `Error::Read` naming the path, not a panic. |
| 12 | `read_error_message_names_the_path` | The rendered form of a read failure names the path and the underlying cause. |
| 13 | `rejection_message_agrees_in_number` | The rejection summary agrees in number with the count it reports. |
| 14 | `no_arguments_prints_usage_and_fails` | Running `rustycc` with no arguments prints usage and exits non-zero. |
| 15 | `missing_input_file_reports_a_readable_error` | A missing input file is reported readably rather than as a panic or a bare exit code. |
| 16 | `compiler_is_callable_in_process` | The compiler is callable as a library, with no child process and no file system access. |
| 17 | `check_accepts_every_valid_program` | `rustycc --check` exits 0 for every valid program in the corpus and prints nothing. |
| 18 | `check_rejects_every_invalid_program` | `rustycc --check` exits non-zero for every program in the invalid corpus, with a rendered error. |
| 19 | `dump_annotations_prints_the_annotation_tables` | `rustycc --dump-annotations` prints what analysis recorded, for a program that analyzes. |
| 20 | `compiling_and_running_a_program_works_end_to_end` | `rustycc program.c -o program && ./program` works, which is the contract the proposal states. |
| 21 | `dash_s_emits_assembly_and_no_binary` | `-S` writes assembly and produces no binary. |
| 22 | `dash_c_emits_an_object_file` | `-c` produces an object file rather than an executable. |
| 23 | `emit_asm_to_writes_the_assembly_alongside_the_binary` | `--emit-asm-to` writes the assembly to the given path while still producing the program. |
| 24 | `intermediates_are_removed_unless_they_are_asked_for` | Intermediate files are gone once the run ends, and kept when the caller asks. |
| 25 | `a_failing_link_is_reported_and_cleans_up` | A failing link leaves nothing behind either, and says what the toolchain said. |
| 26 | `concurrent_compilations_do_not_collide` | Two compilations running at once do not write over each other's intermediates. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of each unit's test run, together with when the source and its
tests were written, is checked in beside this report as
[`evidence_driver.md`](evidence_driver.md), [`evidence_cli.md`](evidence_cli.md), and
[`evidence_lib.md`](evidence_lib.md) (the crate root, `src/lib.rs`). All three are regenerated
by `scripts/test-evidence.sh`, so this report can be re-verified against the code rather than
trusted.
