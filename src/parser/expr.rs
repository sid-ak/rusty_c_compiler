//! Expression parsing: precedence, associativity, and the postfix chain.
//!
//! The grammar in `grammar/syntax.ebnf` writes the binary operators as ten nested rules —
//! `logical_or` calling `logical_and` calling `equality`, on down to `primary` — because nesting
//! depth is how a grammar spells precedence. Ten functions mirroring them one for one would be ten
//! near-identical bodies differing only in which operators they match and which function they call
//! next, and every change to the precedence table would have to be made in two of them at once.
//!
//! `Parser::binary` is those rules with the repetition factored out, a technique usually called
//! precedence climbing: one function, one table in `binary_operator`, and a recursive call whose
//! minimum precedence stands in for descending a level. `1 + 2 * 3` groups as `1 + (2 * 3)` for the
//! same reason it does in the grammar — `*` outranks `+`, so the recursive call for the right
//! operand accepts it and the loop here does not.
//!
//! The rest is shaped as the grammar is: assignment and the prefix operators are
//! right-associative, the binary levels are left-associative, and `[]`, `()`, `++`, and `--` are a
//! loop after the primary expression rather than rules of their own, so they chain in any
//! combination — `a[i]++` and `f(x)[0]` both fall out with no case for either.

use crate::ast::{BinOp, Expr, ExprKind, IncDec, UnOp};
use crate::diagnostics::{Diagnostic, Span};
use crate::lexer::TokenKind;
use crate::parser::{Bail, Parser};

/// The precedence of the loosest-binding operator, and so where a full expression starts.
const LOWEST_PRECEDENCE: u8 = 1;

