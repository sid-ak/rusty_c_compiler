//! Expression lowering: every expression leaves its value in `w0`.
//!
//! One shape runs through all of it. A binary operator evaluates its left side into `w0`, spills
//! that to this node's own temporary, evaluates its right side into `w0`, moves it to `w1`, and
//! reloads the left side into `w0`. Every instruction then reads the left operand in `w0` and the
//! right in `w1`, in the order the source wrote them.
//!
//! The `mov` in the middle costs one instruction and is the reason the rest of this module is dull.
//! Reloading the left operand straight into `w1` would work and would leave every non-commutative
//! instruction reading its operands backwards, so `sub`, `sdiv`, `msub` and every condition code
//! would each have to be written crossed. Both forms are correct if written carefully; only one of
//! them fails loudly when it is not, because a transposed lowering still gives the right answer for
//! `2 - 2` and `a < a`.
//!
//! Two registers beyond `w0` and `w1` are used, never across the evaluation of anything else:
//! `w2`/`x2` for a value an instruction needs a third place for, and `x9` for an address or an
//! immediate too large for a field, named in [`crate::codegen::emit::SCRATCH`].

use crate::ast::{BinOp, Expr, ExprKind, IncDec, UnOp};
use crate::codegen::emit::Width;
use crate::codegen::{element_stride, Generator};
use crate::diagnostics::Span;
use crate::sema::scope::SymbolKind;
use crate::sema::types::{Conversion, Ty};

/// A second scratch register, live only within the instruction pair that uses it.
const SECOND: &str = "2";

