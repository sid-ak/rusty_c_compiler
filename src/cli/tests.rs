//! Unit tests for the command line: flag parsing, stage selection, and conflicting flags.

use super::*;

use clap::CommandFactory;

/// Parse `args` as a full command line, including the program name.
fn parse(args: &[&str]) -> Options {
    let mut argv = vec!["rustycc"];
    argv.extend_from_slice(args);
    Options::try_parse_from(argv).expect("expected these arguments to parse")
}

/// The clap definition itself is well formed; clap asserts this, and a broken `#[arg]`
/// otherwise only shows up at runtime.
#[test]
fn command_definition_is_valid() {
    Options::command().debug_assert();
}

/// The documented contract `rustycc program.c -o program` parses into input and output paths.
#[test]
fn parses_the_documented_invocation() {
    let options = parse(&["program.c", "-o", "program"]);

    assert_eq!(options.input, PathBuf::from("program.c"));
    assert_eq!(options.output, Some(PathBuf::from("program")));
    assert_eq!(options.stage(), Stage::Executable);
}

/// With no debug flag, the run goes all the way to an executable.
#[test]
fn default_stage_is_an_executable() {
    assert_eq!(parse(&["program.c"]).stage(), Stage::Executable);
}

/// Each debug flag stops the pipeline at its own stage.
#[test]
fn each_debug_flag_selects_its_stage() {
    let cases = [
        ("--dump-tokens", Stage::Tokens),
        ("--dump-ast", Stage::Ast),
        ("--check", Stage::Check),
        ("-S", Stage::Assembly),
    ];

    for (flag, expected) in cases {
        assert_eq!(parse(&["program.c", flag]).stage(), expected, "for {flag}");
    }
}

/// The debug flags are mutually exclusive; asking to stop in two places is a usage error.
#[test]
fn debug_flags_conflict_with_each_other() {
    let result = Options::try_parse_from(["rustycc", "program.c", "--dump-tokens", "--check"]);

    assert!(
        result.is_err(),
        "expected --dump-tokens --check to conflict"
    );
}

/// An input file is required, so a bare `rustycc` is a usage error rather than a silent no-op.
#[test]
fn input_file_is_required() {
    assert!(Options::try_parse_from(["rustycc"]).is_err());
}

/// `--keep-temps` is off unless asked for.
#[test]
fn keep_temps_defaults_off() {
    assert!(!parse(&["program.c"]).keep_temps);
    assert!(parse(&["program.c", "--keep-temps"]).keep_temps);
}

/// `for_source` builds the same options argv would, so library callers get one code path.
#[test]
fn for_source_matches_the_parsed_equivalent() {
    let built = Options::for_source(Path::new("program.c"), Stage::Tokens);
    let parsed = parse(&["program.c", "--dump-tokens"]);

    assert_eq!(built.input, parsed.input);
    assert_eq!(built.stage(), parsed.stage());
}
