//! Tests of the differential harness itself.
//!
//! The differential suite is the project's acceptance criterion, which makes it the one piece of
//! test code whose passing is taken as evidence about everything else. Test code that cannot fail
//! proves nothing, so every way this harness is supposed to notice a problem is provoked here and
//! asserted on: a wrong answer on each comparison axis, a program that never finishes, a return
//! value too large for an exit status, a death by signal, and a program one compiler will not build.
//!
//! The fixtures live in `tests/harness/fixtures/` rather than in `tests/programs/`, because they
//! are inputs to a test of the harness and not members of the corpus — two of them are not even
//! programs this compiler accepts.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too, and a fixture that cannot be read is a broken
// checkout rather than something to report a diagnostic about.
#![allow(clippy::expect_used, clippy::panic)]

mod harness;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use harness::{Execution, Exit, Verdict};

/// Where the harness's own fixtures live, relative to the crate root Cargo runs tests from.
const FIXTURES: &str = "tests/harness/fixtures";

/// The path to one fixture.
fn fixture(name: &str) -> PathBuf {
    Path::new(FIXTURES).join(format!("{name}.c"))
}

/// A run that produced `stdout`, nothing on stderr, and exited zero.
///
/// The baseline every injected fault below is a single change away from, so each test changes one
/// axis and nothing else.
fn clean(stdout: &str) -> Execution {
    Execution {
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        exit: Exit::Code(0),
    }
}

// -- The comparison notices a wrong answer on each axis ------------------------------------------

/// Two identical runs compare equal, so the tests below are changing the thing they mean to change.
#[test]
fn identical_runs_agree() {
    assert!(harness::compare(&clean("same"), &clean("same")).is_ok());
}

/// A difference in stdout is reported, and reported as stdout.
#[test]
fn different_stdout_is_a_mismatch() {
    let mismatch = harness::compare(&clean("expected"), &clean("actual"))
        .expect_err("different output must not compare equal");

    assert_eq!(mismatch.axis, "stdout");
    assert!(mismatch.oracle.contains("expected"), "{mismatch}");
    assert!(mismatch.subject.contains("actual"), "{mismatch}");
}

/// A difference in trailing whitespace alone is still a difference.
///
/// The axis most likely to be compared loosely by accident: a harness that trims before comparing
/// would pass this, and would then miss a missing newline in every program in the corpus.
#[test]
fn a_trailing_newline_is_part_of_stdout() {
    let mismatch = harness::compare(&clean("text\n"), &clean("text"))
        .expect_err("a missing newline must not compare equal");

    assert_eq!(mismatch.axis, "stdout");
}

/// A difference in stderr is reported, and reported as stderr.
#[test]
fn different_stderr_is_a_mismatch() {
    let mut noisy = clean("same");
    noisy.stderr = b"a warning at runtime".to_vec();

    let mismatch = harness::compare(&clean("same"), &noisy)
        .expect_err("output on stderr must not compare equal to none");

    assert_eq!(mismatch.axis, "stderr");
}

/// A difference in exit status is reported, and reported as the exit status.
#[test]
fn a_different_exit_code_is_a_mismatch() {
    let mut other = clean("same");
    other.exit = Exit::Code(1);

    let mismatch = harness::compare(&clean("same"), &other)
        .expect_err("a different exit code must not compare equal");

    assert_eq!(mismatch.axis, "the exit status");
    assert!(mismatch.oracle.contains('0'), "{mismatch}");
    assert!(mismatch.subject.contains('1'), "{mismatch}");
}

/// Dying on a signal is not the same outcome as returning that number.
///
/// A harness that folded a signal into an exit code would compare a segmentation fault equal to a
/// `return 11`, which is the difference between a crash and an answer.
#[test]
fn a_signal_death_is_not_an_exit_code() {
    let mut died = clean("same");
    died.exit = Exit::Signal(11);
    let mut returned = clean("same");
    returned.exit = Exit::Code(11);

    let mismatch = harness::compare(&returned, &died)
        .expect_err("a signal death must not compare equal to a return value");

    assert_eq!(mismatch.axis, "the exit status");
    assert!(mismatch.subject.contains("signal"), "{mismatch}");
}

/// A timeout is its own outcome, distinct from every way a program can finish.
#[test]
fn a_timeout_is_not_an_exit_code() {
    let mut hung = clean("same");
    hung.exit = Exit::TimedOut;

    let mismatch =
        harness::compare(&clean("same"), &hung).expect_err("a hang must not compare equal");

    assert_eq!(mismatch.axis, "the exit status");
    assert!(mismatch.subject.contains("timed out"), "{mismatch}");
}

