#!/usr/bin/env bash
#
# Build the crate's API reference and the documentation site that embeds it.
#
# The two are separate static sites: rustdoc renders the doc comments in src/, MkDocs renders the
# prose in docs/. rustdoc runs first and its output is staged into docs/api/, so MkDocs treats it as
# ordinary static content and copies it into the built site. Staging it there rather than merging it
# into site/ afterwards is what makes `mkdocs serve` work: serve builds to its own directory and
# only knows about files under docs/, so anything merged into site/ is invisible to it.
#
# docs/api/ is generated, and gitignored. Run this script once and `uv run mkdocs serve` will then
# serve the API reference at /api/ alongside the prose.
#
# CI runs this script rather than its own copy of these commands, so what CI checks and what you can
# reproduce locally cannot drift apart.
#
# Usage: scripts/build-docs.sh   (from anywhere)

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

echo "==> building the API reference"
# Warnings are failures: -D warnings catches a doc link pointing at an item that no longer exists.
# #![deny(missing_docs)] only catches a doc comment that is absent, not one that has gone stale, so
# this is the check that covers the difference.
#
# --document-private-items includes the scanner internals. They are the most intricate code in the
# repo and the part a reader most needs the comments for, and hiding them would document only the
# public surface of a compiler that is almost entirely private.
#
# target/doc is cleared first because rustdoc adds pages without ever removing them: a renamed or
# deleted module leaves its old page behind, and the stage below would copy that stale page into
# the published site. Rebuilding from empty costs a few seconds and makes the output describe only
# what the crate currently contains.
rm -rf target/doc
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items

echo "==> staging it into docs/api"
rm -rf docs/api
cp -R target/doc docs/api

# docs/api.md renders to /api/ and so replaces rustdoc's own landing page, which is no loss: that
# page only lists crates, while docs/api.md explains what the reference is and links into the crate
# at /api/rustycc/. Nothing here should write docs/api/index.html — MkDocs would overwrite it.

echo "==> building the prose site"
# --strict catches a broken internal link or a page missing from the nav. Because the API reference
# is staged above, docs/api.md's link to it is checked here too rather than being taken on trust.
#
# --with-requirements resolves the pinned docs dependencies itself, so this works on a clean
# checkout with no virtualenv — a plain `uv run mkdocs` finds nothing to spawn there, which is how
# it failed in CI while passing locally off a venv that happened to exist. Naming the requirements
# file rather than installing mkdocs in the workflow keeps the theme and plugin versions in one
# place: mkdocs.yml uses the material theme and the exclude plugin, so mkdocs alone is not enough.
# It is also non-destructive, layering onto whatever environment is active instead of creating or
# overwriting a .venv the user manages.
uv run --with-requirements requirements-docs.txt mkdocs build --strict

test -f site/index.html || {
    echo "error: the prose site did not build" >&2
    exit 1
}
test -f site/api/rustycc/index.html || {
    echo "error: the API reference did not reach site/api" >&2
    exit 1
}

echo "==> done: site/ holds the whole site; 'uv run mkdocs serve' now serves /api/ too"
