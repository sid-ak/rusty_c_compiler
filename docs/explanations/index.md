# Explanations

One page per phase of [the plan](../PLAN.md), written after that phase was built: what it produced,
how each piece works, and why each choice was made rather than the obvious alternative.

These are for a reader with no background in Rust and none in compilers. Every term is defined the
first time it appears and then used normally, so the pages read start to finish without needing
anything alongside them — but nothing is simplified away either. Where a decision was hard, the
rejected alternative is named and the reason given; where something was got wrong the first time,
that is written down too, because the correction is usually the part worth reading.

They sit alongside the other documents rather than replacing any of them:

- [Architecture](../architecture.md) describes the system as it is designed, in the present tense
  and at the level of passes and the data flowing between them. It is the one to read first, and the
  one to keep current.
- [Decisions](../decisions/index.md) records single choices in isolation — what was picked, what was
  rejected, and why — and is binding on the code.
- [Implementation Plan](../PLAN.md) says what each phase is meant to deliver before it is built.
- These pages say what each phase actually delivered once it was, and narrate the reasoning across a
  whole phase rather than one decision at a time.

## Phases

- [Phase 1 — Foundation, diagnostics, and the lexer](Phase-1-Explanation.md): deciding what
  "correct" means and building the machinery to prove it, then turning source text into tokens.
- [Phase 2 — The AST and the recursive-descent parser](Phase-2-Explanation.md): turning that flat
  list of tokens into a tree that records what is nested inside what.