/// Two runs that both timed out are reported as the timeout, not as their truncated output.
///
/// A killed program's stdout is however much of it escaped before the signal arrived, so two
/// programs that both ran forever almost always differ there too. Reporting that difference sends
/// whoever reads the failure looking for a wrong answer in a program that never produced one.
#[test]
fn two_timeouts_are_reported_as_the_timeout() {
    let mut oracle = clean("partial output from clang");
    oracle.exit = Exit::TimedOut;
    let mut subject = clean("a different amount of partial output");
    subject.exit = Exit::TimedOut;

    let mismatch =
        harness::compare(&oracle, &subject).expect_err("neither program finished, which is not ok");

    assert_eq!(mismatch.axis, "the exit status");
    assert!(
        mismatch.subject.contains("neither program finished"),
        "{mismatch}"
    );
}

/// A report does not carry megabytes of output.
///
/// A generated program can print more than anyone will scroll through. The whole of it is on disk
/// in the directory the report names; what the message carries is the beginning and how much more
/// there was.
#[test]
fn a_long_output_is_cut_short_in_the_report() {
    let long = "x".repeat(100_000);
    let mismatch =
        harness::compare(&clean(&long), &clean("short")).expect_err("these do not compare equal");

    assert!(
        mismatch.oracle.len() < 1_000,
        "the report carried {} bytes",
        mismatch.oracle.len()
    );
    assert!(mismatch.oracle.contains("more bytes"), "{mismatch}");
}

// -- The whole harness notices a wrong compiler, end to end --------------------------------------

/// A deliberately wrong program on the `rustycc` side is reported as a mismatch.
///
/// The comparison tests above hand `compare` a wrong answer directly. This one produces the wrong
/// answer the way a broken compiler would — by actually building, linking, and running two binaries
/// that disagree — so the path between "the program printed something" and "the harness read it" is
/// tested too.
#[test]
fn an_injected_wrong_answer_is_caught_end_to_end() {
    let directory = harness::scratch("self-tests", "injected");
    let right = fixture("prints_one");
    let wrong = fixture("prints_two");
    let wrong_source = fs::read(&wrong).expect("a fixture should be readable");

    let oracle = harness::build_with_clang(&right, &directory).expect("clang should build it");
    let subject = harness::build_with_rustycc(&wrong, &wrong_source, &directory)
        .expect("rustycc should build it");

    let limit = harness::timeout();
    let oracle_run = harness::execute(&oracle.binary, &directory, limit);
    let subject_run = harness::execute(&subject.binary, &directory, limit);

    let mismatch = harness::compare(&oracle_run, &subject_run)
        .expect_err("two programs printing different things must not compare equal");

    assert_eq!(mismatch.axis, "stdout");
    assert!(mismatch.oracle.contains("one"), "{mismatch}");
    assert!(mismatch.subject.contains("two"), "{mismatch}");
}

/// The same fixture compared against itself agrees, so the test above failed for the reason it says.
#[test]
fn the_same_program_agrees_with_itself_end_to_end() {
    let directory = harness::scratch("self-tests", "agrees");
    let path = fixture("prints_one");

    match harness::differential(&path, &directory) {
        Verdict::Agreed(run) => assert_eq!(run.stdout, b"one"),
        other => panic!("a program should agree with itself: {other:?}"),
    }
}

/// A return value above 255 compares equal, at the low eight bits both compilers can report.
///
/// `return 300` is observable only as 44. If the harness read the number the program returned
/// rather than the status the system reported, this would still pass — so the masked value is
/// asserted directly as well.
#[test]
fn an_exit_code_above_255_compares_at_its_low_eight_bits() {
    let directory = harness::scratch("self-tests", "masked");
    let path = fixture("returns_300");

    match harness::differential(&path, &directory) {
        Verdict::Agreed(run) => assert_eq!(run.exit, Exit::Code(300 & 0xFF)),
        other => panic!("300 and 300 should compare equal: {other:?}"),
    }
}

/// A program that never finishes is killed and reported as a timeout, within a bounded time.
///
/// The limit is passed in rather than taken from the environment, so this test costs two seconds
/// rather than the ten the corpus is allowed. Two rather than a fraction of one: the suite runs its
/// tests in parallel, and under that load the fixture does not always reach its first `write` in
/// the few hundred milliseconds it needs when it is the only thing running — which showed up as
/// this test seeing a timeout with nothing printed before it.
#[test]
fn a_program_that_never_finishes_times_out() {
    let limit = Duration::from_secs(2);
    let directory = harness::scratch("self-tests", "hangs");
    let path = fixture("loops_forever");
    let built = harness::build_with_clang(&path, &directory).expect("clang should build it");

    let started = Instant::now();
    let run = harness::execute(&built.binary, &directory, limit);
    let elapsed = started.elapsed();

    assert_eq!(run.exit, Exit::TimedOut);
    assert!(
        elapsed >= limit,
        "the harness gave up after {elapsed:?}, before the limit it was given"
    );
    assert!(
        elapsed < Duration::from_secs(30),
        "the harness waited {elapsed:?} on a {limit:?} limit"
    );
    // Whatever the program managed to print before it was killed is still captured, which is what
    // makes a timeout diagnosable rather than only reportable.
    assert_eq!(run.stdout, b"starting");
}

