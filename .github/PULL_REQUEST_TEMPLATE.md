<!--
Title format: [<module>] <Title> — e.g. [sema] Add scope stack and symbol table.
GitHub cannot prefill the title from a template, so set it by hand.
-->

## Summary

<!--
Lead with what is novel or non-obvious, not the scaffolding. Cite commit short-hashes
inline, e.g. (4fa2205), so each claim is traceable to the change that made it.

Say why a non-obvious choice was made, not only what changed — a reviewer can read the
diff for what. Skip a separate "Changes" section; if the summary is doing its job, a
restatement of the diff adds nothing.
-->

## Verification

<!--
A numbered list, each item leading with the exact command in backticks, then a colon and
one short clause on what it does or what to expect. Nest related commands under a parent
step. Replace the placeholders below with what you actually ran.
-->

1. `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && uv run mkdocs build --strict`:
   the full gate from `AGENTS.md`, green.
2. `cargo test --lib <filter>`: the tests this change adds, named.

## Invariants

<!--
Restated from AGENTS.md deliberately: this is where they actually get checked, and a
reviewer will not open AGENTS.md to remember them. Delete any line this change cannot
affect rather than leaving it unticked.
-->

- [ ] No pass panics on user input.
- [ ] The AST is not mutated after parsing.
- [ ] The code generator does no type reasoning.
- [ ] A change to the accepted language changed the grammar in `docs/architecture.md` first.
- [ ] Every new diagnostic has a test provoking it and asserting its message and span.
- [ ] New programs in `tests/programs/` have their `COVERAGE.md` row.
- [ ] `README.md`'s status section describes what is now true.
- [ ] Snapshots were read before being accepted.

Closes #
