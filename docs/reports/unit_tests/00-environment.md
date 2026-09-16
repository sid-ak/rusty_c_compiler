# Development and Test Environment

## Purpose

This document records exactly what software (with version numbers) was used to develop and to test
the Rusty C Compiler (`rustycc`), and gives step-by-step setup instructions so a maintenance team —
or a grader — can recreate both the development environment and the test environment from a clean
machine. It accompanies the unit test reports in this same directory (`reports/unit_tests/`).

Raw, unedited version-probe output this document is built from is checked in alongside it:
`env_snapshot.txt` (toolchain versions) and `cargo_test_output.txt` (the actual `cargo fmt`/`cargo
clippy`/`cargo test` run each unit test report's "Actual Outputs" section quotes from). Both were
captured on 2026-08-23 on the development machine described below. A small shell script,
`regenerate_test_evidence.sh`, is also checked in this directory — running it regenerates both files
from scratch, so this environment record and the reports it supports can be refreshed as the system
evolves rather than going stale.

## Hardware and operating system

| Item | Value |
|---|---|
| Machine | Apple Silicon Mac (MacBook Pro), architecture `arm64` |
| OS | macOS (`ProductVersion` 26.5.2, `BuildVersion` 25F84 — reported by `sw_vers`) |
| Kernel | Darwin 25.5.0 (`arm64_T8103`, reported by `uname -a`) |

Apple Silicon is a hard requirement, not an incidental detail: the compiler's code-generation target
is ARM64 macOS (AAPCS64 ABI) specifically because it matches the development machine and avoids
cross-compilation or emulation (ADR 0003). None of this project's tests can run meaningfully on any
other architecture or OS — the runtime shim tests, the (future) differential tests, and the
compiler's own eventual code generation all assume they can compile and directly execute ARM64
Mach-O binaries on the host.

## Development software

| Software | Version | Role |
|---|---|---|
| Rust toolchain | **1.97.1** (`rustc 1.97.1 (8bab26f4f 2026-07-14)`, `cargo 1.97.1 (c980f4866 2026-06-30)`) | Compiles `rustycc` itself. Pinned exactly (not "stable") in `rust-toolchain.toml` so a local build and a CI build always use the identical compiler and lints. |
| `rustup` | Installed, `stable-aarch64-apple-darwin` active/default, `1.97.1-aarch64-apple-darwin` installed | Manages the pinned toolchain; `rustup show` in the repo root auto-installs 1.97.1 if not already present, because of `rust-toolchain.toml`. |
| Xcode Command Line Tools | Xcode **26.1.1** (Build 17B100); `xcode-select -p` → `/Applications/Xcode.app/Contents/Developer` | Provides the assembler and linker `rustycc`'s driver will shell out to (Phase 4+), and provides `clang`, which the build script uses today to compile `runtime/shim.c`. |
| `clang` (Apple) | **Apple clang version 17.0.0** (clang-1700.4.4.1), target `arm64-apple-darwin25.5.0` | Compiles `runtime/shim.c` (via `build.rs`) and, later, will serve as both the assembler/linker (ADR 0009) and the differential-testing oracle (ADR 0001). |
| `git` | **2.50.1** (Apple Git-155) | Version control. |
| `clap` (Rust crate) | **4.6.6** (locked; `Cargo.toml` requests `4.5`), with `clap_derive` 4.6.4 | Command-line argument parsing for the `rustycc` binary (`src/cli.rs`). |
| `cargo fmt` / `cargo clippy` | Bundled with the pinned 1.97.1 toolchain via the `rustfmt`/`clippy` components in `rust-toolchain.toml` | Formatting and linting gates; both are required clean in CI and were run clean for this report (see `cargo_test_output.txt`). |

Full transitive dependency versions (all resolved and locked, so a build reproduces the identical
dependency graph) are in `Cargo.lock` at the repo root; the direct dependency is `clap` above, and
the sole dev-dependency is `insta` (see Test Software below).

## Test software

| Software | Version | Role |
|---|---|---|
| `cargo test` (built into `cargo` 1.97.1) | 1.97.1 | Runs all unit tests (`#[cfg(test)]` modules) and integration tests (`tests/*.rs`). |
| `insta` (Rust crate) | **1.48.0** (locked; `Cargo.toml` requests `1.43`) | Snapshot/golden-master testing framework used by the Lexer Scanner and Parser Statements/Declarations units (`tests/lexer_snapshots.rs`, `tests/parser_snapshots.rs`, and inline snapshot assertions in `src/diagnostics.rs`/`tests/parser_snapshots.rs`). |
| `cargo-insta` (CLI) | **Not installed** in this environment (`cargo insta --version` → `error: no such command: 'insta'`) | Optional developer convenience (`cargo insta review`/`accept`) for triaging snapshot diffs interactively. **Not required to run the test suite** — the `insta` crate's `assert_snapshot!` macro works standalone under plain `cargo test`; the CLI is only needed if a snapshot changes and a human wants an interactive diff-review UI instead of reading the `.pending-snap` file by hand. Install with `cargo install cargo-insta` if needed. |
| `cargo-fuzz` | **Not installed** in this environment (`cargo fuzz --version` → `error: no such command: 'fuzz'`) | Documented as required for Phase 5 fuzz targets (`AGENTS.md`, `docs/dive-deep/testing.md`); not yet used because fuzzing is not yet wired up in this repo (no `fuzz/` directory exists as of this report). Requires a nightly Rust toolchain, installed separately from the pinned stable one: `rustup toolchain install nightly && cargo install cargo-fuzz`. |
| `uv` | **0.11.24** | Python environment/package manager, used only for the documentation site (MkDocs), not for compiling or testing `rustycc` itself. |
| `mkdocs` | **1.6.1** (via the project's `.venv`, Python 3.12) | Builds/serves the prose documentation site (`docs/`). Not required to run or test the compiler; included here because `AGENTS.md` treats `uv run mkdocs build --strict` as part of the full local gate. |

## Setup instructions — development environment

These steps take a clean Apple Silicon Mac to a state where `cargo build` and `cargo test` succeed.

1. **Install Xcode Command Line Tools** (provides `clang`, the assembler, and the linker):
   ```
   xcode-select --install
   ```
   Verify with `clang --version` and `xcrun --show-sdk-path` (both must succeed — the CI pipeline's
   own "Toolchain preflight" job asserts exactly these two commands before anything else runs).

2. **Install `rustup`** (if not already present), from https://rustup.rs, or via Homebrew:
   ```
   brew install rustup-init && rustup-init
   ```

3. **Clone the repository** and `cd` into it:
   ```
   git clone https://github.com/sid-ak/rusty_c_compiler.git
   cd rusty_c_compiler
   ```

4. **Let `rustup` install the pinned toolchain automatically.** `rust-toolchain.toml` at the repo
   root pins `channel = "1.97.1"` with the `rustfmt` and `clippy` components; the first `cargo`
   command run inside the repo triggers `rustup` to install exactly that toolchain if it is not
   already present. To do this explicitly and verify:
   ```
   rustup show
   cargo --version   # should report 1.97.1
   ```

5. **Build**, which also compiles `runtime/shim.c` via the build script (`build.rs`) using the
   `clang` installed in step 1:
   ```
   cargo build
   ```

At this point `./target/debug/rustycc program.c --dump-tokens` (or `--dump-ast`) is runnable.

## Setup instructions — test environment

The test environment is a superset of the development environment above — no additional toolchain
is required to run the current test suite, since `insta`'s assertion macros are an ordinary Rust
dependency pulled in automatically by `cargo`.

1. Complete every step of "Setup instructions — development environment" above.

2. **Run the full test suite**:
   ```
   cargo test
   ```
   This single command builds and runs: the 142 in-crate unit tests (`cargo test --lib`), the CLI
   integration tests (`tests/cli.rs`), the lexer token-stream snapshot (`tests/lexer_snapshots.rs`),
   the parser corpus-truncation robustness suite (`tests/parser_no_panic.rs`), the parser AST
   snapshots (`tests/parser_snapshots.rs`), and the runtime shim compile-link-run suite
   (`tests/runtime_shim.rs`) — 170 tests total as of this report.

3. **Run the lint/format gate** (required clean in CI, and part of the evidence this report's
   "Actual Outputs" sections cite):
   ```
   cargo fmt --check
   cargo clippy --all-targets -- -D warnings
   ```

4. **(Optional) Install `cargo-insta`** for interactively reviewing a snapshot test failure instead
   of reading the diff from `cargo test`'s own output:
   ```
   cargo install cargo-insta
   cargo insta review   # after a snapshot test fails
   ```

5. **(Optional, for future fuzz targets) Install a nightly toolchain and `cargo-fuzz`.** Not
   required for the test suite as it exists today (no fuzz targets are checked in yet), but
   documented here since `AGENTS.md` specifies it as the Phase 5 requirement:
   ```
   rustup toolchain install nightly
   cargo install cargo-fuzz
   cargo +nightly fuzz run lex   # once fuzz targets exist
   ```

6. **(Optional) Build the documentation site**, which has its own, separate toolchain
   (`uv`/Python/MkDocs) unrelated to compiling or testing `rustycc`:
   ```
   uv venv
   uv pip install -r requirements-docs.txt
   uv run mkdocs build --strict
   ```

## Regenerating this document's evidence

`reports/unit_tests/regenerate_test_evidence.sh` re-runs the exact two commands this document and
the eight unit test reports in this directory are built from, writing `env_snapshot.txt` and
`cargo_test_output.txt`. Run it from the repo root (or anywhere — it locates the repo root itself)
whenever the codebase changes and the unit test reports need to be refreshed against current,
real behavior:

```
./reports/unit_tests/regenerate_test_evidence.sh
```

## Continuous integration

For reference, the project's CI (`.github/workflows/ci.yml`) runs the identical commands on GitHub
Actions' `macos-14` runners (Apple Silicon), on every pull request and every push to `main`: a
toolchain preflight (`clang --version`, `xcrun --show-sdk-path`, `rustup show`), `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test`, and a documentation build
(`./scripts/build-docs.sh`). CI is not a substitute for this document — a `macos-14` GitHub-hosted
runner's exact OS/Xcode point version is not user-controlled and can differ from the versions
recorded above — but its steps are the same steps this document tells a maintainer to run locally,
which is what keeps "passes in CI" and "reproducible on a fresh machine" the same claim.