/// A program that dies on a signal is reported as having died on one, not as having returned.
#[test]
fn a_crash_is_reported_as_a_signal() {
    let directory = harness::scratch("self-tests", "crashes");
    let path = fixture("crashes");
    let built = harness::build_with_clang(&path, &directory).expect("clang should build it");

    let run = harness::execute(&built.binary, &directory, harness::timeout());

    match run.exit {
        Exit::Signal(_) => {}
        other => panic!("a null dereference should die on a signal, got {other}"),
    }
}

// -- A program one compiler will not build is its own outcome ------------------------------------

/// Real C that this compiler turns down is reported as `rustycc` not building it.
///
/// Distinct from a mismatch on purpose: a program this compiler cannot build is a hole in the
/// subset, and reporting it as a wrong answer would send whoever reads the failure looking for a
/// code generation bug that is not there.
#[test]
fn a_program_rustycc_rejects_is_not_a_mismatch() {
    let directory = harness::scratch("self-tests", "rejected");
    let path = fixture("pointer_declaration");

    match harness::differential(&path, &directory) {
        Verdict::NotBuilt { compiler, cause } => {
            assert_eq!(compiler, "rustycc");
            assert!(
                cause.to_string().contains("rejected"),
                "the cause should say it was rejected: {cause}"
            );
        }
        other => panic!("a declared pointer is outside the subset: {other:?}"),
    }
}

/// A file that is not C at all is reported against `clang`, which is asked first.
#[test]
fn a_program_clang_rejects_is_reported_against_clang() {
    let directory = harness::scratch("self-tests", "not-c");
    let path = fixture("not_c_at_all");

    match harness::differential(&path, &directory) {
        Verdict::NotBuilt { compiler, .. } => assert_eq!(compiler, "clang"),
        other => panic!("this is not a translation unit: {other:?}"),
    }
}

// -- A failure says enough to act on -------------------------------------------------------------

/// An agreeing verdict produces no report, so a green run says nothing at all.
#[test]
fn agreement_is_not_reported() {
    let directory = harness::scratch("self-tests", "quiet");
    let path = fixture("prints_one");
    let verdict = harness::differential(&path, &directory);

    assert!(harness::report(&path, &directory, &verdict).is_none());
}

/// A mismatch report names the program, both answers, and where the artifacts were left.
///
/// The point of the report is that a failure can be taken apart without reproducing it first, so
/// what it has to contain is asserted rather than left to whoever wrote the format.
#[test]
fn a_mismatch_report_says_enough_to_diagnose_it() {
    let directory = harness::scratch("self-tests", "reported");
    let right = fixture("prints_one");
    let wrong = fixture("prints_two");
    let wrong_source = fs::read(&wrong).expect("a fixture should be readable");

    let oracle = harness::build_with_clang(&right, &directory).expect("clang should build it");
    let subject = harness::build_with_rustycc(&wrong, &wrong_source, &directory)
        .expect("rustycc should build it");

    let limit = harness::timeout();
    let oracle_run = harness::execute(&oracle.binary, &directory, limit);
    let subject_run = harness::execute(&subject.binary, &directory, limit);
    let verdict = Verdict::Disagreed(
        harness::compare(&oracle_run, &subject_run).expect_err("these two disagree"),
    );

    let report = harness::report(&right, &directory, &verdict).expect("a mismatch is reported");

    assert!(report.contains("prints_one.c"), "{report}");
    assert!(report.contains("one"), "{report}");
    assert!(report.contains("two"), "{report}");
    assert!(report.contains("clang"), "{report}");
    assert!(report.contains("rustycc"), "{report}");
    assert!(
        report.contains(&directory.display().to_string()),
        "the report should say where the artifacts are: {report}"
    );

    // The assembly the report points at is on disk, so following the report actually gets somewhere.
    let assembly = subject.assembly.expect("the assembly is retained");
    assert!(assembly.exists(), "{} is missing", assembly.display());
}

/// A build failure is reported with what the compiler said, not only that there was one.
#[test]
fn a_build_failure_report_carries_the_reason() {
    let directory = harness::scratch("self-tests", "rejected-report");
    let path = fixture("pointer_declaration");
    let verdict = harness::differential(&path, &directory);

    let report = harness::report(&path, &directory, &verdict).expect("a rejection is reported");

    assert!(report.contains("rustycc"), "{report}");
    assert!(report.contains("pointer_declaration.c"), "{report}");
}
