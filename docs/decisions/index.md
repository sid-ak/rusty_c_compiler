# Architectural Decision Records

Each ADR records one decision: the situation that forced it, what was chosen, what follows from
that, and what was rejected. They are the source of truth for the decisions they cover — the
architecture describes how the system works, the ADRs record why it works that way.

They are binding. Read the relevant record before changing what it governs. A decision that turns
out to be wrong is superseded by a new ADR rather than edited away, so the reasoning that led here
stays legible.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](0001-subset-of-c-with-clang-as-oracle.md) | Compile a subset of C, with clang as the testing oracle | Accepted |
| [0002](0002-rust-as-implementation-language.md) | Rust as the implementation language | Accepted |
| [0003](0003-single-target-arm64-macos.md) | One target: ARM64 macOS, with no portability layer | Accepted |
| [0004](0004-immutable-ast-with-side-table-annotations.md) | An immutable AST, annotated through side tables | Accepted |
| [0005](0005-stack-spilling-instead-of-register-allocation.md) | Stack spilling instead of register allocation | Accepted |
| [0006](0006-fixed-arity-runtime-shim.md) | A fixed-arity runtime shim instead of printf | Accepted |
| [0007](0007-array-decay-only-at-parameter-boundary.md) | Array-to-pointer decay only at the function-parameter boundary | Accepted |
| [0008](0008-reject-undefined-behavior.md) | Reject undefined behavior rather than admit it | Accepted |
| [0009](0009-clang-as-assembler-and-linker.md) | Drive the toolchain through clang, not as and ld | Accepted |

## Writing a new one

Copy the shape of an existing record: a title line `# ADR NNNN — Title`, a status and date, then
Context, Decision, Consequences, and Alternatives considered. Number sequentially. Add a row to the
table above and a nav entry in `mkdocs.yml`.

The Alternatives section is the part that earns the record. A decision with no rejected options was
not a decision, and the next person will otherwise re-derive the same dead ends.
