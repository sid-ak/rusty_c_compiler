#!/usr/bin/env bash
#
# Capture the evidence each unit's report cites: what a run of just that unit's tests actually
# produced, and when the code under test and the tests themselves were written.
#
# A unit is usually one `src/**/tests.rs` file together with the source file it tests
# (`src/diagnostics.rs` for `src/diagnostics/tests.rs`, `src/lexer/mod.rs` for
# `src/lexer/tests.rs`, and so on). A few units are instead, or also, tested by an integration test
# under tests/ against a source file outside src/ entirely — `runtime/shim.c`, tested by
# `tests/runtime_shim.rs` — so those are a second, explicitly listed kind of unit below. The
# environment is a third, singular kind: not code under test at all, but the toolchain and lint
# gates every other unit's test run depends on.
#
# For each unit with a narrative report in docs/reports/unit_tests/ this writes
# docs/reports/unit_tests/<report folder>/evidence_<name>.md, beside that report, containing:
#
#   - the source file and the test file, hyperlinked to GitHub (environment: the toolchain and
#     lint gate versions instead, since there is no single source/test pair)
#   - when the source file was created, and when its tests were first created and last updated
#     (all from git history)
#   - the exact `cargo test` invocation scoped to that unit, and its complete output
#
# A unit with no narrative report yet (see the case statement in `narrative_folder` below, and
# `integration_units` for the tests/ kind) is skipped rather than guessed at.
#
# Usage:
#   scripts/test-evidence.sh                 regenerate every unit's evidence, environment included
#   scripts/test-evidence.sh <module-path>    regenerate one src/ unit only, e.g. `diagnostics` or
#                                              `lexer/token` (path under src/, without `/tests.rs`).
#                                              `lib` is the crate root (src/tests.rs) — a special
#                                              case, since it names src/lib.rs, not a subdirectory.
#   scripts/test-evidence.sh <integration>    regenerate one tests/ unit only, e.g. `runtime_shim`
#                                              (see `integration_units`)
#   scripts/test-evidence.sh environment      regenerate only the toolchain/lint-gate evidence
#   scripts/test-evidence.sh all              regenerate only the whole-suite cross-check

set -euo pipefail

cd "$(dirname "$0")/.."

for directory in "$HOME/.cargo/bin" "$HOME/.local/bin" /opt/homebrew/bin; do
	case ":$PATH:" in
	*":$directory:"*) ;;
	*) [ -d "$directory" ] && PATH="$PATH:$directory" ;;
	esac
done
export PATH

repo_url="https://github.com/sid-ak/rusty_c_compiler"
branch="main"
run_date="$(date -u '+%Y-%m-%d %H:%M:%S UTC')"
out_root="docs/reports/unit_tests"

# Where each unit's narrative report already lives, keyed by the module path under src/ (without
# `/tests.rs`). Two modules can share one report — `driver` and `cli` both land in
# 12-driver-and-cli, which reports on both together — so this is a lookup, not a 1:1 derivation.
# A module absent here has no narrative report yet, so generate_unit skips it rather than placing
# evidence with nothing to sit beside.
narrative_folder() {
	case "$1" in
	diagnostics) echo "01-diagnostics" ;;
	ast) echo "02-ast" ;;
	lexer/token) echo "03-lexer-token-model" ;;
	lexer) echo "04-lexer-scanner" ;;
	parser/expr) echo "05-parser-expressions" ;;
	parser) echo "06-parser-statements-recovery" ;;
	sema/types) echo "07-sema-type-model" ;;
	sema/scope) echo "08-sema-scopes" ;;
	sema) echo "09-sema-analyzer" ;;
	codegen/emit) echo "10-codegen-emitter" ;;
	codegen/frame) echo "11-codegen-frame-and-lowering" ;;
	driver) echo "12-driver-and-cli" ;;
	cli) echo "12-driver-and-cli" ;;
	lib) echo "12-driver-and-cli" ;;
	runtime) echo "13-runtime-shim" ;;
	*) echo "" ;;
	esac
}

# Integration units: one line each as "test file|source file|report folder|cargo --test binary
# name". Unlike a src/ unit, the source file here isn't derived from the test file's path — the
# test file lives under tests/, the source it exercises can be anywhere (including outside src/
# entirely) — so both sides are spelled out explicitly rather than inferred.
integration_units() {
	printf '%s\n' \
		"tests/runtime_shim.rs|runtime/shim.c|13-runtime-shim|runtime_shim"
}

# The date of the commit that first added a path, falling back silently to nothing if the path
# has no history yet (e.g. it's staged but not committed).
first_commit_date() {
	git log --follow --diff-filter=A --format='%ad' --date=short -- "$1" | tail -1
}

