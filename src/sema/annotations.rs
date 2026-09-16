//! What analysis records, and the only thing code generation reads besides the AST itself.
//!
//! The AST is never touched. Everything analysis learns is filed here against the [`NodeId`] of
//! the node it learned it about, which is the arrangement ADR 0004 requires: a parser snapshot
//! taken before analysis stays byte-identical after it, and the backend cannot depend on a
//! mutation that happened to have been made.
//!
//! Tables are ordered by node id rather than hashed, so dumping them twice produces the same text
//! both times. An annotation set that could not be compared against itself would be difficult to
//! test and impossible to snapshot.

use std::collections::BTreeMap;

use crate::ast::NodeId;
use crate::sema::scope::{Scopes, Symbol, SymbolId};
use crate::sema::types::Ty;

/// Everything analysis learned about one program.
#[derive(Debug, Clone)]
pub struct Annotations {
    /// The type of each expression node.
    types: BTreeMap<NodeId, Ty>,
    /// The symbol each identifier expression resolves to.
    bindings: BTreeMap<NodeId, SymbolId>,
    /// Every symbol declared anywhere in the program.
    symbols: Scopes,
}

impl Annotations {
    /// An empty set, over `symbols`.
    pub(super) fn new(symbols: Scopes) -> Self {
        Self {
            types: BTreeMap::new(),
            bindings: BTreeMap::new(),
            symbols,
        }
    }

    /// Records that the expression `node` has type `ty`.
    pub(super) fn record_type(&mut self, node: NodeId, ty: Ty) {
        self.types.insert(node, ty);
    }

    /// Records that the identifier `node` resolves to `symbol`.
    pub(super) fn record_binding(&mut self, node: NodeId, symbol: SymbolId) {
        self.bindings.insert(node, symbol);
    }

    /// Replaces the symbol table, once the walk that built it has finished.
    pub(super) fn set_symbols(&mut self, symbols: Scopes) {
        self.symbols = symbols;
    }

    /// The type of the expression `node`, or `None` if analysis recorded none.
    ///
    /// `None` from a program that analysis accepted is a bug in analysis rather than a fact about
    /// the program: every expression it walked has a type.
    pub fn type_of(&self, node: NodeId) -> Option<&Ty> {
        self.types.get(&node)
    }

    /// The symbol the identifier `node` resolves to, or `None` if it is not an identifier.
    pub fn binding_of(&self, node: NodeId) -> Option<SymbolId> {
        self.bindings.get(&node).copied()
    }

    /// The symbol `id` names.
    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.symbol(id)
    }
}
