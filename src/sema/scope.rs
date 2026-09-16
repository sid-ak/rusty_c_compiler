//! The scope stack and symbol table that resolve every identifier.
//!
//! A C program's names are resolved by nesting, not by search order: the innermost declaration of
//! a name wins, and when its block ends the one it was hiding becomes visible again. That is the
//! whole of this module. It keeps a stack of scopes, each holding the symbols declared directly in
//! it, and resolves a name by walking the stack outward.
//!
//! Where the scopes are pushed is what gives C its shape:
//!
//! - File scope, depth 0, holds globals and functions together. A function is an ordinary symbol
//!   there, shadowed by an inner declaration of the same name like any other.
//! - A function body pushes one scope, and its parameters are declared in that scope rather than
//!   in one of their own. `int f(int a) { int a; }` is therefore a redeclaration, which is what
//!   `clang -std=c99` reports; a nested block may still shadow the parameter.
//! - A `for` init clause pushes a scope that encloses the loop body, so `for (int i = 0; ...)`
//!   leaves no `i` behind after the loop and the body can still see it.
//!
//! Symbols are never removed. Leaving a scope makes a name stop resolving, but the entry stays in
//! the table, because the annotations recorded during the walk are read back by code generation
//! long after the scope that produced them closed.
//!
//! This module reports a conflict and nothing more. Turning one into a worded diagnostic is the
//! analyzer's job, so every message the compiler emits is written in one place.

use crate::diagnostics::Span;
use crate::sema::types::Ty;

/// A symbol's identity in the table, stable for the lifetime of the analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(u32);

/// A local's or parameter's index into its function's frame.
///
/// The number is an ordinal, not an offset: it says which slot, and the code generator decides
/// where that slot sits once it knows the whole frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SlotId(pub u32);

/// What kind of thing a symbol names, and where the code generator will find it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// A function, declared or defined at file scope.
    Function,
    /// A variable at file scope, addressed by its symbol name.
    Global,
    /// A parameter, with its zero-based position in the parameter list.
    Parameter(u32),
    /// A variable declared inside a function body.
    Local,
}

/// One declared name, and everything a later pass needs to know about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// The identifier as it was written.
    pub name: String,
    /// The type it was declared with.
    pub ty: Ty,
    /// What it names.
    pub kind: SymbolKind,
    /// Where it was declared, so a later conflict can point back at it.
    pub span: Span,
    /// Its frame slot, for a local or a parameter; `None` for a global or a function.
    pub slot: Option<SlotId>,
}

/// A stack of scopes, innermost last, over one shared symbol table.
#[derive(Debug, Clone)]
pub struct Scopes {
    /// Every symbol ever declared, indexed by [`SymbolId`]. Entries are never removed.
    symbols: Vec<Symbol>,
    /// The scope stack. `scopes[0]` is file scope and is never popped.
    scopes: Vec<Vec<SymbolId>>,
    /// The next frame slot to hand out, restarted at each function.
    next_slot: u32,
}

impl Scopes {
    /// A stack holding nothing, positioned at file scope.
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            scopes: vec![Vec::new()],
            next_slot: 0,
        }
    }

    /// How deeply nested the current scope is, counting file scope as zero.
    pub fn depth(&self) -> usize {
        self.scopes.len().saturating_sub(1)
    }

    /// Opens a scope for a block, a `for` init clause, or anything else that nests.
    pub fn enter_block(&mut self) {
        self.scopes.push(Vec::new());
    }

    /// Closes the innermost scope.
    ///
    /// Leaving file scope is refused rather than underflowing: a caller whose enters and leaves do
    /// not balance gets a stack that is still usable, not a panic.
    pub fn leave_block(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// Opens a function body's scope and restarts frame-slot numbering.
    ///
    /// Parameters are declared into this scope, not a separate one, because C puts them in the
    /// body's scope.
    pub fn enter_function(&mut self) {
        self.next_slot = 0;
        self.enter_block();
    }

    /// Closes a function body's scope.
    pub fn leave_function(&mut self) {
        self.leave_block();
    }

    /// Declares `name` in the innermost scope.
    ///
    /// Returns the new symbol, or `Err` naming the declaration already in this scope. A conflict
    /// changes nothing: the binding that was there stays exactly as it was, so analysis continues
    /// against the first declaration rather than against a half-replaced one.
    pub fn declare(
        &mut self,
        name: &str,
        ty: Ty,
        kind: SymbolKind,
        span: Span,
    ) -> Result<SymbolId, SymbolId> {
        if let Some(existing) = self.lookup_in_current_scope(name) {
            return Err(existing);
        }

        let slot = match kind {
            SymbolKind::Parameter(_) | SymbolKind::Local => {
                let slot = SlotId(self.next_slot);
                // Saturating rather than wrapping: a function with `u32::MAX` locals is not
                // reachable from any file the parser accepts, and a wrapped counter would hand
                // two locals the same slot instead of failing loudly.
                self.next_slot = self.next_slot.saturating_add(1);
                Some(slot)
            }
            SymbolKind::Global | SymbolKind::Function => None,
        };

        let Ok(index) = u32::try_from(self.symbols.len()) else {
            // More than `u32::MAX` symbols in one file. Unreachable in practice, and reporting the
            // conflict with the last symbol is still an answer rather than a panic.
            return Err(SymbolId(u32::MAX));
        };

        let id = SymbolId(index);
        self.symbols.push(Symbol {
            name: name.to_owned(),
            ty,
            kind,
            span,
            slot,
        });

        if let Some(scope) = self.scopes.last_mut() {
            scope.push(id);
        }

        Ok(id)
    }

    /// The symbol `name` resolves to, walking outward from the innermost scope.
    ///
    /// This is the only path by which an identifier resolves. The scan is linear over each scope,
    /// which is the right shape here: scopes in this subset hold a handful of names, and a vector
    /// keeps declaration order, which the frame inventory later depends on.
    pub fn lookup(&self, name: &str) -> Option<SymbolId> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| self.find_in(scope, name))
    }

    /// The symbol `name` resolves to in the innermost scope alone, ignoring everything outside it.
    pub fn lookup_in_current_scope(&self, name: &str) -> Option<SymbolId> {
        self.scopes
            .last()
            .and_then(|scope| self.find_in(scope, name))
    }

    /// The symbol `id` names, or `None` if it names none.
    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        let index = usize::try_from(id.0).ok()?;

        self.symbols.get(index)
    }

    /// The symbol named `name` within one scope's entries.
    fn find_in(&self, scope: &[SymbolId], name: &str) -> Option<SymbolId> {
        scope
            .iter()
            .rev()
            .copied()
            .find(|id| self.symbol(*id).is_some_and(|symbol| symbol.name == name))
    }
}

impl Default for Scopes {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
