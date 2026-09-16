#!/usr/bin/env bash
#
# Capture the evidence the unit test reports cite: what the toolchain is, and what a full run does.
#
# The reports in docs/reports/unit_tests/ say every test passed. That claim is worth what the
# evidence behind it is worth, so the evidence is a file rather than a memory — and a script rather
# than a transcript, so it can be regenerated when the code changes instead of going quietly stale.
#
# Writes two files into docs/reports/unit_tests/evidence/:
#
#   environment.txt   every version the run depended on, from the tools themselves
#   cargo-test.txt    cargo fmt --check, cargo clippy, and cargo test, in that order
#
# Usage: scripts/test-evidence.sh   (from anywhere)

set -euo pipefail

cd "$(dirname "$0")/.."

for directory in "$HOME/.cargo/bin" "$HOME/.local/bin" /opt/homebrew/bin; do
	case ":$PATH:" in
	*":$directory:"*) ;;
	*) [ -d "$directory" ] && PATH="$PATH:$directory" ;;
	esac
done
export PATH

evidence="docs/reports/unit_tests/evidence"
mkdir -p "$evidence"

# Echoes the command before running it, so the file says what produced each block rather than
# leaving the reader to infer it.
run() {
	printf '\n$ %s\n%s\n' "$*" "====================================================================="
	"$@" 2>&1 || printf '(exited %s)\n' "$?"
}

{
	printf 'Captured %s\n' "$(date -u '+%Y-%m-%d %H:%M:%S UTC')"
	run sw_vers
	run uname -m
	run xcode-select -p
	run clang --version
	run rustc --version
	run cargo --version
	run rustup show active-toolchain
	run git --version
	run git rev-parse HEAD
	# The two optional tools. Absent is a legitimate answer for both: the suite runs without either.
	run cargo insta --version
	run cargo fuzz --version
	run uv --version
} >"$evidence/environment.txt"
echo "==> wrote $evidence/environment.txt"

{
	printf 'Captured %s\n' "$(date -u '+%Y-%m-%d %H:%M:%S UTC')"
	run cargo fmt --check
	run cargo clippy --all-targets -- -D warnings
	run cargo test
} >"$evidence/cargo-test.txt"
echo "==> wrote $evidence/cargo-test.txt"

# The summary line of every test binary, which is what a reader checks first.
grep -E "^(test result|running)" "$evidence/cargo-test.txt" || true