impl Generator<'_> {
    /// Lowers `expr`, leaving its value in `w0`.
    pub(crate) fn expr(&mut self, expr: &Expr) {
        // An array at an argument position was recorded as decaying, which means its address is
        // the value. Analysis decided that; this only carries it out.
        if self.annotations.conversion_of(expr.id) == Some(Conversion::DecayArrayToPtr) {
            self.address_of(expr);

            return;
        }

        match &expr.kind {
            ExprKind::IntLit(value) => self.emitter.load_word_immediate("w0", *value),
            ExprKind::CharLit(byte) => self.emitter.load_word_immediate("w0", i32::from(*byte)),
            ExprKind::StrLit(_) => self.string_literal(expr),
            ExprKind::Ident(_) => self.read_lvalue(expr),
            ExprKind::Index { .. } => self.read_lvalue(expr),
            ExprKind::Unary { op, operand } => self.unary(*op, operand, expr),
            ExprKind::Binary { op, left, right } => self.binary(*op, left, right, expr),
            ExprKind::Assign { target, value } => self.assign(target, value, expr),
            ExprKind::PostfixIncDec { op, operand } => self.postfix(*op, operand, expr),
            ExprKind::Call { .. } => self.unlowered(expr.span, "a function call"),
        }
    }

    /// Lowers `expr` one nesting level deeper, so it takes its own temporary slot.
    fn deeper(&mut self, expr: &Expr) {
        self.depth = self.depth.saturating_add(1);
        self.expr(expr);
        self.depth = self.depth.saturating_sub(1);
    }

    /// The temporary slot belonging to the expression currently being lowered.
    fn temporary(&mut self, span: Span) -> Option<u64> {
        let slot = self.layout.temporary(self.depth);
        if slot.is_none() {
            self.unlowered(
                span,
                "an expression deeper than its frame has temporaries for",
            );
        }

        slot
    }

    /// The six-step shape every binary operator shares, then the operator's own instruction.
    fn binary(&mut self, op: BinOp, left: &Expr, right: &Expr, whole: &Expr) {
        if matches!(op, BinOp::And | BinOp::Or) {
            self.short_circuit(op, left, right);

            return;
        }

        let Some(temp) = self.temporary(whole.span) else {
            return;
        };

        self.deeper(left);
        self.emitter.store_to_frame("w0", Width::Word, temp);
        self.deeper(right);
        self.emitter.instruction("mov w1, w0");
        self.emitter.load_from_frame("w0", Width::Word, temp);

        match op {
            BinOp::Add => self.emitter.instruction("add w0, w0, w1"),
            BinOp::Subtract => self.emitter.instruction("sub w0, w0, w1"),
            BinOp::Multiply => self.emitter.instruction("mul w0, w0, w1"),
            BinOp::Divide => self.emitter.instruction("sdiv w0, w0, w1"),
            // There is no remainder instruction. `msub wd, wn, wm, wa` computes `wa - wn * wm`, so
            // the accumulator is the left operand and the quotient times the divisor comes off it.
            BinOp::Remainder => {
                self.emitter.instruction(&format!("sdiv w{SECOND}, w0, w1"));
                self.emitter
                    .instruction(&format!("msub w0, w{SECOND}, w1, w0"));
            }
            BinOp::Equal
            | BinOp::NotEqual
            | BinOp::Less
            | BinOp::Greater
            | BinOp::LessEqual
            | BinOp::GreaterEqual => {
                if let Some(code) = condition_code(op) {
                    self.emitter.instruction("cmp w0, w1");
                    self.emitter.instruction(&format!("cset w0, {code}"));
                }
            }
            // Handled above, before any operand was evaluated.
            BinOp::And | BinOp::Or => {}
        }
    }

    /// Lowers `&&` or `||` to real branches, so the right operand is genuinely skipped.
    ///
    /// The result is normalized to `0` or `1` rather than being whichever operand decided it, which
    /// is what C says these operators produce.
    fn short_circuit(&mut self, op: BinOp, left: &Expr, right: &Expr) {
        let and = op == BinOp::And;
        let name = if and { "and" } else { "or" };
        let decided = self.emitter.new_label(&format!("{name}_decided"));
        let end = self.emitter.new_label(&format!("{name}_end"));

        for operand in [left, right] {
            self.deeper(operand);
            if and {
                self.emitter.branch_if_zero("w0", &decided);
            } else {
                self.emitter.branch_if_nonzero("w0", &decided);
            }
        }

        // Both operands agreed with the operator's identity: true for `&&`, false for `||`.
        self.emitter
            .instruction(if and { "mov w0, #1" } else { "mov w0, #0" });
        self.emitter.branch(&end);

        self.emitter.place_label(&decided);
        self.emitter
            .instruction(if and { "mov w0, #0" } else { "mov w0, #1" });
        self.emitter.place_label(&end);
    }

    /// Lowers a prefix operator.
    fn unary(&mut self, op: UnOp, operand: &Expr, whole: &Expr) {
        match op {
            UnOp::Plus => self.deeper(operand),
            UnOp::Negate => {
                self.deeper(operand);
                self.emitter.instruction("neg w0, w0");
            }
            UnOp::Not => {
                self.deeper(operand);
                self.emitter.instruction("cmp w0, #0");
                self.emitter.instruction("cset w0, eq");
            }
            UnOp::PreIncrement => self.step(operand, whole, 1, false),
            UnOp::PreDecrement => self.step(operand, whole, -1, false),
        }
    }

    /// Lowers a postfix `++` or `--`, which yields the value from before the step.
    fn postfix(&mut self, op: IncDec, operand: &Expr, whole: &Expr) {
        let by = match op {
            IncDec::Increment => 1,
            IncDec::Decrement => -1,
        };

        self.step(operand, whole, by, true);
    }

    /// Adds `by` to the value at `target`, leaving the old value in `w0` if `yield_old`.
    ///
    /// The address is computed once and kept in a temporary across the load, because evaluating it
    /// twice would evaluate any index expression inside it twice as well.
    fn step(&mut self, target: &Expr, whole: &Expr, by: i32, yield_old: bool) {
        let Some(temp) = self.temporary(whole.span) else {
            return;
        };
        let width = self.width_of(target.id);

        self.deeper_address(target);
        self.emitter.store_to_frame("x0", Width::Double, temp);
        self.emitter
            .instruction(&format!("{} w0, [x0]", width.load()));

        // The new value goes to `w1` so that `w0` still holds the old one, which is the only
        // difference between the prefix and postfix forms. The step is written as the instruction
        // that means it rather than as an `add` of a negative immediate, which assembles but reads
        // as the opposite of what it does.
        if by < 0 {
            self.emitter
                .instruction(&format!("sub w1, w0, #{}", by.saturating_neg()));
        } else {
            self.emitter.instruction(&format!("add w1, w0, #{by}"));
        }

        self.emitter
            .load_from_frame(&format!("x{SECOND}"), Width::Double, temp);
        self.emitter
            .instruction(&format!("{} w1, [x{SECOND}]", width.store()));

        if !yield_old {
            self.emitter.instruction("mov w0, w1");
        }
    }

    /// Lowers an assignment, leaving the assigned value in `w0`.
    fn assign(&mut self, target: &Expr, value: &Expr, whole: &Expr) {
        let Some(temp) = self.temporary(whole.span) else {
            return;
        };
        let width = self.width_of(target.id);

        self.deeper_address(target);
        self.emitter.store_to_frame("x0", Width::Double, temp);
        self.deeper(value);
        self.emitter
            .load_from_frame(&format!("x{SECOND}"), Width::Double, temp);
        self.emitter
            .instruction(&format!("{} w0, [x{SECOND}]", width.store()));
    }

    /// Reads the value at an lvalue into `w0`.
    fn read_lvalue(&mut self, expr: &Expr) {
        let width = self.width_of(expr.id);
        self.address_of(expr);
        self.emitter
            .instruction(&format!("{} w0, [x0]", width.load()));
    }

    /// Computes an lvalue's address one nesting level deeper.
    fn deeper_address(&mut self, expr: &Expr) {
        self.depth = self.depth.saturating_add(1);
        self.address_of(expr);
        self.depth = self.depth.saturating_sub(1);
    }

    /// Puts the address of `expr` into `x0`.
    pub(crate) fn address_of(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Ident(name) => self.address_of_name(expr, name),
            ExprKind::Index { base, index } => self.address_of_element(base, index, expr),
            ExprKind::StrLit(_) => self.string_literal(expr),
            _ => self.unlowered(expr.span, "an address of this expression"),
        }
    }

    /// Puts the address the name `expr` refers to into `x0`.
    fn address_of_name(&mut self, expr: &Expr, name: &str) {
        let Some(symbol) = self
            .annotations
            .binding_of(expr.id)
            .and_then(|id| self.annotations.symbol(id))
        else {
            self.unlowered(expr.span, "a name with no binding");

            return;
        };
        let kind = symbol.kind;
        let slot = symbol.slot;
        let is_pointer = matches!(symbol.ty, Ty::Ptr(_));

        match kind {
            SymbolKind::Global | SymbolKind::Function => {
                let mangled = crate::codegen::emit::Emitter::symbol(name);
                self.emitter.address_of("x0", &mangled);
            }
            SymbolKind::Local | SymbolKind::Parameter(_) => {
                let Some(offset) = slot.and_then(|slot| self.layout.offset_of(slot)) else {
                    self.unlowered(expr.span, "a name with no frame slot");

                    return;
                };

                if is_pointer {
                    // A decayed array parameter holds an address rather than the data, so the
                    // address wanted is the one stored in the slot, not the slot's own.
                    self.emitter.load_from_frame("x0", Width::Double, offset);
                } else {
                    self.emitter.frame_address("x0", offset);
                }
            }
        }
    }

    /// Puts the address of `base[index]` into `x0`.
    fn address_of_element(&mut self, base: &Expr, index: &Expr, whole: &Expr) {
        let Some(temp) = self.temporary(whole.span) else {
            return;
        };
        let stride = element_stride(self.width_of(whole.id));

        self.deeper_address(base);
        self.emitter.store_to_frame("x0", Width::Double, temp);
        self.deeper(index);

        // The index is a signed 32-bit value and the address arithmetic is 64-bit, so it is
        // widened rather than reinterpreted: a negative index has to stay negative.
        self.emitter.instruction("sxtw x1, w0");
        self.emitter.load_from_frame("x0", Width::Double, temp);
        self.emitter.load_immediate(&format!("x{SECOND}"), stride);
        self.emitter
            .instruction(&format!("madd x0, x1, x{SECOND}, x0"));
    }

    /// Puts the address of a string literal's bytes into `x0`.
    fn string_literal(&mut self, expr: &Expr) {
        let Some(label) = self.annotations.string_label(expr.id) else {
            self.unlowered(expr.span, "a string literal with no label");

            return;
        };
        let label = label.to_owned();

        self.emitter.address_of("x0", &label);
    }

    /// The access width for the value at `node`.
    pub(crate) fn width_of(&self, node: crate::ast::NodeId) -> Width {
        match self.annotations.type_of(node) {
            Some(Ty::Char) => Width::Byte,
            Some(Ty::Ptr(_)) => Width::Double,
            Some(Ty::Array(element, _)) => match element.as_ref() {
                Ty::Char => Width::Byte,
                _ => Width::Word,
            },
            _ => Width::Word,
        }
    }

    /// The access width of one element of the array declared at `node`.
    pub(crate) fn element_width(&self, node: crate::ast::NodeId) -> Width {
        let Some(symbol) = self
            .annotations
            .binding_of(node)
            .and_then(|id| self.annotations.symbol(id))
        else {
            return Width::Word;
        };

        match &symbol.ty {
            Ty::Array(element, _) => match element.as_ref() {
                Ty::Char => Width::Byte,
                Ty::Ptr(_) => Width::Double,
                _ => Width::Word,
            },
            Ty::Char => Width::Byte,
            Ty::Ptr(_) => Width::Double,
            _ => Width::Word,
        }
    }
}

/// The condition code that reads the same way as the source operator.
///
/// `None` for an operator that is not a comparison. Returning a code anyway would mean picking one,
/// and the only honest choice — `al`, always — is a `cset` that yields 1 unconditionally: a wrong
/// answer that assembles, which is exactly the failure this module is arranged to avoid.
fn condition_code(op: BinOp) -> Option<&'static str> {
    match op {
        BinOp::Equal => Some("eq"),
        BinOp::NotEqual => Some("ne"),
        BinOp::Less => Some("lt"),
        BinOp::Greater => Some("gt"),
        BinOp::LessEqual => Some("le"),
        BinOp::GreaterEqual => Some("ge"),
        BinOp::Add
        | BinOp::Subtract
        | BinOp::Multiply
        | BinOp::Divide
        | BinOp::Remainder
        | BinOp::And
        | BinOp::Or => None,
    }
}