impl Parser<'_> {
    /// Parse a full expression, one nesting level deeper than the caller.
    pub(super) fn expression(&mut self) -> Result<Expr, Bail> {
        self.nested(Self::assignment)
    }

    /// Parse an assignment, or whatever tighter-binding expression stands in for one.
    ///
    /// Assignment is right-associative, so `a = b = c` is `a = (b = c)`: the right operand is
    /// parsed by calling back in at this same level, rather than by the loop that makes the binary
    /// operators group leftward.
    fn assignment(&mut self) -> Result<Expr, Bail> {
        let target = self.binary(LOWEST_PRECEDENCE)?;

        if !self.at(&TokenKind::Assign) {
            return Ok(target);
        }

        // Whether the target can actually be written to is Phase 3's question — it needs types to
        // answer it. What can be settled here is the shape: `1 = x` and `f() = x` are not
        // assignments that happen to be wrong, they are not assignments.
        if !is_assignable(&target.kind) {
            return Err(self.report(
                Diagnostic::parse(target.span, "expression is not assignable")
                    .with_note("only a variable or an array element can be assigned to"),
            ));
        }

        self.advance();
        let value = self.expression()?;
        let span = target.span.to(value.span);

        Ok(self.expr(
            ExprKind::Assign {
                target: Box::new(target),
                value: Box::new(value),
            },
            span,
        ))
    }

    /// Parse a binary expression whose operators bind at least as tightly as `min_precedence`.
    ///
    /// The loop makes each level left-associative: `1 - 2 - 3` is `(1 - 2) - 3` because the result
    /// so far becomes the left operand of the next operator. The recursive call uses one more than
    /// the current precedence, which is what stops the right operand from swallowing an operator
    /// of equal rank.
    ///
    /// Each pass of the loop makes the tree one level deeper without recursing, so each is charged
    /// a nesting level explicitly; otherwise `1 + 1 + … + 1` could build a tree deeper than any
    /// later pass can walk.
    fn binary(&mut self, min_precedence: u8) -> Result<Expr, Bail> {
        self.restoring_depth(|parser| parser.binary_chain(min_precedence))
    }

    /// Parse a binary expression. Always reached through [`Parser::binary`], which gives back the
    /// levels its loop charges — needed because the right operand is parsed by calling back in
    /// here directly, so its levels would otherwise stay charged to the rest of the chain.
    fn binary_chain(&mut self, min_precedence: u8) -> Result<Expr, Bail> {
        let mut left = self.unary()?;

        loop {
            if let Some((spelling, span)) = self.adjacent_operator() {
                return Err(self.unsupported_at(span, format!("'{spelling}'")));
            }

            let Some((op, precedence)) = binary_operator(&self.peek().kind) else {
                break;
            };
            if precedence < min_precedence {
                break;
            }

            self.deepen()?;
            self.advance();
            let right = self.binary(precedence.saturating_add(1))?;
            let span = left.span.to(right.span);

            left = self.expr(
                ExprKind::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                },
                span,
            );
        }

        Ok(left)
    }

    /// Parse a prefix operator and its operand, one nesting level deeper than the caller.
    fn unary(&mut self) -> Result<Expr, Bail> {
        self.nested(Self::unary_inner)
    }

    /// Parse a prefix expression. Always reached through [`Parser::unary`], which counts the level.
    fn unary_inner(&mut self) -> Result<Expr, Bail> {
        let Some(op) = prefix_operator(&self.peek().kind) else {
            return self.postfix();
        };

        let start = self.advance().span;
        // Right-associative, so `- - x` is `-(-x)`: the operand is another unary expression, not
        // a postfix one.
        let operand = self.unary()?;
        let span = start.to(operand.span);

        Ok(self.expr(
            ExprKind::Unary {
                op,
                operand: Box::new(operand),
            },
            span,
        ))
    }

    /// Parse a primary expression and whatever postfix operators follow it.
    ///
    /// Like the loop in [`Parser::binary`], each operator in the chain makes the tree one level
    /// deeper without recursing, so `a[0][0]…[0]` is charged a nesting level per operator. Only
    /// [`Parser::unary`] reaches this, and its `nested` gives those levels back on the way out.
    fn postfix(&mut self) -> Result<Expr, Bail> {
        let mut expr = self.primary()?;

        loop {
            let next = &self.peek().kind;
            let extends = matches!(next, TokenKind::LBracket | TokenKind::LParen)
                || postfix_operator(next).is_some();
            if !extends {
                break;
            }
            self.deepen()?;

            if self.at(&TokenKind::LBracket) {
                self.advance();
                let index = self.expression()?;
                let close = self.expect(&TokenKind::RBracket)?;
                let span = expr.span.to(close);

                expr = self.expr(
                    ExprKind::Index {
                        base: Box::new(expr),
                        index: Box::new(index),
                    },
                    span,
                );
                continue;
            }

            if self.at(&TokenKind::LParen) {
                let (args, close) = self.call_args()?;
                let span = expr.span.to(close);

                expr = self.expr(
                    ExprKind::Call {
                        callee: Box::new(expr),
                        args,
                    },
                    span,
                );
                continue;
            }

            let Some(op) = postfix_operator(&self.peek().kind) else {
                break;
            };
            let end = self.advance().span;
            let span = expr.span.to(end);

            expr = self.expr(
                ExprKind::PostfixIncDec {
                    op,
                    operand: Box::new(expr),
                },
                span,
            );
        }

        Ok(expr)
    }

    /// Parse a call's argument list, returning the arguments and the closing parenthesis.
    ///
    /// Each argument is parsed as a full expression, which stops at a comma because this subset
    /// has no comma operator — so `f(a, b = c)` is two arguments, and the assignment inside the
    /// second is not mistaken for a separator.
    fn call_args(&mut self) -> Result<(Vec<Expr>, Span), Bail> {
        self.expect(&TokenKind::LParen)?;
        let mut args = Vec::new();

        if !self.at(&TokenKind::RParen) {
            args.push(self.expression()?);
            while self.eat(&TokenKind::Comma) {
                args.push(self.expression()?);
            }
        }

        let close = self.expect(&TokenKind::RParen)?;

        Ok((args, close))
    }

    /// Parse a literal, a name, or a parenthesized expression.
    fn primary(&mut self) -> Result<Expr, Bail> {
        let span = self.here();

        // A literal's value was decoded by the lexer, in the one place that had the source text in
        // hand, so it is copied out here rather than parsed again.
        let literal = match &self.peek().kind {
            TokenKind::IntLit(value) => Some(ExprKind::IntLit(*value)),
            TokenKind::CharLit(byte) => Some(ExprKind::CharLit(*byte)),
            TokenKind::StrLit(bytes) => Some(ExprKind::StrLit(bytes.clone())),
            _ => None,
        };
        if let Some(kind) = literal {
            self.advance();

            return Ok(self.expr(kind, span));
        }

        if matches!(self.peek().kind, TokenKind::Ident(_)) {
            let name = self.name()?;

            return Ok(self.expr(ExprKind::Ident(name.text), name.span));
        }

        if self.at(&TokenKind::LParen) {
            self.advance();
            // Parentheses leave nothing behind. The tree they were written to produce already
            // records the grouping, so `((((1))))` is the same tree as `1`.
            let inner = self.expression()?;
            self.expect(&TokenKind::RParen)?;

            return Ok(inner);
        }

        Err(self.expected("an expression"))
    }

    /// The operator the next two tokens spell together, if the subset omits it.
    ///
    /// `+=` and `<<` are not tokens here, because this subset has neither, so the lexer hands over
    /// their two halves. Recognizing the pair — adjacent, in that order — is what makes `a += b`
    /// one diagnostic that names compound assignment, instead of a complaint about an unexpected
    /// `=` that leaves the reader to work out what the parser thought was going on.
    fn adjacent_operator(&self) -> Option<(&'static str, Span)> {
        let first = self.peek();
        let second = self.peek_at(1);

        if first.span.end != second.span.start {
            return None;
        }

        let spelling = match (&first.kind, &second.kind) {
            (TokenKind::Plus, TokenKind::Assign) => "+=",
            (TokenKind::Minus, TokenKind::Assign) => "-=",
            (TokenKind::Star, TokenKind::Assign) => "*=",
            (TokenKind::Slash, TokenKind::Assign) => "/=",
            (TokenKind::Percent, TokenKind::Assign) => "%=",
            (TokenKind::Lt, TokenKind::Lt) => "<<",
            (TokenKind::Gt, TokenKind::Gt) => ">>",
            _ => return None,
        };

        Some((spelling, first.span.to(second.span)))
    }

    /// An expression of `kind` covering `span`, with a fresh identity.
    fn expr(&mut self, kind: ExprKind, span: Span) -> Expr {
        Expr {
            id: self.node_id(),
            kind,
            span,
        }
    }
}

