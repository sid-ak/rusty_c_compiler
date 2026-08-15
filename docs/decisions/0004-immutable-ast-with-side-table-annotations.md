# ADR 0004 — An immutable AST, annotated through side tables

- Status: Accepted
- Date: 2026-08-10

## Context

Semantic analysis produces information the code generator needs: the type of every expression, what
each identifier resolves to, the stack inventory of each function. That information has to be
attached to the tree somehow.

The usual approach is a mutable field on each node — `expr.resolved_type = Some(ty)` — filled in
during analysis and read during code generation. It is the shortest path, and it has two costs. In
Rust it means threading mutable borrows through a recursive walk, which is where borrow checking
turns unpleasant. More importantly, it means the tree a parser test snapshots is not the tree that
exists after analysis, so parser snapshots have to be rewritten whenever a downstream pass starts
recording something new.

That second cost is not hypothetical here. The phases are built in order and each one's tests must
survive the next one landing: Phase 2 pins AST shape with snapshots, Phase 3 adds analysis, Phase 4
adds code generation. If Phase 3 mutates the tree, every Phase 2 snapshot churns.

## Decision

The parser builds the AST once and nothing mutates it afterwards. Each node carries a `NodeId`
assigned during construction. Semantic analysis returns a separate annotation structure keyed by
`NodeId`, holding resolved types, identifier bindings, per-function frame inventories, the interned
string-literal table, and the explicit promotion and decay nodes it inserts.

## Consequences

Parser snapshots taken in Phase 2 stay byte-identical after Phase 3 lands, because analysis cannot
change what the dumper sees. Test churn across phase boundaries goes to zero, which is what makes
the phased plan honest — an earlier phase's tests keep proving the same thing.

The code generator cannot accidentally depend on a mutation that analysis happened to perform. Its
only inputs are the AST and the annotation structure, both explicit parameters, so if analysis fails
to record something the backend needs, that is a missing lookup rather than a silently stale field.

Analysis output is a value, which means it can be dumped, snapshotted, and compared. The
"annotations are deterministic" test is possible only because they live in one inspectable structure.

The costs: a `NodeId` indirection on every lookup instead of a field access, and the discipline that
anything the backend needs must be explicitly recorded rather than quietly stashed on a node. Both
are cheap at this scale, and the second is a feature — it forces the interface between the two
phases to be written down.

## Alternatives considered

Mutable fields on AST nodes. Shortest path, and standard in compilers written in languages without
Rust's borrow rules. Rejected for the snapshot churn and the implicit coupling described above.

A typed AST as a separate tree — analysis consumes the untyped AST and produces a new tree with
types built in. This is what a larger compiler would do, and it makes illegal states unrepresentable
in the backend's input. Rejected as disproportionate here: it means writing and maintaining two
near-identical tree definitions and a full translation between them, for a subset with roughly a
dozen expression forms.

Interior mutability (`RefCell`) on nodes. Rejected: it keeps the snapshot problem, adds runtime
borrow panics to a codebase whose central invariant is that it does not panic, and hides the
mutation rather than removing it.
