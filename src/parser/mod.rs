//! The parser: a token stream in, a [`Program`] out.
//!
//! This module holds the declaration and statement grammar; [`expr`] holds the expression grammar,
//! which is shaped differently enough to be worth reading on its own. Both are recursive descent —
//! one function per production in `grammar/syntax.ebnf`, calling each other in the shape of the
//! grammar itself — so the two can be read side by side.
//!
//! Three properties hold for every input, valid or not.
//!
//! # It reports and keeps going
//!
//! A syntax error records a diagnostic and abandons the construct it was in by returning `Bail`.
//! The nearest enclosing loop — over the items of a file, or over the statements of a block —
//! catches that, skips to the next `;` or `}` at the current brace depth, and carries on. A file
//! with four mistakes reports four of them rather than only the first, and one mistake reports
//! once rather than cascading into a dozen follow-on complaints about the wreckage.
//!
//! # It terminates
//!
//! Every recovery step consumes at least one token. That is not left to inspection: the loop
//! records where the failed construct started and, if recovery skipped nothing at all, consumes one
//! token itself. A construct that fails on its very first token therefore still moves the parser
//! forward, so no input can make recovery spin in place.
//!
//! # It does not overflow the stack
//!
//! Recursive descent recurses, and deeply nested input would otherwise run the call stack out —
//! which is a crash, and a crash is the one thing the front end is not allowed to produce. Every
//! recursive entry point goes through `Parser::nested`, which reports past
//! [`MAX_NESTING_DEPTH`] instead of descending further.

pub mod expr;

use std::fmt;

use crate::ast::{
    BaseType, Block, ForInit, FuncDecl, FuncDef, FuncSig, Initializer, Item, Name, NodeId, NodeIds,
    Param, Program, Stmt, StmtKind, TypeSpec, VarDecl,
};
use crate::diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, Span};
use crate::lexer::token;
use crate::lexer::{Keyword, Token, TokenKind};

/// How many levels of nesting the parser will descend before reporting instead of recursing.
///
/// The limit counts parser descents rather than brackets: a parenthesized expression costs two,
/// a nested block one. Real C reaches nothing close to it — the deepest expression anyone writes
/// by hand is a handful of levels — so the limit is only ever met by generated or hostile input,
/// which is exactly the case it exists to turn into a diagnostic.
///
/// The number is a stack budget, not a taste in style, and it was measured rather than guessed. A
/// level costs roughly 4 KB of stack in an unoptimized build, so 128 of them fit in well under a
/// megabyte — comfortable inside the 2 MiB stack the test harness gives a thread, and far inside
/// the 8 MiB the binary's main thread has. The `nesting_stays_within_a_small_stack` test holds that
/// margin to a fixed figure by parsing on a deliberately undersized stack, so a change that makes a
/// parse frame fatter fails there rather than as a crash on someone's input.
pub const MAX_NESTING_DEPTH: usize = 128;

/// The token a lookup past the end of the stream reports.
///
/// The lexer always ends its stream with `Eof`, so this is reached only if the parser is handed an
/// empty slice — which the Phase 5 fuzz targets can do. Answering with a token rather than a `None`
/// keeps every lookahead site free of a case that cannot occur in a real stream.
const END_OF_STREAM: Token = Token {
    kind: TokenKind::Eof,
    span: Span { start: 0, end: 0 },
};

/// Everything one parse of a token stream produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parsed {
    /// The program. Complete after a clean parse; missing whatever failed otherwise.
    pub program: Program,
    /// The problems found, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Parse `tokens` into a program.
pub fn parse(tokens: &[Token]) -> Parsed {
    let mut parser = Parser::new(tokens);
    let program = parser.program();

    Parsed {
        program,
        diagnostics: parser.diagnostics.into_sorted(),
    }
}

/// A construct the parser abandoned after reporting why.
///
/// It carries nothing: the diagnostic is already in the bag, and every caller either passes it
/// upward with `?` or recovers from it. Being its own type rather than `()` is what stops a bail
/// from reading like a successful unit result at a glance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bail;

/// The parsing state: the tokens, how far into them we are, and how deep.
struct Parser<'tokens> {
    /// The stream being parsed.
    tokens: &'tokens [Token],
    /// How many tokens have been consumed. Never decreases, which is half of why parsing ends.
    position: usize,
    /// How many recursive descents are currently on the stack.
    depth: usize,
    /// Hands out the identity of each node built.
    ids: NodeIds,
    /// Problems found so far.
    diagnostics: DiagnosticBag,
}

