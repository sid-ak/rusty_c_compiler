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
    fn binary(&mut self, min_precedence: u8) -> Result<Expr, Bail> {
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
    fn postfix(&mut self) -> Result<Expr, Bail> {
        let mut expr = self.primary()?;

        loop {
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
mod tests {
    use super::*;

    use crate::ast::{self, Spans};
    use crate::lexer;

    /// Parse `source` as a single expression, asserting it consumed all of it cleanly, and render
    /// its tree on one line.
    ///
    /// One line, because precedence and associativity are questions about shape and nothing else;
    /// the indentation that makes a whole-program dump readable only gets in the way here.
    fn shape(source: &str) -> String {
        let lexed = lexer::lex(source.as_bytes());
        assert!(
            lexed.diagnostics.is_empty(),
            "expected {source:?} to lex cleanly, got: {:?}",
            lexed.diagnostics
        );

        let mut parser = Parser::new(&lexed.tokens);
        let expr = parser
            .expression()
            .unwrap_or_else(|Bail| panic!("expected {source:?} to parse as one expression"));

        assert!(
            parser.diagnostics.is_empty(),
            "expected {source:?} to parse cleanly, got: {:?}",
            parser.diagnostics
        );
        assert!(
            parser.at_eof(),
            "expected {source:?} to be one whole expression"
        );

        ast::dump_expression(&expr, Spans::Hidden)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Parse `source` as an expression and return what it complained about.
    fn errors(source: &str) -> Vec<String> {
        let lexed = lexer::lex(source.as_bytes());
        assert!(lexed.diagnostics.is_empty(), "{:?}", lexed.diagnostics);

        let mut parser = Parser::new(&lexed.tokens);
        let _ = parser.expression();

        parser
            .diagnostics
            .clone()
            .into_sorted()
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    /// Every primary form parses to itself.
    #[test]
    fn primaries_parse_to_themselves() {
        assert_eq!(shape("42"), "(int-lit 42)");
        assert_eq!(shape("0xff"), "(int-lit 255)");
        assert_eq!(shape("'a'"), "(char-lit 'a')");
        assert_eq!(shape(r#""hi""#), r#"(str-lit "hi")"#);
        assert_eq!(shape("total"), "(ident total)");
    }

    /// Precedence and associativity, asserted on the tree rather than on a result: `1+2*3 == 7`
    /// would also hold if precedence were wrong in a way that cancelled out.
    #[test]
    fn precedence_and_associativity_match_c() {
        let cases = [
            (
                "1+2*3",
                "(binary + (int-lit 1) (binary * (int-lit 2) (int-lit 3)))",
            ),
            (
                "1*2+3",
                "(binary + (binary * (int-lit 1) (int-lit 2)) (int-lit 3))",
            ),
            (
                "1-2-3",
                "(binary - (binary - (int-lit 1) (int-lit 2)) (int-lit 3))",
            ),
            (
                "1/2/3",
                "(binary / (binary / (int-lit 1) (int-lit 2)) (int-lit 3))",
            ),
            ("a=b=c", "(assign (ident a) (assign (ident b) (ident c)))"),
            ("-x*y", "(binary * (unary - (ident x)) (ident y))"),
            ("!a&&b", "(binary && (unary ! (ident a)) (ident b))"),
            (
                "a||b&&c",
                "(binary || (ident a) (binary && (ident b) (ident c)))",
            ),
            (
                "a<b==c<d",
                "(binary == (binary < (ident a) (ident b)) (binary < (ident c) (ident d)))",
            ),
            (
                "a%b*c",
                "(binary * (binary % (ident a) (ident b)) (ident c))",
            ),
            (
                "a[i]+1",
                "(binary + (index (ident a) (ident i)) (int-lit 1))",
            ),
            (
                "f(x)*2",
                "(binary * (call (ident f) (args (ident x))) (int-lit 2))",
            ),
            (
                "-f(x)[i]",
                "(unary - (index (call (ident f) (args (ident x))) (ident i)))",
            ),
            ("1+ +2", "(binary + (int-lit 1) (unary + (int-lit 2)))"),
            ("1++ +2", "(binary + (postfix ++ (int-lit 1)) (int-lit 2))"),
            (
                "a<=b>=c",
                "(binary >= (binary <= (ident a) (ident b)) (ident c))",
            ),
            (
                "a!=b||c==d",
                "(binary || (binary != (ident a) (ident b)) (binary == (ident c) (ident d)))",
            ),
        ];

        for (source, expected) in cases {
            assert_eq!(shape(source), expected, "for {source:?}");
        }
    }

    /// Prefix operators are right-associative, so they nest outermost-first.
    #[test]
    fn prefix_operators_are_right_associative() {
        assert_eq!(shape("- -x"), "(unary - (unary - (ident x)))");
        assert_eq!(shape("!!a"), "(unary ! (unary ! (ident a)))");
        assert_eq!(shape("++--a"), "(unary ++ (unary -- (ident a)))");
    }

    /// The postfix operators chain in any combination, from one loop with no case for either.
    #[test]
    fn postfix_operators_chain() {
        assert_eq!(shape("a[i]++"), "(postfix ++ (index (ident a) (ident i)))");
        assert_eq!(
            shape("f(x)[0]"),
            "(index (call (ident f) (args (ident x))) (int-lit 0))"
        );
        assert_eq!(
            shape("a[i][j]"),
            "(index (index (ident a) (ident i)) (ident j))"
        );
        assert_eq!(shape("a--"), "(postfix -- (ident a))");
    }

    /// Call arguments parse at assignment precedence, so an assignment inside one is an argument
    /// rather than a separator.
    #[test]
    fn call_arguments_parse_at_assignment_precedence() {
        assert_eq!(shape("f()"), "(call (ident f) (args))");
        assert_eq!(
            shape("f(a, b = c)"),
            "(call (ident f) (args (ident a) (assign (ident b) (ident c))))"
        );
        assert_eq!(
            shape("f(a + b, c)"),
            "(call (ident f) (args (binary + (ident a) (ident b)) (ident c)))"
        );
    }

    /// Parentheses group and then vanish, so a deeply parenthesized expression is the same tree as
    /// the expression inside it.
    #[test]
    fn parentheses_leave_no_trace() {
        assert_eq!(shape("((((1))))"), shape("1"));
        assert_eq!(
            shape("(a+b)*c"),
            "(binary * (binary + (ident a) (ident b)) (ident c))"
        );
        assert_eq!(shape("(a) = 1"), "(assign (ident a) (int-lit 1))");
    }

    /// Only a variable or an array element can be written to; anything else is not an assignment
    /// that is wrong, it is not an assignment.
    #[test]
    fn only_syntactic_lvalues_may_be_assigned_to() {
        for source in ["1 = 2", "f() = 1", "(a + b) = 1", "a++ = 1"] {
            assert_eq!(
                errors(source),
                ["expression is not assignable".to_string()],
                "for {source:?}"
            );
        }

        for source in ["a = 1", "a[i] = 1", "(a) = 1", "(a[i]) = 1"] {
            let _ = shape(source);
        }
    }

    /// An operator this subset omits that lexes as two tokens is named, rather than reported as a
    /// stray second half.
    #[test]
    fn operators_spelled_as_two_tokens_are_named() {
        let cases = [
            ("a += b", "unsupported in this C subset: '+='"),
            ("a -= b", "unsupported in this C subset: '-='"),
            ("a *= b", "unsupported in this C subset: '*='"),
            ("a /= b", "unsupported in this C subset: '/='"),
            ("a %= b", "unsupported in this C subset: '%='"),
            ("a << b", "unsupported in this C subset: '<<'"),
            ("a >> b", "unsupported in this C subset: '>>'"),
        ];

        for (source, message) in cases {
            assert_eq!(errors(source), [message.to_string()], "for {source:?}");
        }
    }

    /// The pair is only recognized when the two halves touch, so spaced-out real operators still
    /// parse as themselves.
    #[test]
    fn separated_operators_are_not_mistaken_for_a_pair() {
        assert_eq!(shape("a < -b"), "(binary < (ident a) (unary - (ident b)))");
        assert_eq!(shape("a > + b"), "(binary > (ident a) (unary + (ident b)))");
    }

    /// An expression that cannot be parsed says what it wanted and what it found.
    #[test]
    fn a_missing_operand_names_what_was_wanted() {
        assert_eq!(errors("1 +"), ["expected an expression, found end of file"]);
        assert_eq!(
            errors("a[i]["),
            ["expected an expression, found end of file"]
        );
        assert_eq!(errors("*x"), ["expected an expression, found '*'"]);
    }

    /// Every binary operator in the AST is reachable from source, so none is unreachable in
    /// practice while still being representable.
    #[test]
    fn every_binary_operator_parses() {
        for op in BinOp::ALL {
            let source = format!("a {} b", op.spelling());

            assert_eq!(
                shape(&source),
                format!("(binary {} (ident a) (ident b))", op.spelling()),
                "for {source:?}"
            );
        }
    }

    /// Every prefix and postfix operator is reachable from source too.
    #[test]
    fn every_unary_operator_parses() {
        for op in UnOp::ALL {
            let source = format!("{}a", op.spelling());

            assert_eq!(
                shape(&source),
                format!("(unary {} (ident a))", op.spelling()),
                "for {source:?}"
            );
        }

        for op in IncDec::ALL {
            let source = format!("a{}", op.spelling());

            assert_eq!(
                shape(&source),
                format!("(postfix {} (ident a))", op.spelling()),
                "for {source:?}"
            );
        }
    }
}
