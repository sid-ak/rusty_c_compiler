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
//! recursive entry point goes through `Parser::nested`, and every loop that makes the tree deeper
//! without recursing — an operator chain, a postfix chain — charges a level per pass through
//! `Parser::deepen`, so the tree itself never passes [`MAX_NESTING_DEPTH`] and neither does any
//! later pass that walks it.

pub mod expr;

use std::fmt;

use crate::ast::{
    BaseType, Block, ForInit, FuncDecl, FuncDef, FuncSig, Initializer, Item, Name, NodeId, NodeIds,
    Param, Program, Stmt, StmtKind, TypeSpec, VarDecl,
};
use crate::diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, Span};
use crate::lexer::token;
use crate::lexer::{Keyword, Token, TokenKind};

/// How many levels deep the parser will let the syntax tree grow before reporting instead.
///
/// The limit counts levels of the tree the parser builds rather than brackets: a parenthesized
/// expression costs two, a nested block one, and each operator in a chain like `1 + 1 + 1` or
/// `a[0][0]` one. Counting the tree rather than only the parser's own recursion is what protects
/// every pass that walks the tree afterwards — the dump, dropping it, and later analysis and code
/// generation all recurse once per level. Real C reaches nothing close to it — the deepest
/// expression anyone writes by hand is a handful of levels — so the limit is only ever met by
/// generated or hostile input, which is exactly the case it exists to turn into a diagnostic.
///
/// The number is a stack budget, not a taste in style, and it was measured rather than guessed. In
/// an unoptimized build the costliest shape, nested blocks, needs about 550 KB of stack to parse,
/// dump, and drop at the limit, and nested parentheses about 400 KB — inside the 2 MiB stack the
/// test harness gives a thread, and far inside the 8 MiB the binary's main thread has. The
/// `deep_input_stays_within_a_small_stack` test holds that margin to a fixed 1 MiB figure for every
/// shape, so a change that makes a frame fatter fails there rather than as a crash on someone's
/// input.
pub const MAX_NESTING_DEPTH: usize = 128;

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
    /// How many levels deep in the tree the construct being parsed sits.
    depth: usize,
    /// Hands out the identity of each node built.
    ids: NodeIds,
    /// Problems found so far.
    diagnostics: DiagnosticBag,
    /// The token every lookup past the end of the stream reports: `Eof`, just after the last token.
    ///
    /// The lexer ends its stream with `Eof`, but `parse` is public and may be handed a slice that
    /// does not — the Phase 5 fuzz targets do exactly that. Answering past-the-end lookups with an
    /// `Eof` of the parser's own, rather than repeating the last real token, is what lets every
    /// `while !at_eof()` loop end on such a slice, and placing it where the input stops keeps a
    /// diagnostic about the missing rest pointing at the end of the file rather than its start.
    end_of_stream: Token,
}

impl<'tokens> Parser<'tokens> {
    /// A parser positioned at the start of `tokens`.
    fn new(tokens: &'tokens [Token]) -> Self {
        let end = tokens.last().map_or(0, |last| last.span.end);

        Self {
            tokens,
            end_of_stream: Token::new(TokenKind::Eof, Span::empty_at(end)),
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
        self.restoring_depth(|parser| {
            parser.deepen()?;
            parse(parser)
        })
    }

    /// Run `parse`, then put the depth back to what it was before, however `parse` returned.
    ///
    /// Every level charged with [`Parser::deepen`] happens inside one of these, so no way out of a
    /// construct — a finished parse, a `?` bail, an early error return — can leave its levels
    /// charged to whatever the parser reads next.
    fn restoring_depth<T>(
        &mut self,
        parse: impl FnOnce(&mut Self) -> Result<T, Bail>,
    ) -> Result<T, Bail> {
        let entry = self.depth;
        let parsed = parse(self);
        self.depth = entry;

        parsed
    }

    /// Charge one level of tree depth, reporting instead once the tree would pass the limit.
    ///
    /// Recursion charges a level through [`Parser::nested`]. A loop that makes the tree deeper on
    /// each pass without recursing — a left-associative operator chain, a postfix chain — calls this
    /// directly once per pass, inside [`Parser::restoring_depth`].
    fn deepen(&mut self) -> Result<(), Bail> {
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            return Err(self.too_deep());
        }

        Ok(())
    }

    /// Report that the input nests deeper than the syntax tree is allowed to go.
    fn too_deep(&mut self) -> Bail {
        let span = self.here();

        self.report(
            Diagnostic::parse(
                span,
                format!("nesting is too deep: the syntax tree goes at most {MAX_NESTING_DEPTH} levels deep"),
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
        self.tokens.get(index).unwrap_or(&self.end_of_stream)
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
mod tests;
