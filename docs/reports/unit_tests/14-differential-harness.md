# Unit Test Report — The Differential Harness

## Unit

Source under test: `tests/harness/` and `tests/generator/`, tested by
`tests/harness_self_tests.rs` and `tests/generated.rs`.

This unit is test infrastructure, and it is reported here for the same reason it is tested at all:
it is the thing everything else is measured against.

It does four things:

- **Builds a program twice**, once with `rustycc` and once with `clang -O0 -std=c99 -Wall`, linking
  both against the same runtime object so the comparison is between two compilers and not between
  two runtimes.
- **Runs both**, with no arguments, with empty input, and with a wall-clock limit, capturing what
  each wrote and how each ended.
- **Compares the two runs** on what they printed, what they wrote to standard error, and how they
  exited — with death by signal kept distinct from a numeric exit status, and a program that never
  finished kept distinct from both.
- **Generates programs**, from a seed, that are well-typed subset C and stay inside the behavior C
  actually defines.

## Date

2026-09-16

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

**Approach: fault injection on every comparison axis, plus end-to-end provocation of each outcome
the harness is supposed to distinguish.**

The governing idea is that a test harness that has only ever seen agreement has never been shown to
notice anything. A harness that returned "passed" unconditionally would produce exactly the output
this project's suite produces on a good day.

1. **The comparison is a pure function, deliberately.** Running a program and comparing two runs are
   separate pieces of code, so the tests can hand the comparison two records of a run directly, with
   no compiler and no process involved. Each axis then gets a wrong answer on purpose, and the test
   asserts both that a mismatch was reported and that it was reported as the *right* axis.

2. **The cheap wrong answer is tested too.** A difference of one trailing newline, and nothing else,
   must count. A harness that trimmed whitespace before comparing would pass every other test here
   and then miss a missing newline in every program in the corpus for the rest of the project.

3. **Each distinguishable outcome is provoked for real.** Two fixtures that differ only in what they
   print, built and linked and run, so the path between a program printing something and the harness
   reading it is covered and not only the comparison. A program returning `300`, confirming it is
   observed as `44` on both sides. A program that never finishes, killed and reported as a timeout
   within a bounded time. A null dereference — built by `clang` alone, since it is outside the
   subset — confirming a signal death is read as one. A program `rustycc` rejects, and a file that is
   not C at all, each attributed to the right compiler.

4. **The report is asserted on, not just the verdict.** A failure that cannot be acted on without
   first reproducing it is most of the way to no report. The tests check that a mismatch report names
   the program, both answers, and the directory holding both binaries and the emitted assembly — and
   that the assembly it points at is actually on disk.

5. **The generator is held to its own guarantee by a third party.** Its claim is that it never emits
   undefined behavior. That claim is a chain of reasoning about interval arithmetic, and chains of
   reasoning are sometimes wrong, so a sample of its programs is compiled with `clang`'s
   undefined-behavior sanitizer and run. Finding nothing is the check on the reasoning.

6. **Reproducibility is tested in both directions.** A seed must produce the same program twice —
   everything else rests on that, because a printed seed is worthless if it does not reproduce. And
   different seeds must produce different programs, because a generator that ignored its seed would
   pass the first test perfectly.

### Why this test methodology?

Ordinary code is tested by asserting what it produces. A test harness is different: what it produces
is a verdict, and the verdict that matters is the one it has never had to give. Every test here is
constructed to make it give that verdict.

The self-tests also paid for themselves immediately. Reordering the comparison so a timeout is
reported before output is compared came out of a generated program that ran away and was killed
mid-print; the harness reported "stdout differs", which was true, and which sent the investigation
looking for a wrong answer in a program that had never produced one.

## Test Coverage

Covered: every comparison axis, injected wrong on purpose; every outcome the harness distinguishes,
provoked end to end; the contents of a failure report; the generator's determinism, its variety, its
staying inside the subset, and its staying inside defined behavior.

