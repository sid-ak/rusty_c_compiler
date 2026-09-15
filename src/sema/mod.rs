//! Semantic analysis: the pass between parsing and code generation.
//!
//! Analysis answers every type question in the program exactly once and records the answers, so
//! the code generator performs no type reasoning of its own. The design is in
//! `docs/dive-deep/semantic-analysis.md`; the rule that the answers live beside the AST rather
//! than in it is ADR 0004.

pub mod scope;
pub mod types;
