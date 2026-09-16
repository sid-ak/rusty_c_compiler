# Development and Test Environment

## Purpose

This document records exactly what software, with version numbers, the Rusty C Compiler (`rustycc`)
was developed and tested with, and gives step-by-step instructions for recreating both environments
from a clean machine. It accompanies the unit test reports in this directory.

The raw output every version below was read from is checked in beside this report, as
[`evidence_environment.md`](evidence_environment.md). Every other unit test report has its own
test run checked in the same way, beside that report, as `evidence_<module>.md`. A single run of
`scripts/test-evidence.sh` regenerates all of it, so this record and every report it supports can
be re-verified against the machine and the code rather than trusted.

## Hardware and operating system

| Item | Value |
| --- | --- |
| Machine | Apple Silicon Mac, architecture `arm64` |
| Operating system | macOS 26.6.2 (build 25G83), reported by `sw_vers` |

Apple Silicon is a hard requirement rather than an incidental detail. The compiler's target is ARM64
macOS, chosen because it matches the development machine and avoids cross-compilation or emulation
([ADR 0003](../../../decisions/0003-single-target-arm64-macos.md)). The tests compile and directly
execute ARM64 Mach-O binaries — the runtime shim's tests, the golden-program tests, and the whole
differential suite — so there is no meaningful way to run this suite on another architecture.

## Development software

| Software | Version | Role |
| --- | --- | --- |
| Rust toolchain | 1.97.1 | Compiles `rustycc`. Pinned exactly, not as "stable", in `rust-toolchain.toml`, so a local build and a CI build always use the identical compiler and the identical lints. |
| `rustup` | Installed, with the pinned toolchain | Installs the pin automatically: the first `cargo` command run inside the repository fetches 1.97.1 if it is not already present. |
| Xcode Command Line Tools | Xcode 26.1.1 | Provides `clang`, which the build script uses to compile `runtime/shim.c`, which the driver shells out to for assembling and linking, and which the differential suite uses as its oracle. |
| `clang` (Apple) | Apple clang 17.0.0, target `arm64-apple-darwin` | Three roles at once: it builds the runtime shim, it is the assembler and linker ([ADR 0009](../../../decisions/0009-clang-as-assembler-and-linker.md)), and it is the testing oracle ([ADR 0001](../../../decisions/0001-subset-of-c-with-clang-as-oracle.md)). |
| `git` | 2.50.1 | Version control. |
| `clap` | 4.6.6 (locked; `Cargo.toml` asks for 4.5) | Command-line argument parsing, in `src/cli.rs`. The compiler's only direct dependency. |
| `cargo fmt` and `cargo clippy` | Bundled with the pinned toolchain | Formatting and lint gates. Both are required clean in CI and were clean for this report. |

Every transitive dependency is resolved and locked in `Cargo.lock`, so a build reproduces the same
dependency graph.

## Test software

| Software | Version | Role |
| --- | --- | --- |
| `cargo test` | Bundled with 1.97.1 | Runs every tier: the in-crate unit tests, the integration tests in `tests/`, the golden programs, the differential suite, and the generated corpus. |
| `insta` | 1.48.0 (locked; `Cargo.toml` asks for 1.43) | Snapshot testing. The only dev-dependency. Its assertion macros work under plain `cargo test`. |
| `cargo-insta` (CLI) | Optional | A reviewing interface for a changed snapshot. Not required: without it a changed snapshot still fails, writing a `.snap.new` beside the old one to read by hand. `cargo install cargo-insta` adds it. |
| `cargo-fuzz` | 0.13.2 | The three fuzz targets in `fuzz/`. Needs a nightly toolchain, installed alongside the pinned stable one rather than replacing it: `rustup toolchain install nightly && cargo install cargo-fuzz`. |
| `libfuzzer-sys` | 0.4 | The fuzzing runtime the targets are written against. A dependency of the `fuzz/` crate only, which is deliberately not part of the compiler's own build. |
| `uv` and `mkdocs` | uv 0.11.24, MkDocs 1.6.1 | The documentation site. Not needed to build or test the compiler; included because `AGENTS.md` counts a strict docs build as part of the full local gate. |

## Setup — development environment

These steps take a clean Apple Silicon Mac to a state where `cargo build` and `cargo test` succeed.

1. `xcode-select --install`: installs the Command Line Tools, which provide `clang`, the assembler,
   and the linker.
    - `clang --version && xcrun --show-sdk-path`: both must succeed. CI asserts exactly these two
      before anything else runs, so that a missing toolchain is one clear failure rather than a
      confusing linker error later.
2. `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`: installs `rustup`, if it is not
   already present.
3. `git clone https://github.com/sid-ak/rusty_c_compiler.git && cd rusty_c_compiler`: the repository.
4. `rustup show`: installs the pinned toolchain, because `rust-toolchain.toml` names it.
    - `cargo --version`: should report 1.97.1.
5. `cargo build`: builds the compiler, and compiles `runtime/shim.c` with the `clang` from step 1.

`./target/debug/rustycc program.c -o program && ./program` works from here.

## Setup — test environment

The test environment is the development environment. Nothing further is needed for `cargo test`:
`insta` is an ordinary dependency that `cargo` fetches on its own.

1. Complete every step above.
2. `cargo test`: the whole suite. Roughly a minute on an M1, most of it spent in `clang`, since the
   program tiers compile and run every corpus program two and three times over.
3. `cargo fmt --check && cargo clippy --all-targets -- -D warnings`: the lint gates, which CI also
   runs and which [`evidence_environment.md`](evidence_environment.md) includes — they are
   crate-wide, not scoped to one unit, so they are captured here rather than in any single unit's
   own evidence.
4. `scripts/test-evidence.sh`: regenerates `evidence_environment.md`, along with every other
   report's own `evidence_<module>.md`, from a real run.

Two optional additions:

1. `cargo install cargo-insta`: an interactive reviewer for snapshot diffs.
    - `cargo insta review`: step through them. Never accept a snapshot without reading it.
2. `rustup toolchain install nightly && cargo install cargo-fuzz`: the fuzzing toolchain, which the
   compiler itself does not need.
    - `./scripts/fuzz.sh lex`: fifteen minutes on one target, seeded from every program in the
      repository. Also `parse` and `frontend`.

The documentation site has its own toolchain, unrelated to building or testing the compiler:

1. `uv venv && uv pip install -r requirements-docs.txt`: install MkDocs.
2. `./scripts/build-docs.sh`: build the API reference and the prose site, exactly as CI does.

## Continuous integration

`.github/workflows/ci.yml` runs the same commands on GitHub Actions' `macos-14` runners — Apple
Silicon — on every pull request and every push to `main`: a toolchain preflight, `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`, the differential suite as a check of its
own, and `./scripts/build-docs.sh`.

CI is not a substitute for this document. The exact macOS and Xcode point versions on a hosted
runner are not under anyone's control here and can differ from those recorded above. What CI
guarantees is that the steps this document tells a maintainer to run locally are the same steps that
gate a merge — which is what keeps "passes in CI" and "reproducible on a fresh machine" the same
claim.

`.github/workflows/nightly.yml` runs the long jobs on a schedule: the three fuzz targets for half an
hour each, and a far larger generated corpus than a per-push run can afford.
