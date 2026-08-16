# API reference

The compiler's own doc comments, rendered by `rustdoc`: every module, type, and function in `src/`,
with the reasoning that sits next to the code rather than in a design document.

[Browse the API reference](api/mycc/index.html)

This is the counterpart to [Architecture](architecture.md), not a replacement for it. The
architecture document explains what the compiler does and why it is shaped that way, at the level of
passes and data flowing between them. The API reference explains what a particular function
guarantees to its caller. A reader following how a diagnostic gets from the lexer to the terminal
wants the first; a reader about to call `SourceMap::render` wants the second.

Private items are included. Nearly all of a compiler is internal — the scanner, its resynchronization
rules, the frame layout later phases will add — and documenting only the public surface would leave
out the parts a reader most needs the comments for.

## Building it yourself

The published reference is rebuilt from `main`. To render the same pages from a working tree:

1. `scripts/build-docs.sh`: build the reference, stage it into `docs/api/`, and build the site
   around it, exactly as CI does. Run this before `uv run mkdocs serve`, or the reference 404s —
   `serve` only knows about files under `docs/`, and `docs/api/` is generated rather than committed.
2. `cargo doc --no-deps --document-private-items --open`: build only the reference and open it,
   without touching the site.

Doc comments are not optional here. `#![deny(missing_docs)]` makes an undocumented public item a
build failure, and CI builds the reference with `RUSTDOCFLAGS="-D warnings"`, which turns a link
pointing at an item that no longer exists into a failure too — the case the `missing_docs` lint
cannot catch, because the comment is present, just wrong.
