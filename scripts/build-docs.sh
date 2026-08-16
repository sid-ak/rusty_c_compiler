#!/usr/bin/env bash
#
# Build the documentation site and the crate's API docs, and stitch them into one directory.
#
# The two are separate static sites: MkDocs renders the prose in docs/, rustdoc renders the doc
# comments in src/. This puts rustdoc's output under the site's /api/ path so a single directory can
# be served or published, and so the API reference is one click from the prose rather than something
# a reader has to build for themselves.
#
# CI runs this script rather than its own copy of these commands, so what CI checks and what you can
# reproduce locally cannot drift apart.
#
# Usage: scripts/build-docs.sh   (from the repo root)

set -euo pipefail

cd "$(dirname "$0")/.."

# rustup and uv both write their PATH line into the shell's interactive startup file, so a script
# launched from an editor, a task runner, or `sh -c` can find itself without them even though an
# interactive terminal has them. Add their default install locations before giving up.
for directory in "$HOME/.cargo/bin" "$HOME/.local/bin" /opt/homebrew/bin; do
    case ":$PATH:" in
    *":$directory:"*) ;;
    *) [ -d "$directory" ] && PATH="$PATH:$directory" ;;
    esac
done
export PATH

for tool in cargo uv; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "error: '$tool' is not on PATH." >&2
        case "$tool" in
        cargo) echo "  Install it with: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2 ;;
        uv) echo "  Install it with: brew install uv" >&2 ;;
        esac
        exit 1
    }
done

# Warnings are failures on both halves: mkdocs --strict catches a broken internal link or a page
# missing from the nav, and -D warnings catches a rustdoc link pointing at an item that no longer
# exists. #![deny(missing_docs)] only catches a doc comment that is absent, not one that has gone
# stale, so this is the check that covers the difference.
export RUSTDOCFLAGS="-D warnings"

echo "==> building the prose site"
uv run mkdocs build --strict

echo "==> building the API reference"
# --document-private-items includes the scanner internals. They are the most intricate code in the
# repo and the part a reader most needs the comments for, and hiding them would document only the
# public surface of a compiler that is almost entirely private.
cargo doc --no-deps --document-private-items

echo "==> stitching the API reference into site/api"
rm -rf site/api
cp -R target/doc site/api

# Rustdoc's own landing page lists every crate including dependencies; the crate's page is the one
# worth linking to, so make it the entry point for /api/.
cat > site/api/index.html <<'HTML'
<!doctype html>
<meta charset="utf-8">
<title>API reference — Rusty C Compiler</title>
<meta http-equiv="refresh" content="0; url=mycc/index.html">
<link rel="canonical" href="mycc/index.html">
<p>Redirecting to <a href="mycc/index.html">the mycc crate documentation</a>.</p>
HTML

test -f site/index.html || { echo "error: the prose site did not build" >&2; exit 1; }
test -f site/api/mycc/index.html || { echo "error: the API reference did not land in site/api" >&2; exit 1; }

echo "==> done: site/ holds the prose site, site/api/ holds the API reference"
