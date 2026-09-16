//! The invalid-program corpus: what this compiler rejects, and how that compares to `clang`.
//!
//! Every file in `tests/programs/invalid/` breaks exactly one rule and carries a header saying
//! which one, what the message should be, and whether `clang` turns it down too. The tests here
//! hold all three to account.
//!
//! The direction that matters is the one nothing else would catch. A check that stops working
//! makes this compiler accept a program it should reject, and no differential test would notice,
//! because a program that compiles under both compilers and behaves the same way under both is
//! indistinguishable from a correct one. This corpus is the only thing standing between a dropped
//! check and a silently wider language.

// clippy.toml exempts test code from the panic-adjacent lints, but only inside `#[test]` bodies;
// the helpers below are test scaffolding too. A corpus file that cannot be read, or one whose
// header does not say what it is for, is a broken checkout rather than something to report a
// diagnostic about — failing loudly at the file that caused it is the useful behavior.
#![allow(clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

use rustycc::lexer;
use rustycc::parser;
use rustycc::sema;

/// Where the invalid corpus lives, relative to the crate root Cargo runs tests from.
const CORPUS: &str = "tests/programs/invalid";

/// The architecture document, which carries the list of deliberate deviations from C.
const ARCHITECTURE: &str = "docs/architecture.md";

/// What one invalid program claims about itself.
struct Invalid {
    /// The file name, which is how the architecture document refers to it.
    name: String,
    /// The source, header comments included.
    source: String,
    /// The rule it breaks, from `// rule:`.
    rule: String,
    /// The message it should produce, from `// expect:`.
    expected: String,
    /// Whether `clang` rejects it too, from `// clang:`.
    clang_rejects: bool,
}

/// Every program in the invalid corpus, by file name, in a stable order.
fn corpus() -> BTreeMap<String, Invalid> {
    let entries = fs::read_dir(CORPUS).expect("the invalid corpus directory should exist");

    entries
        .filter_map(|entry| {
            let path = entry.expect("a corpus entry should be readable").path();
            let is_program = path.extension().is_some_and(|extension| extension == "c");

            is_program.then(|| {
                let invalid = read(&path);

                (invalid.name.clone(), invalid)
            })
        })
        .collect()
}

/// Reads one invalid program and the claims in its header.
fn read(path: &Path) -> Invalid {
    let source = fs::read_to_string(path).expect("a corpus program should be readable");
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string();

    let rule = field(&source, "rule")
        .unwrap_or_else(|| panic!("{name} has no `// rule:` line saying what it breaks"));
    let expected = field(&source, "expect")
        .unwrap_or_else(|| panic!("{name} has no `// expect:` line giving its message"));
    let verdict = field(&source, "clang")
        .unwrap_or_else(|| panic!("{name} has no `// clang:` line giving clang's verdict"));

    let clang_rejects = match verdict.as_str() {
        "rejects" => true,
        accepts if accepts.starts_with("accepts") => false,
        other => panic!("{name}: `// clang:` should say `rejects` or `accepts …`, got {other:?}"),
    };

    Invalid {
        name,
        source,
        rule,
        expected,
        clang_rejects,
    }
}

/// The first `// <key>: <value>` line in `source`.
fn field(source: &str, key: &str) -> Option<String> {
    let prefix = format!("// {key}: ");

    source
        .lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .map(|value| value.trim().to_string())
}

/// The messages this compiler reports for `source`, through the whole front end.
///
/// Every pass is run rather than stopping at the first that complains, because which pass turns a
/// program down is an implementation detail the corpus should not be pinned to.
fn messages(source: &str) -> Vec<String> {
    let lexed = lexer::lex(source.as_bytes());
    if !lexed.diagnostics.is_empty() {
        return lexed
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect();
    }

    let parsed = parser::parse(&lexed.tokens);
    if !parsed.diagnostics.is_empty() {
        return parsed
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect();
    }

    sema::analyze(&parsed.program)
        .diagnostics
        .into_iter()
        .map(|diagnostic| diagnostic.message)
        .collect()
}

