#!/usr/bin/env bash

# Helper function to print and execute commands dynamically
run() {
	echo -e "\n$*"
	echo -e "====================================="
	"$@"
}

# --- Section 1: Environment Snapshot ---
{
	run sw_vers
	run uname -a
	run xcode-select -p
	run xcodebuild -version
	run clang --version
	run git --version
	run rustc --version
	run cargo --version
	run rustup show
	run cargo insta --version
	run cargo fuzz --version
	run uv --version

	# Executed via a subshell to safely isolate the directory change
	(echo -e "\n== (cd ~/repos/rusty_c_compiler && uv run mkdocs --version) ==" && cd ~/repos/rusty_c_compiler && uv run mkdocs --version)

} >env_snapshot.txt 2>&1
echo "==> Wrote env_snapshot.txt"

# --- Section 2: Cargo Tests ---
{
	run cargo fmt --check
	run cargo clippy --all-targets -- -D warnings
	run cargo test -- --nocapture
	run cargo test --lib -- --nocapture

} >cargo_test_output.txt 2>&1
echo "==> Wrote cargo_test_output.txt"
