#!/usr/bin/env bash
#
# Run one fuzz target for a fixed duration, from a seed corpus built out of the repository.
#
# Usage: scripts/fuzz.sh <target> [seconds]
#
#   target   one of lex, parse, frontend
#   seconds  how long to run; defaults to the 900 seconds (15 minutes) per target that
#            docs/PLAN.md requires before a front-end phase is called done
#
# The seed corpus is assembled here rather than checked in. Everything in it already exists in the
# repository — the valid programs, the invalid ones, and the adversarial inputs Phase 2 collected —
# and a second copy under fuzz/ would be one more thing to keep in step, going stale the first time
# a program was added. What libfuzzer discovers during a run is kept, so a later run starts from
# what an earlier one learned.
#
# Needs a nightly toolchain and cargo-fuzz:
#   rustup toolchain install nightly
#   cargo install cargo-fuzz

set -euo pipefail

target="${1:-}"
seconds="${2:-900}"

case "$target" in
lex | parse | frontend) ;;
*)
	echo "usage: scripts/fuzz.sh <lex|parse|frontend> [seconds]" >&2
	exit 2
	;;
esac

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

corpus="fuzz/corpus/$target"
mkdir -p "$corpus"

# Copied under a name that keeps the source directory, so a crash traced back to a seed says where
# that seed came from.
seed() {
	local directory="$1"
	local prefix="$2"

	if [ ! -d "$directory" ]; then
		return
	fi

	for file in "$directory"/*.c; do
		[ -e "$file" ] || continue
		cp "$file" "$corpus/$prefix-$(basename "$file")"
	done
}

seed tests/programs valid
seed tests/programs/invalid invalid
seed tests/adversarial adversarial
seed fuzz/regressions regression

echo "==> $target: $(find "$corpus" -type f | wc -l | tr -d ' ') seeds, running for ${seconds}s"

# -max_total_time bounds the run. -rss_limit_mb is libfuzzer's own ceiling: a target that allocates
# without bound is a finding, not something to let run until the machine gives out.
cargo +nightly fuzz run "$target" -- \
	-max_total_time="$seconds" \
	-rss_limit_mb=4096 \
	-print_final_stats=1