impl<'tokens> Parser<'tokens> {
    /// A parser positioned at the start of `tokens`.
    fn new(tokens: &'tokens [Token]) -> Self {
        Self {
            tokens,
            position: 0,
            depth: 0,
            ids: NodeIds::new(),
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Parse a whole translation unit.
    fn program(&mut self) -> Program {
        let mut items = Vec::new();

        while !self.at_eof() {
            let started_at = self.position;
            match self.item() {
                Ok(item) => items.push(item),
                Err(Bail) => self.recover_from(started_at),
            }
        }

        Program { items }
    }

    /// Parse one top-level declaration.
    ///
    /// The three forms share a prefix — a type and a name — and are told apart by what follows it,
    /// so no lookahead beyond the next token is needed anywhere.
    fn item(&mut self) -> Result<Item, Bail> {
        let (ty, name) = self.declarator()?;

        if self.at(&TokenKind::LParen) {
            return self.function(ty, name);
        }

        let declaration = self.finish_var_decl(ty, name)?;
        self.expect(&TokenKind::Semi)?;

        Ok(Item::GlobalVar(declaration))
    }

    /// Parse a function from its parameter list on, given the type and name already read.
    fn function(&mut self, return_type: TypeSpec, name: Name) -> Result<Item, Bail> {
        self.expect(&TokenKind::LParen)?;
        let params = self.params()?;
        let close = self.expect(&TokenKind::RParen)?;

        let signature = FuncSig {
            span: return_type.span.to(close),
            return_type,
            name,
            params,
        };

        if !self.at(&TokenKind::LBrace) {
            let semi = self.expect(&TokenKind::Semi)?;
            let span = signature.span.to(semi);

            return Ok(Item::FuncDecl(FuncDecl {
                id: self.node_id(),
                signature,
                span,
            }));
        }

        let body = self.block()?;
        let span = signature.span.to(body.span);

        Ok(Item::FuncDef(FuncDef {
            id: self.node_id(),
            signature,
            body,
            span,
        }))
    }

    /// Parse a parameter list, between the parentheses.
    fn params(&mut self) -> Result<Vec<Param>, Bail> {
        if self.at(&TokenKind::RParen) {
            return Ok(Vec::new());
        }

        // `(void)` is the explicit spelling of "takes nothing", and is only that when the `void`
        // is the whole list — `void x` is a parameter named `x`, which the grammar admits and
        // Phase 3 rejects.
        if self.at(&TokenKind::Keyword(Keyword::Void)) && self.peek_at(1).kind == TokenKind::RParen
        {
            self.advance();
            return Ok(Vec::new());
        }

        let mut params = vec![self.param()?];
        while self.eat(&TokenKind::Comma) {
            params.push(self.param()?);
        }

        Ok(params)
    }

    /// Parse one parameter.
    fn param(&mut self) -> Result<Param, Bail> {
        let (ty, name) = self.declarator()?;

        if !self.at(&TokenKind::LBracket) {
            let span = ty.span.to(name.span);

            return Ok(Param {
                id: self.node_id(),
                ty,
                name,
                span,
            });
        }

        self.advance();
        if !self.at(&TokenKind::RBracket) {
            let span = self.here();
            return Err(self.report(
                Diagnostic::parse(span, "an array parameter may not declare a length").with_note(
                    "write 'int a[]': an array argument arrives as a pointer, so its length is \
                     not part of the parameter's type",
                ),
            ));
        }
        let close = self.expect(&TokenKind::RBracket)?;

        Ok(Param {
            id: self.node_id(),
            ty: TypeSpec::unsized_array(ty.base, ty.span.to(close)),
            name,
            span: ty.span.to(close),
        })
    }

    /// Parse a braced sequence of statements and declarations.
    fn block(&mut self) -> Result<Block, Bail> {
        let open = self.expect(&TokenKind::LBrace)?;
        let mut stmts = Vec::new();

        while !self.at(&TokenKind::RBrace) && !self.at_eof() {
            let started_at = self.position;
            match self.block_item() {
                Ok(stmt) => stmts.push(stmt),
                Err(Bail) => self.recover_from(started_at),
            }
        }

        let close = self.expect(&TokenKind::RBrace)?;

        Ok(Block {
            stmts,
            span: open.to(close),
        })
    }

    /// Parse one item of a block: a declaration, or a statement.
    fn block_item(&mut self) -> Result<Stmt, Bail> {
        if !self.at_type_keyword() {
            return self.statement();
        }

        let (ty, name) = self.declarator()?;
        let declaration = self.finish_var_decl(ty, name)?;
        let semi = self.expect(&TokenKind::Semi)?;
        let span = declaration.span.to(semi);

        Ok(self.stmt(StmtKind::LocalVar(declaration), span))
    }

    /// Parse one statement, one level deeper than the caller.
    fn statement(&mut self) -> Result<Stmt, Bail> {
        self.nested(Self::statement_inner)
    }

    /// Parse one statement. Always reached through [`Parser::statement`], which counts the level.
    fn statement_inner(&mut self) -> Result<Stmt, Bail> {
        if self.at(&TokenKind::LBrace) {
            let block = self.block()?;
            let span = block.span;

            return Ok(self.stmt(StmtKind::Block(block), span));
        }

        if self.at(&TokenKind::Semi) {
            let span = self.advance().span;

            return Ok(self.stmt(StmtKind::Empty, span));
        }

        match self.leading_keyword() {
            Some(Keyword::If) => self.if_statement(),
            Some(Keyword::While) => self.while_statement(),
            Some(Keyword::For) => self.for_statement(),
            Some(Keyword::Return) => self.return_statement(),
            Some(Keyword::Break) => self.jump_statement(StmtKind::Break),
            Some(Keyword::Continue) => self.jump_statement(StmtKind::Continue),
            Some(Keyword::Else) => Err(self.error_here("'else' without a matching 'if'")),
            // A declaration is a block item, not a statement, so `if (x) int y = 1;` declares a
            // variable whose scope would end before anything could use it. C rejects it, and
            // saying why beats "expected an expression, found 'int'".
            Some(Keyword::Int | Keyword::Char | Keyword::Void) => {
                let span = self.here();
                Err(self.report(
                    Diagnostic::parse(span, "a declaration is not allowed here").with_note(
                        "declarations may appear only directly inside a block or as a 'for' \
                         initializer",
                    ),
                ))
            }
            None => self.expression_statement(),
        }
    }

    /// Parse `if (condition) statement [else statement]`.
    fn if_statement(&mut self) -> Result<Stmt, Bail> {
        let start = self.advance().span;
        self.expect(&TokenKind::LParen)?;
        let condition = self.expression()?;
        self.expect(&TokenKind::RParen)?;
        let then_branch = Box::new(self.statement()?);

        // An `else` binds to the nearest `if` for free: it is taken here, by the innermost call
        // still on the stack, before that call returns to whatever `if` contains it.
        let else_branch = if self.eat(&TokenKind::Keyword(Keyword::Else)) {
            Some(Box::new(self.statement()?))
        } else {
            None
        };

        let end = else_branch
            .as_ref()
            .map_or(then_branch.span, |branch| branch.span);
        let span = start.to(end);

        Ok(self.stmt(
            StmtKind::If {
                condition,
                then_branch,
                else_branch,
            },
            span,
        ))
    }

    /// Parse `while (condition) statement`.
    fn while_statement(&mut self) -> Result<Stmt, Bail> {
        let start = self.advance().span;
        self.expect(&TokenKind::LParen)?;
        let condition = self.expression()?;
        self.expect(&TokenKind::RParen)?;
        let body = Box::new(self.statement()?);
        let span = start.to(body.span);

        Ok(self.stmt(StmtKind::While { condition, body }, span))
    }

    /// Parse `for (init; condition; step) statement`, with any clause omitted.
    fn for_statement(&mut self) -> Result<Stmt, Bail> {
        let start = self.advance().span;
        self.expect(&TokenKind::LParen)?;

        let init = if self.at(&TokenKind::Semi) {
            None
        } else if self.at_type_keyword() {
            let (ty, name) = self.declarator()?;
            Some(Box::new(ForInit::Decl(self.finish_var_decl(ty, name)?)))
        } else {
            Some(Box::new(ForInit::Expr(self.expression()?)))
        };
        self.expect(&TokenKind::Semi)?;

        let condition = if self.at(&TokenKind::Semi) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(&TokenKind::Semi)?;

        let step = if self.at(&TokenKind::RParen) {
            None
        } else {
            Some(self.expression()?)
        };
        self.expect(&TokenKind::RParen)?;

        let body = Box::new(self.statement()?);
        let span = start.to(body.span);

        Ok(self.stmt(
            StmtKind::For {
                init,
                condition,
                step,
                body,
            },
            span,
        ))
    }

    /// Parse `return;` or `return expression;`.
    fn return_statement(&mut self) -> Result<Stmt, Bail> {
        let start = self.advance().span;

        let value = if self.at(&TokenKind::Semi) {
            None
        } else {
            Some(self.expression()?)
        };
        let semi = self.expect(&TokenKind::Semi)?;

        Ok(self.stmt(StmtKind::Return(value), start.to(semi)))
    }

    /// Parse `break;` or `continue;`, which differ only in which they are.
    fn jump_statement(&mut self, kind: StmtKind) -> Result<Stmt, Bail> {
        let start = self.advance().span;
        let semi = self.expect(&TokenKind::Semi)?;

        Ok(self.stmt(kind, start.to(semi)))
    }

    /// Parse `expression;`.
    fn expression_statement(&mut self) -> Result<Stmt, Bail> {
        let expr = self.expression()?;
        let semi = self.expect(&TokenKind::Semi)?;
        let span = expr.span.to(semi);

        Ok(self.stmt(StmtKind::Expr(expr), span))
    }

    /// Parse the `type ident` every declaration and parameter begins with.
    ///
    /// One home for the shape means one place to notice a `*` between them, which is real C this
    /// subset does not have rather than a malformed declaration.
    fn declarator(&mut self) -> Result<(TypeSpec, Name), Bail> {
        let ty = self.type_spec()?;

        if self.at(&TokenKind::Star) {
            return Err(self.unsupported_here("pointer declarators"));
        }

        let name = self.name()?;

        Ok((ty, name))
    }

    /// Parse a type: `int`, `char`, or `void`.
    fn type_spec(&mut self) -> Result<TypeSpec, Bail> {
        let base = match self.leading_keyword() {
            Some(Keyword::Int) => BaseType::Int,
            Some(Keyword::Char) => BaseType::Char,
            Some(Keyword::Void) => BaseType::Void,
            _ => return Err(self.expected("a type")),
        };
        let span = self.advance().span;

        Ok(TypeSpec::scalar(base, span))
    }

    /// Parse what follows `type ident` in a declaration: an optional length, then an initializer.
    ///
    /// The terminating `;` belongs to the caller, because a `for` initializer does not have one.
    fn finish_var_decl(&mut self, base: TypeSpec, name: Name) -> Result<VarDecl, Bail> {
        let ty = if self.at(&TokenKind::LBracket) {
            self.array_type(base)?
        } else {
            base
        };

        let init = if self.eat(&TokenKind::Assign) {
            Some(self.initializer()?)
        } else {
            None
        };

        let end = init.as_ref().map_or(name.span, Initializer::span);
        let span = ty.span.to(end);

        Ok(VarDecl {
            id: self.node_id(),
            ty,
            name,
            init,
            span,
        })
    }

    /// Parse the `[3]` of an array declaration, given the element type already read.
    fn array_type(&mut self, base: TypeSpec) -> Result<TypeSpec, Bail> {
        self.expect(&TokenKind::LBracket)?;

        let Some(literal) = self.peek_int_literal() else {
            let span = self.here();
            return Err(self.report(
                Diagnostic::parse(span, "expected an integer literal for the array length")
                    .with_note(
                        "the length has to be written out; this subset has no constant \
                         expressions",
                    ),
            ));
        };
        self.advance();
        let close = self.expect(&TokenKind::RBracket)?;
        let span = base.span.to(close);

        // The lexer never produces a negative literal — `-3` is unary minus applied to `3` — so
        // this conversion cannot fail today. Handling it costs one branch and keeps the pass free
        // of an assumption a later change to the lexer could quietly break.
        let Ok(len) = u32::try_from(literal) else {
            return Err(self.report(Diagnostic::parse(span, "array length must not be negative")));
        };

        Ok(TypeSpec::array(base.base, len, span))
    }

    /// Parse an initializer: one expression, or a braced list.
    fn initializer(&mut self) -> Result<Initializer, Bail> {
        if !self.at(&TokenKind::LBrace) {
            return Ok(Initializer::Expr(self.expression()?));
        }

        let open = self.advance().span;
        let mut elements = Vec::new();

        while !self.at(&TokenKind::RBrace) && !self.at_eof() {
            elements.push(self.expression()?);

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        let close = self.expect(&TokenKind::RBrace)?;

        Ok(Initializer::List {
            elements,
            span: open.to(close),
        })
    }

    /// Consume an identifier, or report why the next token is not one.
    fn name(&mut self) -> Result<Name, Bail> {
        // A word real C reserves is not a name the user chose, however it happened to lex.
        // Declining it here sends it to `expected`, which is the one place that turns it into
        // "unsupported in this C subset" rather than a complaint about an identifier.
        let text = match &self.peek().kind {
            TokenKind::Ident(text) if token::unsupported_keyword(text).is_none() => text.clone(),
            _ => return Err(self.expected("an identifier")),
        };
        let span = self.advance().span;

        Ok(Name { text, span })
    }

    /// Skip past the construct that failed so parsing can resume at the next one.
    ///
    /// `started_at` is where that construct began. If nothing at all was consumed — a construct
    /// that failed on its very first token — one token is skipped anyway, which is what makes
    /// every recovery step consume at least one and so makes recovery terminate.
    fn recover_from(&mut self, started_at: usize) {
        self.skip_to_boundary();

        if self.position == started_at {
            self.advance();
        }
    }

    /// Skip to just past the next `;`, or up to the `}` that closes the enclosing block.
    ///
    /// Nested braces are counted so that skipping over a `{ ... }` inside the failed construct
    /// does not mistake its closing brace for the enclosing block's. A `}` seen at depth zero is
    /// left where it is: it belongs to the block the parser is inside, and swallowing it is how
    /// one missing brace turns into the rest of the file disappearing.
    fn skip_to_boundary(&mut self) {
        let mut depth = 0usize;

        while !self.at_eof() {
            if self.at(&TokenKind::LBrace) {
                depth += 1;
                self.advance();
                continue;
            }

            if self.at(&TokenKind::RBrace) {
                if depth == 0 {
                    return;
                }
                depth -= 1;
                self.advance();
                if depth == 0 {
                    // A `};` closes a struct, union, or enum definition — which are exactly the
                    // constructs most likely to have brought recovery here — so that semicolon is
                    // part of the wreckage rather than the start of whatever comes next.
                    self.eat(&TokenKind::Semi);
                    return;
                }
                continue;
            }

            if depth == 0 && self.at(&TokenKind::Semi) {
                self.advance();
                return;
            }

            self.advance();
        }
    }

    /// Run `parse` one nesting level deeper, reporting rather than recursing past the limit.
    fn nested<T>(&mut self, parse: fn(&mut Self) -> Result<T, Bail>) -> Result<T, Bail> {
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            self.depth -= 1;
            return Err(self.too_deep());
        }

        let parsed = parse(self);
        self.depth -= 1;

        parsed
    }

    /// Report that the input nests deeper than the parser will follow.
    fn too_deep(&mut self) -> Bail {
        let span = self.here();

        self.report(
            Diagnostic::parse(
                span,
                format!(
                    "nesting is too deep: the parser descends at most {MAX_NESTING_DEPTH} levels"
                ),
            )
            .with_note("split the expression or the block into smaller pieces"),
        )
    }

    /// Report that `wanted` was expected where the next token is, and abandon the construct.
    ///
    /// The single place a token is turned down, which is why it is also the place that recognizes
    /// a word real C reserves: every rejection anywhere in the parser says "unsupported in this C
    /// subset" instead of "expected a type" when that is the truer answer.
    fn expected(&mut self, wanted: &str) -> Bail {
        if let Some(word) = unsupported_word(&self.peek().kind) {
            return self.unsupported_here(format!("'{word}'"));
        }

        let span = self.here();
        let message = if self.at(&TokenKind::Eof) {
            format!("expected {wanted}, found end of file")
        } else {
            format!("expected {wanted}, found '{}'", self.peek().kind)
        };

        self.report(Diagnostic::parse(span, message))
    }

    /// Report `message` at the next token, and abandon the construct.
    fn error_here(&mut self, message: impl Into<String>) -> Bail {
        let span = self.here();

        self.report(Diagnostic::parse(span, message))
    }

    /// Report the next token as a construct outside the subset, and abandon it.
    fn unsupported_here(&mut self, construct: impl fmt::Display) -> Bail {
        let span = self.here();

        self.unsupported_at(span, construct)
    }

    /// Report `span` as a construct outside the subset, and abandon it.
    fn unsupported_at(&mut self, span: Span, construct: impl fmt::Display) -> Bail {
        self.report(Diagnostic::unsupported(
            DiagnosticKind::Parse,
            span,
            construct,
        ))
    }

    /// Record `diagnostic`, and abandon the construct it describes.
    fn report(&mut self, diagnostic: Diagnostic) -> Bail {
        self.diagnostics.push(diagnostic);

        Bail
    }

    /// A node of `kind` covering `span`, with a fresh identity.
    fn stmt(&mut self, kind: StmtKind, span: Span) -> Stmt {
        Stmt {
            id: self.node_id(),
            kind,
            span,
        }
    }

    /// The next unused node identity.
    fn node_id(&mut self) -> NodeId {
        self.ids.next_id()
    }

    /// The token at `index`, or the end of the stream if the index is past it.
    fn token_at(&self, index: usize) -> &Token {
        self.tokens
            .get(index)
            .or_else(|| self.tokens.last())
            .unwrap_or(&END_OF_STREAM)
    }

    /// The next token, without consuming it.
    fn peek(&self) -> &Token {
        self.token_at(self.position)
    }

    /// The token `ahead` positions past the next one, without consuming anything.
    fn peek_at(&self, ahead: usize) -> &Token {
        self.token_at(self.position.saturating_add(ahead))
    }

    /// The span of the next token, for a diagnostic pointing at it.
    fn here(&self) -> Span {
        self.peek().span
    }

    /// The value of the next token, if it is an integer literal.
    fn peek_int_literal(&self) -> Option<i32> {
        match self.peek().kind {
            TokenKind::IntLit(value) => Some(value),
            _ => None,
        }
    }

    /// The keyword the next token is, if it is one.
    fn leading_keyword(&self) -> Option<Keyword> {
        match self.peek().kind {
            TokenKind::Keyword(keyword) => Some(keyword),
            _ => None,
        }
    }

    /// Whether the next token starts a declaration.
    fn at_type_keyword(&self) -> bool {
        matches!(
            self.leading_keyword(),
            Some(Keyword::Int | Keyword::Char | Keyword::Void)
        )
    }

    /// Whether the next token is `kind`.
    fn at(&self, kind: &TokenKind) -> bool {
        self.peek().kind == *kind
    }

    /// Whether the whole stream has been consumed.
    fn at_eof(&self) -> bool {
        self.at(&TokenKind::Eof)
    }

    /// Consume and return the next token. Consumes nothing once the end is reached.
    fn advance(&mut self) -> &Token {
        let index = self.position;
        if index < self.tokens.len() {
            self.position = index + 1;
        }

        self.token_at(index)
    }

    /// Consume the next token if it is `kind`, reporting whether it was.
    fn eat(&mut self, kind: &TokenKind) -> bool {
        let matched = self.at(kind);
        if matched {
            self.advance();
        }

        matched
    }

    /// Consume the next token, which must be `kind`, and return its span.
    fn expect(&mut self, kind: &TokenKind) -> Result<Span, Bail> {
        if self.at(kind) {
            return Ok(self.advance().span);
        }

        Err(self.expected(&format!("'{kind}'")))
    }
}

/// The C word outside this subset that `kind` spells, if it spells one.
fn unsupported_word(kind: &TokenKind) -> Option<&'static str> {
    match kind {
        TokenKind::Ident(text) => token::unsupported_keyword(text),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::ast::{self, Spans};
    use crate::lexer;

    /// Parse `source`, asserting it lexes and parses cleanly, and return its AST dump.
    fn dump(source: &str) -> String {
        let lexed = lexer::lex(source.as_bytes());
        assert!(
            lexed.diagnostics.is_empty(),
            "expected {source:?} to lex cleanly, got: {:?}",
            lexed.diagnostics
        );

        let parsed = parse(&lexed.tokens);
        assert!(
            parsed.diagnostics.is_empty(),
            "expected {source:?} to parse cleanly, got: {:?}",
            parsed.diagnostics
        );

        ast::dump(&parsed.program, Spans::Hidden)
    }

    /// Parse `source` and return its dump collapsed onto one line.
    ///
    /// Indentation is what makes a whole-program dump readable and what makes a one-construct
    /// assertion unreadable, so structural tests below compare the shape and nothing else. Only
    /// safe because none of them contain a string literal with a space in it.
    fn shape(source: &str) -> String {
        dump(source)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Parse `source` and return the messages it complained about, in source order.
    fn errors(source: &str) -> Vec<String> {
        let lexed = lexer::lex(source.as_bytes());
        assert!(
            lexed.diagnostics.is_empty(),
            "expected {source:?} to lex cleanly, got: {:?}",
            lexed.diagnostics
        );

        parse(&lexed.tokens)
            .diagnostics
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    /// Assert `source` produces exactly one diagnostic, reading `message`.
    fn assert_one_error(source: &str, message: &str) {
        assert_eq!(errors(source), [message.to_string()], "for {source:?}");
    }

    /// A program spanning most of the grammar, for the whole-tree properties below.
    const BROAD: &str = "int total;
         int add(int a, int b);
         int add(int a, int b) { return a + b; }
         int main(void) {
             int values[2] = {1, 2};
             for (int i = 0; i < 2; i = i + 1) {
                 if (values[i] > 0) { total = add(total, values[i]++); } else { continue; }
             }
             while (!total) { break; }
             return total;
         }";

    /// Every node the parser builds gets an identity no other node has. Phase 3 keys its
    /// annotations by these, so a collision would silently give two nodes the same type.
    #[test]
    fn the_parser_gives_every_node_a_distinct_id() {
        let lexed = lexer::lex(BROAD.as_bytes());
        let parsed = parse(&lexed.tokens);
        assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

        let nodes = ast::nodes(&parsed.program);
        let distinct: std::collections::HashSet<_> = nodes.iter().map(|(id, _)| *id).collect();

        assert!(nodes.len() > 40, "expected a program worth walking");
        assert_eq!(distinct.len(), nodes.len(), "ids repeat: {nodes:?}");
    }

    /// Every span the parser records is a real range inside the file it came from.
    #[test]
    fn every_span_points_into_the_source() {
        let lexed = lexer::lex(BROAD.as_bytes());
        let parsed = parse(&lexed.tokens);

        for (id, span) in ast::nodes(&parsed.program) {
            assert!(span.start <= span.end, "reversed span on {id:?}");
            assert!(span.end <= BROAD.len(), "span past the source on {id:?}");
            assert!(!span.is_empty(), "empty span on {id:?}");
        }
    }

    /// An empty file is a valid translation unit with nothing in it.
    #[test]
    fn an_empty_file_is_an_empty_program() {
        assert_eq!(dump(""), "(program)\n");
    }

    /// A function definition carries its return type, name, parameters, and body.
    #[test]
    fn a_function_definition_parses() {
        assert_eq!(
            dump("int main(void) { return 0; }"),
            concat!(
                "(program\n",
                "  (func-def int main\n",
                "    (params)\n",
                "    (block\n",
                "      (return\n",
                "        (int-lit 0)))))\n",
            )
        );
    }

    /// An empty parameter list and an explicit `(void)` mean the same thing.
    #[test]
    fn empty_and_void_parameter_lists_agree() {
        assert_eq!(dump("int f() { }"), dump("int f(void) { }"));
    }

    /// A declaration without a body is a forward declaration, and may be followed by the
    /// definition it promised.
    #[test]
    fn a_forward_declaration_precedes_its_definition() {
        assert_eq!(
            shape("int f(int n); int f(int n) { return n; }"),
            "(program (func-decl int f (params (param int n))) \
             (func-def int f (params (param int n)) (block (return (ident n)))))"
        );
    }

    /// Zero through nine parameters parse. Nine matters because the ninth crosses the
    /// eight-register boundary the ABI draws in Phase 4.
    #[test]
    fn functions_take_zero_through_nine_parameters() {
        for count in 0..=9 {
            let params: Vec<String> = (0..count).map(|index| format!("int p{index}")).collect();
            let source = format!("int f({}) {{ return 0; }}", params.join(", "));

            let dumped = dump(&source);
            assert_eq!(
                dumped.matches("(param ").count(),
                count,
                "for {count} parameters"
            );
        }
    }

    /// Every declaration form the grammar allows parses to the shape it describes.
    #[test]
    fn every_declaration_form_parses() {
        let cases = [
            ("int n;", "(program (global-var int n))"),
            (
                "int n = 1;",
                "(program (global-var int n (init (int-lit 1))))",
            ),
            ("int a[3];", "(program (global-var int[3] a))"),
            (
                "int a[3] = {1, 2, 3};",
                "(program (global-var int[3] a (init-list (int-lit 1) (int-lit 2) (int-lit 3))))",
            ),
            (
                "int a[1] = {};",
                "(program (global-var int[1] a (init-list)))",
            ),
            (
                "int a[2] = {1, 2,};",
                "(program (global-var int[2] a (init-list (int-lit 1) (int-lit 2))))",
            ),
            (
                "char c = 'x';",
                r"(program (global-var char c (init (char-lit 'x'))))",
            ),
            (
                "void f(int a[]) { }",
                "(program (func-def void f (params (param int[] a)) (block)))",
            ),
        ];

        for (source, expected) in cases {
            assert_eq!(shape(source), expected, "for {source:?}");
        }
    }

    /// Every statement form the grammar allows parses to the shape it describes.
    #[test]
    fn every_statement_form_parses() {
        let cases = [
            ("{ }", "(block)"),
            ("{ { } }", "(block (block))"),
            (";", "(empty)"),
            ("x;", "(expr-stmt (ident x))"),
            ("int y;", "(local-var int y)"),
            ("return;", "(return)"),
            ("return 1;", "(return (int-lit 1))"),
            ("break;", "(break)"),
            ("continue;", "(continue)"),
            ("if (a) b;", "(if (ident a) (then (expr-stmt (ident b))))"),
            (
                "if (a) b; else c;",
                "(if (ident a) (then (expr-stmt (ident b))) (else (expr-stmt (ident c))))",
            ),
            ("while (a) b;", "(while (ident a) (expr-stmt (ident b)))"),
        ];

        for (source, expected) in cases {
            let wrapped = format!("void f(void) {{ {source} }}");
            let expected = format!("(program (func-def void f (params) (block {expected})))");

            assert_eq!(shape(&wrapped), expected, "for {source:?}");
        }
    }

    /// A body may be a single statement without braces, at any of the three loop and branch forms.
    #[test]
    fn bodies_may_be_a_single_unbraced_statement() {
        for source in ["if (a) b;", "while (a) b;", "for (;;) b;"] {
            let wrapped = format!("void f(void) {{ {source} }}");

            assert!(!shape(&wrapped).contains("(block (block"), "for {source:?}");
        }
    }

    /// A dangling `else` binds to the nearest `if`, checked three deep so a rule that happened to
    /// work at two levels does not pass by luck.
    #[test]
    fn a_dangling_else_binds_to_the_nearest_if() {
        let shaped = shape("void f(void) { if (a) if (b) if (c) x; else y; }");

        assert_eq!(
            shaped,
            "(program (func-def void f (params) (block \
             (if (ident a) (then \
             (if (ident b) (then \
             (if (ident c) (then (expr-stmt (ident x))) (else (expr-stmt (ident y)))))))))))"
        );
    }

    /// All eight combinations of a present or absent `for` clause parse, each keeping its own slot
    /// in the dump so an omitted clause cannot be mistaken for a shifted one.
    #[test]
    fn every_combination_of_for_clauses_parses() {
        for combination in 0..8u8 {
            let init = if combination & 1 == 0 { "" } else { "i = 0" };
            let condition = if combination & 2 == 0 { "" } else { "i < 3" };
            let step = if combination & 4 == 0 {
                ""
            } else {
                "i = i + 1"
            };
            let source = format!("void f(void) {{ for ({init}; {condition}; {step}) x; }}");

            let shaped = shape(&source);
            assert_eq!(
                shaped.contains("(init (assign"),
                !init.is_empty(),
                "init, for {source:?}: {shaped}"
            );
            assert_eq!(
                shaped.contains("(cond (binary"),
                !condition.is_empty(),
                "condition, for {source:?}: {shaped}"
            );
            assert_eq!(
                shaped.contains("(step (assign"),
                !step.is_empty(),
                "step, for {source:?}: {shaped}"
            );
        }
    }

    /// A `for` initializer may declare its own variable.
    #[test]
    fn a_for_initializer_may_declare() {
        assert!(
            shape("void f(void) { for (int i = 0; i < 3; i = i + 1) x; }")
                .contains("(init (local-var int i (init (int-lit 0))))")
        );
    }

    /// A declaration is a block item, not a statement, so it may not be a branch or loop body.
    #[test]
    fn a_declaration_may_not_be_a_branch_body() {
        for source in [
            "void f(void) { if (a) int y = 1; }",
            "void f(void) { while (a) int y = 1; }",
            "void f(void) { for (;;) int y = 1; }",
        ] {
            assert_one_error(source, "a declaration is not allowed here");
        }
    }

    /// Each syntax error names what was wanted and what was there, in source spelling.
    #[test]
    fn syntax_errors_name_the_token_they_wanted() {
        let cases = [
            ("int f(void) { return 1 }", "expected ';', found '}'"),
            ("int f(void) { g(1; }", "expected ')', found ';'"),
            ("int f(void) { if x; }", "expected '(', found 'x'"),
            (
                "int f(void) { if () x; }",
                "expected an expression, found ')'",
            ),
            ("int f(void) { ; ", "expected '}', found end of file"),
            ("int 1;", "expected an identifier, found '1'"),
            ("1;", "expected a type, found '1'"),
            (
                "int f(void) { int a[] = {1}; }",
                "expected an integer literal for the array length",
            ),
            (
                "void f(int a[3]) { }",
                "an array parameter may not declare a length",
            ),
            ("int f(void) { else x; }", "'else' without a matching 'if'"),
        ];

        for (source, message) in cases {
            assert_one_error(source, message);
        }
    }

    /// A keyword where a name belongs is reported as the keyword it is.
    #[test]
    fn a_keyword_used_as_an_identifier_is_rejected() {
        assert_one_error("int while;", "expected an identifier, found 'while'");
        assert_one_error(
            "int f(void) { int return; }",
            "expected an identifier, found 'return'",
        );
    }

    /// Two independent mistakes in one function produce two diagnostics, not a cascade.
    #[test]
    fn two_errors_produce_exactly_two_diagnostics() {
        assert_eq!(
            errors("int f(void) { int a = ; int b = 2; int c = ; return b; }"),
            [
                "expected an expression, found ';'",
                "expected an expression, found ';'",
            ]
        );
    }

    /// Recovery resumes at the next statement, so what follows a mistake still parses.
    #[test]
    fn parsing_resumes_after_a_bad_statement() {
        let lexed = lexer::lex(b"int f(void) { int a = ; return 7; }");
        let parsed = parse(&lexed.tokens);

        assert_eq!(parsed.diagnostics.len(), 1);
        assert!(
            ast::dump(&parsed.program, Spans::Hidden).contains("(return\n        (int-lit 7))"),
            "the statement after the mistake should still be in the tree"
        );
    }

    /// A mistake in one function does not consume the next one.
    ///
    /// The missing semicolon leaves recovery staring at the `}` that closes `f`. Leaving that
    /// brace where it is, rather than skipping it as part of the wreckage, is what keeps `g`.
    #[test]
    fn recovery_does_not_swallow_the_following_function() {
        let lexed = lexer::lex(b"int f(void) { return 1 } int g(void) { return 1; }");
        let parsed = parse(&lexed.tokens);

        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(
            parsed.program.items.len(),
            2,
            "both functions should survive: {:?}",
            ast::dump(&parsed.program, Spans::Hidden)
        );
    }

    /// A brace left open at the end of the file is reported once, not once per line after it.
    #[test]
    fn an_unbalanced_brace_at_eof_reports_once() {
        assert_one_error(
            "int f(void) { int a = 1;",
            "expected '}', found end of file",
        );
    }

    /// Recovery cannot loop: a file of nothing but closing parentheses ends, having complained a
    /// bounded number of times.
    #[test]
    fn a_file_of_closing_parentheses_terminates() {
        let messages = errors(")))))))))))))))))))))))))))))))");

        assert!(!messages.is_empty());
        assert!(
            messages.len() < 5,
            "expected a bounded report, got {messages:?}"
        );
    }

    /// A file of nothing but braces ends too, whichever way they are unbalanced.
    #[test]
    fn a_file_of_braces_terminates() {
        for source in ["{{{{{{", "}}}}}}", "{}{}{}", "{ } } { {"] {
            let lexed = lexer::lex(source.as_bytes());
            let parsed = parse(&lexed.tokens);

            assert!(!parsed.diagnostics.is_empty(), "for {source:?}");
        }
    }

    /// Every C construct the subset leaves out is named as such, rather than reported as a
    /// program that does not parse.
    #[test]
    fn unsupported_constructs_are_named() {
        let cases = [
            ("struct point p;", "unsupported in this C subset: 'struct'"),
            ("union u v;", "unsupported in this C subset: 'union'"),
            ("enum color c;", "unsupported in this C subset: 'enum'"),
            (
                "typedef int word;",
                "unsupported in this C subset: 'typedef'",
            ),
            (
                "int f(void) { switch (n) { } }",
                "unsupported in this C subset: 'switch'",
            ),
            (
                "int f(void) { do { } while (n); }",
                "unsupported in this C subset: 'do'",
            ),
            (
                "int f(void) { return sizeof(n); }",
                "unsupported in this C subset: 'sizeof'",
            ),
            ("float x;", "unsupported in this C subset: 'float'"),
            ("double x;", "unsupported in this C subset: 'double'"),
            ("long x;", "unsupported in this C subset: 'long'"),
            ("unsigned x;", "unsupported in this C subset: 'unsigned'"),
            ("short x;", "unsupported in this C subset: 'short'"),
            ("signed x;", "unsupported in this C subset: 'signed'"),
            ("const int x = 1;", "unsupported in this C subset: 'const'"),
            ("static int x;", "unsupported in this C subset: 'static'"),
            ("extern int x;", "unsupported in this C subset: 'extern'"),
            (
                "int *p;",
                "unsupported in this C subset: pointer declarators",
            ),
            (
                "int f(char *s) { return 0; }",
                "unsupported in this C subset: pointer declarators",
            ),
        ];

        for (source, message) in cases {
            let messages = errors(source);
            assert_eq!(
                messages.first().map(String::as_str),
                Some(message),
                "for {source:?}, got {messages:?}"
            );
        }
    }

    /// A type definition is one diagnostic, not one for the keyword and another for the `};` left
    /// behind after recovery skipped its body.
    #[test]
    fn an_out_of_subset_type_definition_reports_once() {
        for source in [
            "struct point { int x; int y; };",
            "union value { int i; char c; };",
            "enum color { red, green };",
            "typedef int word;",
        ] {
            assert_eq!(errors(source).len(), 1, "for {source:?}");
        }
    }

    /// A definition the subset lacks does not take the declarations after it down with it.
    #[test]
    fn parsing_resumes_after_an_unsupported_definition() {
        let lexed = lexer::lex(b"struct point { int x; };\nint main(void) { return 0; }");
        let parsed = parse(&lexed.tokens);

        assert_eq!(parsed.diagnostics.len(), 1);
        assert_eq!(parsed.program.items.len(), 1, "main should still parse");
    }

    /// Nesting past the limit is a diagnostic rather than a stack overflow.
    #[test]
    fn nesting_past_the_limit_is_reported() {
        let depth = MAX_NESTING_DEPTH * 4;
        let source = format!(
            "int f(void) {{ return {}1{}; }}",
            "(".repeat(depth),
            ")".repeat(depth)
        );

        let messages = errors(&source);
        assert!(
            messages
                .first()
                .is_some_and(|message| message.starts_with("nesting is too deep")),
            "got {messages:?}"
        );
    }

    /// Deeply nested blocks are bounded by the same limit, not only expressions.
    #[test]
    fn deeply_nested_blocks_are_reported() {
        let depth = MAX_NESTING_DEPTH * 4;
        let source = format!(
            "int f(void) {{ {}{} }}",
            "{".repeat(depth),
            "}".repeat(depth)
        );

        let messages = errors(&source);
        assert!(
            messages
                .first()
                .is_some_and(|message| message.starts_with("nesting is too deep")),
            "got {messages:?}"
        );
    }

    /// The depth limit leaves real stack to spare, checked on a stack far smaller than any the
    /// parser actually runs on.
    ///
    /// The point of the guard is that recursion cannot exhaust the stack, and a limit tuned so
    /// finely that it only just fits would not deliver that — it would move the crash rather than
    /// remove it. So this parses input deep enough to reach the limit inside a thread given a
    /// quarter of what the test harness hands out by default. A change that fattens a parse frame
    /// enough to matter fails here, loudly, instead of on a user's file.
    #[test]
    fn nesting_stays_within_a_small_stack() {
        /// A quarter of the 2 MiB a test thread gets, and a sixteenth of the binary's main stack.
        const SMALL_STACK: usize = 512 * 1024;

        let source = format!(
            "int f(void) {{ return {}1{}; }}",
            "(".repeat(MAX_NESTING_DEPTH * 4),
            ")".repeat(MAX_NESTING_DEPTH * 4)
        );

        let parsed_on_a_small_stack = std::thread::Builder::new()
            .stack_size(SMALL_STACK)
            .spawn(move || errors(&source))
            .expect("could not spawn the thread")
            .join()
            .expect("parsing overflowed a small stack, so the depth limit is set too high");

        assert!(
            parsed_on_a_small_stack
                .first()
                .is_some_and(|message| message.starts_with("nesting is too deep")),
            "got {parsed_on_a_small_stack:?}"
        );
    }

    /// Nesting that stays inside the limit still parses, so the guard rejects only what it must.
    #[test]
    fn nesting_within_the_limit_parses() {
        let depth = MAX_NESTING_DEPTH / 4;
        let source = format!(
            "int f(void) {{ return {}1{}; }}",
            "(".repeat(depth),
            ")".repeat(depth)
        );

        assert!(dump(&source).contains("(int-lit 1)"));
    }

    /// An empty token slice is a valid, empty parse rather than an out-of-bounds read. The Phase 5
    /// fuzz targets can hand the parser one, so it may not assume the lexer's trailing `Eof`.
    #[test]
    fn an_empty_token_slice_parses() {
        let parsed = parse(&[]);

        assert!(parsed.program.items.is_empty());
        assert!(parsed.diagnostics.is_empty());
    }
}