/// Whether `clang -O0 -std=c99` rejects `path`.
fn clang_rejects(path: &Path) -> bool {
    let status = Command::new("clang")
        .args(["-O0", "-std=c99", "-fsyntax-only"])
        .arg(path)
        .output()
        .expect("clang should be installed; run xcode-select --install");

    !status.status.success()
}

/// The corpus is not empty, so the checks over it are not passing by having nothing to check.
#[test]
fn the_corpus_has_programs_in_it() {
    assert!(
        corpus().len() >= 20,
        "expected a program per rejection rule"
    );
}

/// Every invalid program is rejected, with the message its header promises.
#[test]
fn every_invalid_program_is_rejected_with_its_stated_message() {
    for (name, invalid) in corpus() {
        let reported = messages(&invalid.source);

        assert!(
            !reported.is_empty(),
            "{name} was accepted, but breaks the rule: {}",
            invalid.rule
        );
        assert!(
            reported.contains(&invalid.expected),
            "{name}: expected {:?}, got {reported:?}",
            invalid.expected
        );
    }
}

/// Every invalid program is rejected by `clang` too, or says why it is not.
///
/// This is the check that keeps the subset honest. A file that `clang` builds and this compiler
/// turns down is a restriction rather than a bug, and a restriction has to be a decision somebody
/// wrote down — so the header has to claim it before the test will allow it.
#[test]
fn clang_agrees_or_the_deviation_is_recorded() {
    for (name, invalid) in corpus() {
        let path = Path::new(CORPUS).join(&name);
        let rejected = clang_rejects(&path);

        assert_eq!(
            rejected,
            invalid.clang_rejects,
            "{name}: clang {} it, but the header says it {}",
            if rejected { "rejects" } else { "accepts" },
            if invalid.clang_rejects {
                "rejects"
            } else {
                "accepts"
            }
        );
    }
}

/// Every deviation names the ADR that decided it, and gives a reason.
#[test]
fn every_deviation_cites_a_decision_and_a_reason() {
    for (name, invalid) in corpus() {
        if invalid.clang_rejects {
            continue;
        }

        let verdict = field(&invalid.source, "clang").unwrap_or_default();
        assert!(
            verdict.contains("ADR "),
            "{name}: a deviation has to name the ADR that decided it, got {verdict:?}"
        );
        assert!(
            field(&invalid.source, "why").is_some(),
            "{name}: a deviation has to carry a `// why:` line"
        );
    }
}

/// The deviations in the corpus and the ones in the architecture document are the same set.
///
/// Two lists of the same thing drift apart unless something compares them, and the architecture
/// document is where a reader looks first. A deviation missing from it is a restriction nobody
/// outside this directory can find out about.
#[test]
fn the_architecture_document_lists_exactly_the_deviations_in_the_corpus() {
    let architecture =
        fs::read_to_string(ARCHITECTURE).expect("the architecture document should exist");

    let deviations: Vec<String> = corpus()
        .into_values()
        .filter(|invalid| !invalid.clang_rejects)
        .map(|invalid| invalid.name)
        .collect();

    for name in &deviations {
        assert!(
            architecture.contains(name.as_str()),
            "{name} is a deviation but {ARCHITECTURE} does not mention it"
        );
    }

    let listed = corpus()
        .into_values()
        .filter(|invalid| architecture.contains(invalid.name.as_str()))
        .count();

    assert_eq!(
        listed,
        deviations.len(),
        "{ARCHITECTURE} names a corpus program that is not a deviation"
    );
}

/// Every invalid program has a row in the corpus's coverage matrix.
#[test]
fn every_invalid_program_is_in_the_coverage_matrix() {
    let matrix = fs::read_to_string(Path::new(CORPUS).join("COVERAGE.md"))
        .expect("the invalid corpus should have a coverage matrix");

    for name in corpus().keys() {
        assert!(
            matrix.contains(name.as_str()),
            "{name} has no entry in {CORPUS}/COVERAGE.md"
        );
    }
}

/// Every rule named in the corpus is named once, so two files cannot cover one rule and none another.
#[test]
fn each_rule_is_covered_by_exactly_one_program() {
    let mut rules: Vec<String> = corpus().into_values().map(|invalid| invalid.rule).collect();
    let before = rules.len();
    rules.sort();
    rules.dedup();

    assert_eq!(rules.len(), before, "two programs cover the same rule");
}
