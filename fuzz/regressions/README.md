# Fuzz regressions

Every input a fuzz run has ever crashed on, minimized, kept here as a file.

A crash the fuzzer found once is only fixed if it stays fixed, and a fuzz corpus is the wrong place
to guarantee that: it is machine-specific, it is regenerated, and nobody re-fuzzes before merging.
So a finding is minimized and checked in here instead, where `cargo test` runs it — through the
lexer, the parser, and semantic analysis — on every change, on a deliberately small stack.

`tests/frontend_no_panic.rs` is what runs them. `scripts/fuzz.sh` also seeds every run from this
directory, so a past crash is one of the first things a new run tries.

## Adding one

1. Reproduce it: `cargo +nightly fuzz run <target> fuzz/artifacts/<target>/<crash file>`.
2. Minimize it: `cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/<crash file>`.
3. Copy the minimized input here with a name that says what it is, ending in `.c`.
4. Fix the compiler, and check that the file goes from failing to passing rather than only passing.

## What is here now

Nothing. The three targets have each been run for the documented fifteen minutes from a seed corpus
built out of `tests/programs/`, `tests/programs/invalid/`, and `tests/adversarial/`, with no crash
and no timeout found. That is a statement about what has been run so far, not a claim that none
exists — which is the reason this directory has a README rather than being left out until it is
needed.