/// The binary operator `kind` is, and how tightly it binds. A higher number binds more tightly.
///
/// This table is the grammar's `logical_or` through `multiplicative` chain, written once. The
/// numbers are the nesting depth of those rules read from the outside in, which is why they are
/// consecutive and why nothing else in the parser has to know them.
fn binary_operator(kind: &TokenKind) -> Option<(BinOp, u8)> {
    let (op, precedence) = match kind {
        TokenKind::PipePipe => (BinOp::Or, 1),
        TokenKind::AmpAmp => (BinOp::And, 2),
        TokenKind::EqEq => (BinOp::Equal, 3),
        TokenKind::BangEq => (BinOp::NotEqual, 3),
        TokenKind::Lt => (BinOp::Less, 4),
        TokenKind::Gt => (BinOp::Greater, 4),
        TokenKind::LtEq => (BinOp::LessEqual, 4),
        TokenKind::GtEq => (BinOp::GreaterEqual, 4),
        TokenKind::Plus => (BinOp::Add, 5),
        TokenKind::Minus => (BinOp::Subtract, 5),
        TokenKind::Star => (BinOp::Multiply, 6),
        TokenKind::Slash => (BinOp::Divide, 6),
        TokenKind::Percent => (BinOp::Remainder, 6),
        _ => return None,
    };

    Some((op, precedence))
}

/// The prefix operator `kind` is, if it is one.
fn prefix_operator(kind: &TokenKind) -> Option<UnOp> {
    let op = match kind {
        TokenKind::Minus => UnOp::Negate,
        TokenKind::Bang => UnOp::Not,
        TokenKind::Plus => UnOp::Plus,
        TokenKind::PlusPlus => UnOp::PreIncrement,
        TokenKind::MinusMinus => UnOp::PreDecrement,
        _ => return None,
    };

    Some(op)
}

/// The postfix operator `kind` is, if it is one.
fn postfix_operator(kind: &TokenKind) -> Option<IncDec> {
    match kind {
        TokenKind::PlusPlus => Some(IncDec::Increment),
        TokenKind::MinusMinus => Some(IncDec::Decrement),
        _ => None,
    }
}

/// Whether `kind` is a shape the grammar allows on the left of `=`.
///
/// Parentheses leave no node behind, so `(a) = 1` arrives here as a plain identifier — which is
/// what C says it is.
fn is_assignable(kind: &ExprKind) -> bool {
    matches!(kind, ExprKind::Ident(_) | ExprKind::Index { .. })
}

#[cfg(test)]
mod tests;
