//! The generated corpus: random well-typed programs put through the same `clang` comparison as the
//! hand-written one.
//!
//! A hand-written corpus plateaus. Every program in it was written by someone who already had a
//! theory about what might be broken, so it finds the bugs that fit a theory and stops. The
//! generator has no theory — it builds expression trees nobody would write, in shapes nobody chose,
//! and the comparison against `clang` still knows the right answer for every one of them.
//!
//! Each program is its own comparison. On a failure the seed is printed, and re-running with
//! `RUSTYCC_GENERATED_SEED` set to it reproduces the source byte for byte, so a failure turns into
//! a file rather than into a story about a run that already finished.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too.
#![allow(clippy::expect_used, clippy::panic)]

mod generator;
mod harness;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use harness::{Exit, Verdict};
use rustycc::runtime::SHIM_OBJECT;

/// How many programs a plain `cargo test` generates and compares.
///
/// Small enough that the suite stays a suite. The longer runs are what the environment variable and
/// the scheduled CI job are for; a per-push run that took ten minutes would be turned off inside a
/// week, and a check nobody runs catches nothing.
const DEFAULT_COUNT: usize = 40;

/// The variable that raises [`DEFAULT_COUNT`] for a longer local or scheduled run.
const COUNT_VARIABLE: &str = "RUSTYCC_GENERATED_PROGRAMS";

/// The variable that pins the first seed, so a reported failure can be reproduced exactly.
const SEED_VARIABLE: &str = "RUSTYCC_GENERATED_SEED";

/// The seed the run starts from, and the count it runs.
///
/// Fixed rather than drawn from the clock: a suite that tests something different on every run
/// reports a failure nobody else can reproduce, and reports green without meaning the same thing
/// twice. A longer run covers more by counting further from the same place, not by starting
/// somewhere new.
fn plan() -> (u64, usize) {
    let seed = env::var(SEED_VARIABLE)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0x5EED_0000);
    let count = env::var(COUNT_VARIABLE)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_COUNT);

    (seed, count)
}

/// Writes the program for `seed` into its own directory and returns both paths.
fn materialize(tier: &str, seed: u64) -> (PathBuf, PathBuf) {
    let directory = harness::scratch(tier, &format!("seed-{seed}"));
    let path = directory.join("generated.c");
    fs::write(&path, generator::program(seed)).expect("could not write the generated program");

    (path, directory)
}

/// Every generated program behaves identically under `rustycc` and under `clang`.
///
/// One test rather than one per program, because the programs do not exist until the test runs and
/// Rust needs its test functions at compile time. The seed stands in for the name: the failure
/// message carries it, and it is the whole of what reproducing the failure needs.
#[test]
fn generated_programs_agree_with_clang() {
    let (first, count) = plan();
    let mut failures = Vec::new();

    for offset in 0..count as u64 {
        let seed = first.wrapping_add(offset);
        let (path, directory) = materialize("generated", seed);

        match harness::differential(&path, &directory) {
            Verdict::Agreed(run) => assert!(
                !run.stdout.is_empty(),
                "seed {seed} produced a program that printed nothing"
            ),
            verdict => {
                let report = harness::report(&path, &directory, &verdict)
                    .unwrap_or_else(|| "no report".to_owned());
                failures.push(format!("seed {seed}: {report}"));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} of {count} generated programs disagreed. Reproduce one with \
         {SEED_VARIABLE}=<seed> cargo test --test generated\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A seed reproduces its program exactly.
///
/// Everything above rests on this. A failing seed is only worth printing if running it again
/// produces the same program, and a generator that drew from the clock or iterated a hash map would
/// report failures nobody could look into.
#[test]
fn a_seed_reproduces_its_program_byte_for_byte() {
    for seed in [0, 1, 7, 0x5EED_0000, u64::MAX] {
        assert_eq!(
            generator::program(seed),
            generator::program(seed),
            "seed {seed} generated two different programs"
        );
    }
}

/// Different seeds produce different programs.
///
/// Reproducibility is easy to get by accident in the worst way — a generator that ignores its seed
/// passes the test above perfectly. This is the other half of the claim.
#[test]
fn different_seeds_produce_different_programs() {
    let first = generator::program(1);
    let second = generator::program(2);
    let third = generator::program(3);

    assert_ne!(first, second);
    assert_ne!(second, third);
    assert_ne!(first, third);
}

/// Every generated program is accepted by this compiler's front end.
///
/// The generator is supposed to emit subset C, not C at large. A program `rustycc` rejects would be
/// reported by the comparison as one compiler not building it, which is a real outcome for the
/// corpus and a bug in the generator here.
#[test]
fn generated_programs_are_inside_the_subset() {
    let (first, count) = plan();

    for offset in 0..count as u64 {
        let seed = first.wrapping_add(offset);
        let source = generator::program(seed);
        let path = PathBuf::from(format!("generated-{seed}.c"));
        let options = rustycc::cli::Options::for_source(&path, rustycc::cli::Stage::Check);

        if let Err(diagnostics) = rustycc::compile(source.as_bytes(), &path, &options) {
            panic!(
                "seed {seed} generated a program outside the subset: {:?}\n{source}",
                diagnostics
                    .iter()
                    .map(|diagnostic| &diagnostic.message)
                    .collect::<Vec<_>>()
            );
        }
    }
}

/// No generated program has undefined behavior in it, according to `clang`'s own checker.
///
/// The generator rules out overflow, division by zero, and out-of-range subscripts by construction,
/// and this is the check on that reasoning being right. Two compilers agreeing about an undefined
/// program proves nothing, so a generator that quietly started emitting one would turn this whole
/// suite into noise that still passed.
///
/// Fewer programs than the comparison above, because each one is compiled a third time with the
/// checker built in, and the reasoning being tested is the generator's, which does not vary from
/// seed to seed the way a compiler bug might.
#[test]
fn generated_programs_are_free_of_undefined_behavior() {
    let (first, _) = plan();
    let checked = 12;
    let mut reports = Vec::new();

    for offset in 0..checked {
        let seed = first.wrapping_add(offset);
        let (path, directory) = materialize("generated-ubsan", seed);
        let binary = directory.join("checked.out");

        let built = Command::new("clang")
            .args([
                "-std=c99",
                "-O0",
                "-fsanitize=undefined",
                "-fno-sanitize-recover=undefined",
            ])
            .arg(&path)
            .arg(SHIM_OBJECT)
            .arg("-o")
            .arg(&binary)
            .output()
            .expect("could not run clang; run xcode-select --install");
        assert!(
            built.status.success(),
            "seed {seed} did not build under the sanitizer:\n{}",
            String::from_utf8_lossy(&built.stderr)
        );

        let run = harness::execute(&binary, &directory, harness::timeout());

        // Both halves matter. The checker reports what it found on stderr, and with
        // `-fno-sanitize-recover` it then ends the program — so a run that says nothing and still
        // does not exit cleanly is a finding this test would otherwise sleep through. Every
        // generated `main` ends in `return 0`, which is what makes the status worth asserting.
        if !run.stderr.is_empty() || run.exit != Exit::Code(0) {
            reports.push(format!(
                "seed {seed}: {}, {}",
                run.exit,
                String::from_utf8_lossy(&run.stderr)
            ));
        }
    }

    assert!(
        reports.is_empty(),
        "the sanitizer found undefined behavior in generated programs:\n{}",
        reports.join("\n")
    );
}