# The date of the most recent commit touching a path.
last_commit_date() {
	git log -1 --follow --format='%ad' --date=short -- "$1"
}

# `cargo test`'s filter is a substring match anywhere in a test's full path, so a bare module
# prefix over-matches: "sema::" also matches "sema::scope::tests::..." and "sema::types::tests::
# ...". Rather than rely on a filter string, list every test once and select by exact full name
# for the unit being generated — unambiguous regardless of how modules nest.
all_tests="$(cargo test --lib -- --list 2>&1)"

# Writes docs/reports/unit_tests/<report folder>/evidence_<name>.md for one unit.
#   $1 = the unit's test file, e.g. src/diagnostics/tests.rs or src/lexer/token/tests.rs
generate_unit() {
	local test_file="$1"
	local module_dir rel source_file filter_prefix folder
	module_dir="$(dirname "$test_file")"

	if [ "$module_dir" = "src" ]; then
		rel="lib"
		source_file="src/lib.rs"
		filter_prefix="tests::"
	else
		rel="${module_dir#src/}"
		if [ -f "${module_dir}.rs" ]; then
			source_file="${module_dir}.rs"
		else
			source_file="${module_dir}/mod.rs"
		fi
		filter_prefix="${rel//\//::}::tests::"
	fi

	folder="$(narrative_folder "$rel")"
	if [ -z "$folder" ]; then
		echo "==> skipping $rel: no narrative report claims it yet"
		return
	fi

	local out_dir="$out_root/$folder"
	local name="${rel//\//_}"
	local report="$out_dir/evidence_${name}.md"
	mkdir -p "$out_dir"

	local source_written tests_created tests_updated
	source_written="$(first_commit_date "$source_file")"
	tests_created="$(first_commit_date "$test_file")"
	tests_updated="$(last_commit_date "$test_file")"

	local names=()
	while IFS= read -r found; do
		names+=("$found")
	done < <(printf '%s\n' "$all_tests" | grep -E "^${filter_prefix}[^[:space:]]+: test$" | sed -E 's/: test$//')

	{
		printf '<!-- Automatically generated by scripts/test-evidence.sh on %s -->\n\n' "$run_date"
		printf '# Test Report — `%s`\n\n' "$rel"
		printf '## Files\n\n'
		printf -- '- Source: [`%s`](%s/blob/%s/%s)\n' "$source_file" "$repo_url" "$branch" "$source_file"
		printf -- '    - Created: %s\n' "$source_written"
		printf -- '- Tests: [`%s`](%s/blob/%s/%s)\n' "$test_file" "$repo_url" "$branch" "$test_file"
		printf -- '    - Created: %s\n' "$tests_created"
		printf -- '    - Updated: %s\n\n' "$tests_updated"
		printf '## Test Run\n\n'
		if [ "${#names[@]}" -eq 0 ]; then
			printf '_No tests matched `%s`._\n' "$filter_prefix"
		else
			printf '```\n$ cargo test --lib -- --exact %s...  (%s tests)\n' "$filter_prefix" "${#names[@]}"
			printf '%s\n' "====================================================================="
			cargo test --lib -- --exact "${names[@]}" 2>&1 || printf '(exited %s)\n' "$?"
			printf '```\n'
		fi
	} >"$report"
	echo "==> wrote $report"
}

# Writes docs/reports/unit_tests/<report folder>/evidence_<bin_name>.md for one integration unit.
# Runs the whole binary rather than selecting by exact test name: unlike src/'s nested modules,
# one tests/*.rs file is one cargo test binary, so there is nothing else in it to over-match.
#   $1 = test file, $2 = source file, $3 = report folder, $4 = cargo --test binary name
generate_integration_unit() {
	local test_file="$1" source_file="$2" folder="$3" bin_name="$4"

	local out_dir="$out_root/$folder"
	local report="$out_dir/evidence_${bin_name}.md"
	mkdir -p "$out_dir"

	local source_written tests_created tests_updated
	source_written="$(first_commit_date "$source_file")"
	tests_created="$(first_commit_date "$test_file")"
	tests_updated="$(last_commit_date "$test_file")"

	{
		printf '<!-- Automatically generated by scripts/test-evidence.sh on %s -->\n\n' "$run_date"
		printf '# Test Report — `%s`\n\n' "$bin_name"
		printf '## Files\n\n'
		printf -- '- Source: [`%s`](%s/blob/%s/%s)\n' "$source_file" "$repo_url" "$branch" "$source_file"
		printf -- '    - Created: %s\n' "$source_written"
		printf -- '- Tests: [`%s`](%s/blob/%s/%s)\n' "$test_file" "$repo_url" "$branch" "$test_file"
		printf -- '    - Created: %s\n' "$tests_created"
		printf -- '    - Updated: %s\n\n' "$tests_updated"
		printf '## Test Run\n\n'
		printf '```\n$ cargo test --test %s\n' "$bin_name"
		printf '%s\n' "====================================================================="
		cargo test --test "$bin_name" 2>&1 || printf '(exited %s)\n' "$?"
		printf '```\n'
	} >"$report"
	echo "==> wrote $report"
}

