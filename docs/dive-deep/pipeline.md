# Pipeline Invariants

Two structural rules hold across every pass in the pipeline. Neither is a style preference — both are
invariants that the test suite actively checks for.

## Immutable AST

The parser builds the tree once, and nothing edits it afterwards. Semantic
analysis does not annotate nodes in place; it produces a separate annotation structure keyed by node
id (`NodeId → data`, e.g. `NodeId → Type`) instead. The practical payoff: an AST snapshot taken in a
parser test stays byte-identical after semantic analysis is added in a later phase, so parser tests
never need rewriting because a downstream pass changed. It also makes it impossible for the code
generator to accidentally depend on a mutation the analyzer happened to perform — its only inputs are
the tree and the annotation structure, both explicit parameters. The full rationale is [ADR
0004](decisions/0004-immutable-ast-with-side-table-annotations.md).

## No pass panics

Every failure is a `Diagnostic` with a span, collected in a shared
`DiagnosticBag` and rendered with a caret under the offending source. A pass reports as many errors
as it can find rather than stopping at the first — a compiler that reports one error per run is
unusable to actually work with. This invariant is what Phase 5's fuzzer exists to violate: raw bytes
go into the lexer, the parser, and semantic analysis, and the only acceptable outcomes are
diagnostics or success. A crash or a hang is a bug, full stop.

Both invariants are enforced in code, not just in review: `unwrap`, `expect`, `panic!`, and unchecked
indexing are denied by clippy lints in `src/`, and the AST type has no `&mut` accessors reachable
after parsing completes.