Not covered here: whether the corpus is *wide* enough for agreement to mean much. That is the
coverage matrix's job, and it is recorded in `tests/programs/COVERAGE.md`.

## Automated Test Code

<!-- inventory: tests/harness_self_tests.rs, tests/generated.rs -->
| # | Test | What it pins |
| --- | --- | --- |
| 1 | `identical_runs_agree` | Two identical runs compare equal, so the tests below are changing the thing they mean to change. |
| 2 | `different_stdout_is_a_mismatch` | A difference in stdout is reported, and reported as stdout. |
| 3 | `a_trailing_newline_is_part_of_stdout` | A difference in trailing whitespace alone is still a difference. |
| 4 | `different_stderr_is_a_mismatch` | A difference in stderr is reported, and reported as stderr. |
| 5 | `a_different_exit_code_is_a_mismatch` | A difference in exit status is reported, and reported as the exit status. |
| 6 | `a_signal_death_is_not_an_exit_code` | Dying on a signal is not the same outcome as returning that number. |
| 7 | `a_timeout_is_not_an_exit_code` | A timeout is its own outcome, distinct from every way a program can finish. |
| 8 | `two_timeouts_are_reported_as_the_timeout` | Two runs that both timed out are reported as the timeout, not as their truncated output. |
| 9 | `a_long_output_is_cut_short_in_the_report` | A report does not carry megabytes of output. |
| 10 | `an_injected_wrong_answer_is_caught_end_to_end` | A deliberately wrong program on the `rustycc` side is reported as a mismatch. |
| 11 | `the_same_program_agrees_with_itself_end_to_end` | The same fixture compared against itself agrees, so the test above failed for the reason it says. |
| 12 | `an_exit_code_above_255_compares_at_its_low_eight_bits` | A return value above 255 compares equal, at the low eight bits both compilers can report. |
| 13 | `a_program_that_never_finishes_times_out` | A program that never finishes is killed and reported as a timeout, within a bounded time. |
| 14 | `a_crash_is_reported_as_a_signal` | A program that dies on a signal is reported as having died on one, not as having returned. |
| 15 | `a_program_rustycc_rejects_is_not_a_mismatch` | Real C that this compiler turns down is reported as `rustycc` not building it. |
| 16 | `a_program_clang_rejects_is_reported_against_clang` | A file that is not C at all is reported against `clang`, which is asked first. |
| 17 | `agreement_is_not_reported` | An agreeing verdict produces no report, so a green run says nothing at all. |
| 18 | `a_mismatch_report_says_enough_to_diagnose_it` | A mismatch report names the program, both answers, and where the artifacts were left. |
| 19 | `a_build_failure_report_carries_the_reason` | A build failure is reported with what the compiler said, not only that there was one. |
| 20 | `generated_programs_agree_with_clang` | Every generated program behaves identically under `rustycc` and under `clang`. |
| 21 | `a_seed_reproduces_its_program_byte_for_byte` | A seed reproduces its program exactly. |
| 22 | `different_seeds_produce_different_programs` | Different seeds produce different programs. |
| 23 | `generated_programs_are_inside_the_subset` | Every generated program is accepted by this compiler's front end. |
| 24 | `generated_programs_are_free_of_undefined_behavior` | No generated program has undefined behavior in it, according to `clang`'s own checker. |
<!-- end inventory -->

## Actual Outputs

Every test in the table above passed, with no failures, and both lint gates — `cargo fmt --check`
and `cargo clippy --all-targets -- -D warnings` — were clean.

The complete, unedited output of that run is checked in beside this report as
[`evidence/cargo-test.txt`](evidence/cargo-test.txt), and the versions of everything it was run with
are in [`evidence/environment.txt`](evidence/environment.txt). Both are regenerated by
`scripts/test-evidence.sh`, so this report can be re-verified against the code rather than trusted.