# Echoes a command before running it, so the transcript says what produced each block rather than
# leaving the reader to infer it.
env_run() {
	printf '$ %s\n%s\n' "$*" "====================================================================="
	"$@" 2>&1 || printf '(exited %s)\n' "$?"
	printf '\n'
}

# Writes docs/reports/unit_tests/00-environment/evidence_environment.md: the toolchain every unit's
# test run above depended on, and the two lint gates, captured in the same invocation as those runs
# so this can never drift from what actually produced them.
generate_environment() {
	local out_dir="$out_root/00-environment"
	local report="$out_dir/evidence_environment.md"
	mkdir -p "$out_dir"

	{
		printf '<!-- Automatically generated by scripts/test-evidence.sh on %s -->\n\n' "$run_date"
		printf '# Test Report — `environment`\n\n'
		printf '## Toolchain and Lint Gates\n\n'
		printf '```\n'
		env_run sw_vers
		env_run uname -m
		env_run xcode-select -p
		env_run clang --version
		env_run rustc --version
		env_run cargo --version
		env_run rustup show active-toolchain
		env_run git --version
		env_run git rev-parse HEAD
		# The three optional tools. Absent is a legitimate answer for each: the suite runs without
		# any of them.
		env_run cargo insta --version
		env_run cargo fuzz --version
		env_run uv --version
		env_run cargo fmt --check
		env_run cargo clippy --all-targets -- -D warnings
		printf '```\n'
	} >"$report"
	echo "==> wrote $report"
}

# Writes docs/reports/unit_tests/evidence_all.md: every unit's own evidence_*.md, concatenated in
# report order, outside every unit's own folder because it isn't any one unit's evidence — it's
# all of them in one file to scan or diff at once, dates included, rather than a fresh test run of
# its own. Reads whatever evidence_*.md files are currently on disk, so run this after (or as part
# of) regenerating the units it should reflect, not before.
generate_all() {
	local report="$out_root/evidence_all.md"

	{
		printf '<!-- Automatically generated by scripts/test-evidence.sh on %s -->\n\n' "$run_date"
		printf '# Test Report — `all units`\n\n'
		printf "Every unit's own evidence, concatenated in report order, for scanning or diffing all\n"
		printf 'of it at once — not a replacement for the per-unit evidence beside each report.\n\n'
		local first=1
		while IFS= read -r file; do
			if [ "$first" -eq 0 ]; then
				printf '\n---\n\n'
			fi
			first=0
			cat "$file"
		done < <(find "$out_root" -mindepth 2 -name 'evidence_*.md' | sort)
	} >"$report"
	echo "==> wrote $report"
}

# True (exit 0) if $1 names one of integration_units's binaries, having already run it as a side
# effect — used so the single-unit CLI path can dispatch to whichever kind $1 turns out to be.
run_integration_unit_if_named() {
	local wanted="$1" line test_file source_file folder bin_name
	while IFS='|' read -r test_file source_file folder bin_name; do
		if [ "$bin_name" = "$wanted" ]; then
			generate_integration_unit "$test_file" "$source_file" "$folder" "$bin_name"
			return 0
		fi
	done < <(integration_units)
	return 1
}

if [ "$#" -gt 0 ]; then
	if [ "$1" = "environment" ]; then
		generate_environment
	elif [ "$1" = "all" ]; then
		generate_all
	elif [ "$1" = "lib" ]; then
		# The crate root: src/tests.rs, not src/lib/tests.rs — "lib" names the module (src/lib.rs),
		# it is not a subdirectory of src/ the way every other module-path argument is.
		generate_unit "src/tests.rs"
	elif ! run_integration_unit_if_named "$1"; then
		generate_unit "src/$1/tests.rs"
	fi
else
	while IFS= read -r test_file; do
		generate_unit "$test_file"
	done < <(find src -name tests.rs | sort)

	while IFS='|' read -r test_file source_file folder bin_name; do
		generate_integration_unit "$test_file" "$source_file" "$folder" "$bin_name"
	done < <(integration_units)

	generate_environment
	generate_all
fi
