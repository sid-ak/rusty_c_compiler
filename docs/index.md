# Rusty C Compiler

A compiler for a well-defined subset of C, written in Rust, emitting ARM64 assembly for Apple
Silicon macOS and linking it into a real native executable. Because the input is C rather than a
purpose-built toy language, `clang` serves directly as the testing oracle.

## Where to start

<div class="grid cards" markdown>

- [Architecture](architecture.md) — the design: the four passes, the language subset grammar, the
  code generation strategy, and how the whole thing is tested. Start here.

- [Implementation Plan](PLAN.md) — five phases, each with deliverables, tests, and exit criteria,
  mapped to the GitHub issues that track them.

- [Decisions](decisions/index.md) — the ADRs. What was chosen, what was rejected, and why.

- [Proposal](PROPOSAL.md) — the original project proposal, kept as written.

</div>

## Status

Planning complete; implementation has not started. Progress is tracked in the
[GitHub issues](https://github.com/sid-ak/rusty_c_compiler/issues) as five milestones, one per
phase, each with an epic issue holding its task checklist.
