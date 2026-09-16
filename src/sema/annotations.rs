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
use std::fmt::Write as _;

use crate::ast::NodeId;
use crate::sema::scope::{Scopes, SlotId, Symbol, SymbolId, SymbolKind};
use crate::sema::types::{Conversion, Ty};

/// One local or parameter, with everything needed to give it a place in the frame.
///
/// Size and alignment are resolved here rather than in the backend, so laying out a frame is
/// arithmetic over this list instead of a second pass over the types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameSlot {
    /// Which slot of this function's frame.
    pub slot: SlotId,
    /// The name it was declared with, for readable assembly and readable dumps.
    pub name: String,
    /// What it holds.
    pub ty: Ty,
    /// How many bytes it occupies.
    pub size: u64,
    /// What boundary it has to start on.
    pub align: u64,
    /// Whether it is a parameter, and if so which one.
    pub kind: SymbolKind,
}

/// Everything one function needs storage for, in declaration order.
///
/// Declaration order rather than any cleverer arrangement: it is deterministic, it is what the
/// source reads like, and packing is the backend's decision to make from this list if it wants to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    /// Every local and parameter of the function.
    pub slots: Vec<FrameSlot>,
}

/// One distinct string literal and the label its bytes live under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StringLiteral {
    /// The decoded bytes, without the terminator the backend appends.
    pub bytes: Vec<u8>,
    /// The assembly label they are emitted under.
    pub label: String,
}

/// Everything analysis learned about one program.
#[derive(Debug, Clone)]
pub struct Annotations {
    /// The type of each expression node.
    types: BTreeMap<NodeId, Ty>,
    /// The symbol each identifier expression resolves to.
    bindings: BTreeMap<NodeId, SymbolId>,
    /// The implicit conversion applied to an expression's value, where one is.
    conversions: BTreeMap<NodeId, Conversion>,
    /// The label each string-literal expression's bytes interned to.
    literals: BTreeMap<NodeId, String>,
    /// What each defined function needs storage for.
    frames: BTreeMap<String, Frame>,
    /// Every distinct string literal, in the order they were first seen.
    strings: Vec<StringLiteral>,
    /// Every symbol declared anywhere in the program.
    symbols: Scopes,
}

impl Annotations {
    /// An empty set, over `symbols`.
    pub(super) fn new(symbols: Scopes) -> Self {
        Self {
            types: BTreeMap::new(),
            bindings: BTreeMap::new(),
            conversions: BTreeMap::new(),
            literals: BTreeMap::new(),
            frames: BTreeMap::new(),
            strings: Vec::new(),
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

    /// Records that `node`'s value is converted before it is used.
    pub(super) fn record_conversion(&mut self, node: NodeId, conversion: Conversion) {
        self.conversions.insert(node, conversion);
    }

    /// Records `frame` as what the function `name` needs storage for.
    pub(super) fn record_frame(&mut self, name: &str, frame: Frame) {
        self.frames.insert(name.to_owned(), frame);
    }

    /// Interns `bytes`, recording the label against `node` and returning it.
    ///
    /// Two occurrences of one literal share an entry, so the read-only data section holds one copy
    /// however many times the program writes it.
    pub(super) fn intern_string(&mut self, node: NodeId, bytes: &[u8]) {
        let existing = self
            .strings
            .iter()
            .find(|literal| literal.bytes == bytes)
            .map(|literal| literal.label.clone());

        let label = existing.unwrap_or_else(|| {
            let label = format!("l_.str.{}", self.strings.len());
            self.strings.push(StringLiteral {
                bytes: bytes.to_vec(),
                label: label.clone(),
            });

            label
        });

        self.literals.insert(node, label);
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

    /// The conversion applied to `node`'s value, or `None` if it is used as it stands.
    pub fn conversion_of(&self, node: NodeId) -> Option<Conversion> {
        self.conversions.get(&node).copied()
    }

    /// The type `node`'s value has after any conversion recorded against it.
    ///
    /// This is what the backend actually receives at the use site, as against
    /// [`type_of`](Annotations::type_of), which is what the source wrote.
    pub fn converted_type_of(&self, node: NodeId) -> Option<Ty> {
        let written = self.type_of(node)?;

        Some(match self.conversion_of(node) {
            Some(Conversion::PromoteCharToInt) => Ty::Int,
            Some(Conversion::TruncateIntToChar) => Ty::Char,
            Some(Conversion::DecayArrayToPtr) => written.decayed(),
            None => written.clone(),
        })
    }

    /// The label the string literal at `node` interned to.
    pub fn string_label(&self, node: NodeId) -> Option<&str> {
        self.literals.get(&node).map(String::as_str)
    }

    /// Every distinct string literal in the program, in the order they were first seen.
    pub fn strings(&self) -> &[StringLiteral] {
        &self.strings
    }

    /// What the function `name` needs storage for.
    pub fn frame(&self, name: &str) -> Option<&Frame> {
        self.frames.get(name)
    }

    /// The whole annotation set as text, for snapshots and for `--dump-annotations`.
    ///
    /// Every table is ordered, so two runs over one program produce the same string. That is what
    /// makes this comparable against itself, which is the only way it can be snapshotted at all.
    pub fn dump(&self) -> String {
        let mut dumped = String::new();

        dumped.push_str("types\n");
        for (node, ty) in &self.types {
            let _ = writeln!(dumped, "  #{} {ty}", node.index());
        }

        dumped.push_str("conversions\n");
        for (node, conversion) in &self.conversions {
            let _ = writeln!(dumped, "  #{} {}", node.index(), describe(*conversion));
        }

        dumped.push_str("bindings\n");
        for (node, symbol) in &self.bindings {
            let Some(symbol) = self.symbols.symbol(*symbol) else {
                continue;
            };
            let _ = writeln!(
                dumped,
                "  #{} {} {}",
                node.index(),
                describe_kind(symbol.kind),
                symbol.name
            );
        }

        dumped.push_str("frames\n");
        for (name, frame) in &self.frames {
            let _ = writeln!(dumped, "  {name}");
            for slot in &frame.slots {
                let _ = writeln!(
                    dumped,
                    "    {} {} {} size {} align {}",
                    slot.slot.0,
                    describe_kind(slot.kind),
                    slot.name,
                    slot.size,
                    slot.align
                );
            }
        }

        dumped.push_str("strings\n");
        for literal in &self.strings {
            let _ = writeln!(
                dumped,
                "  {} {:?}",
                literal.label,
                String::from_utf8_lossy(&literal.bytes)
            );
        }

        dumped
    }
}

/// How a conversion is written in a dump.
fn describe(conversion: Conversion) -> &'static str {
    match conversion {
        Conversion::PromoteCharToInt => "char -> int",
        Conversion::TruncateIntToChar => "int -> char",
        Conversion::DecayArrayToPtr => "array -> pointer",
    }
}

/// How a symbol's kind is written in a dump.
fn describe_kind(kind: SymbolKind) -> String {
    match kind {
        SymbolKind::Function => "function".to_owned(),
        SymbolKind::Global => "global".to_owned(),
        SymbolKind::Parameter(index) => format!("param[{index}]"),
        SymbolKind::Local => "local".to_owned(),
    }
}
