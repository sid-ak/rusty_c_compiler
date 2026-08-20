//! The abstract syntax tree: what the parser builds, and what every later pass reads.
//!
//! The tree is plain data. No node holds a resolved type, a stack offset, or anything else a later
//! pass works out — semantic analysis records those in a side table keyed by [`NodeId`], and the
//! reasoning behind that split is in `docs/decisions/0004-immutable-ast-with-side-table-annotations.md`.
//! The practical consequence shows up here as an absence: there is no `Option<Ty>` field to be
//! `None` before analysis and `Some` after, so a snapshot of this tree means the same thing in
//! Phase 2 as it will in Phase 5.
//!
//! # What counts as a node
//!
//! A node is something a later pass can have an opinion about, and it carries a [`NodeId`] and a
//! [`Span`]: items, parameters, declarations, statements, and expressions. A type written in a
//! declaration ([`TypeSpec`]) and the name it declares ([`Name`]) carry a span but no id — they are
//! part of the declaration that spells them out, not things analysis annotates on their own.
//!
//! # The dump
//!
//! [`dump`] renders a program as an S-expression, one node per line. It is what `--dump-ast`
//! prints and what parser tests assert against, because operator precedence can only be checked by
//! looking at the shape of the tree: asserting that `1+2*3` evaluates to `7` would also pass if
//! precedence were wrong in a way that happened to cancel out.

use std::fmt;

use crate::diagnostics::Span;
use crate::lexer::token;

/// A node's identity, unique within one parsed program.
///
/// Semantic analysis keys its annotations by this rather than writing them onto the node, so the
/// tree a parser snapshot captured cannot change under it later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(u32);

impl NodeId {
    /// The id as a plain number, for use as a table key.
    pub fn index(self) -> u32 {
        self.0
    }
}

/// Hands out [`NodeId`]s in the order the parser builds nodes.
///
/// The counter saturates rather than overflowing, so the no-panic invariant holds even in the
/// arithmetic. Reaching the cap needs more than four billion nodes in one file, and a node costs
/// at least one byte of source, so no file that fits on a disk can get there.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NodeIds {
    next: u32,
}

impl NodeIds {
    /// A fresh counter, starting from zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// The next unused id.
    pub fn next_id(&mut self) -> NodeId {
        let id = NodeId(self.next);
        self.next = self.next.saturating_add(1);

        id
    }
}

/// The three types a declaration in this subset can name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BaseType {
    /// `int`
    Int,
    /// `char`
    Char,
    /// `void`
    Void,
}

impl BaseType {
    /// Every base type, in declaration order.
    pub const ALL: [BaseType; 3] = [BaseType::Int, BaseType::Char, BaseType::Void];

    /// How this type is spelled in source.
    pub fn spelling(self) -> &'static str {
        match self {
            BaseType::Int => "int",
            BaseType::Char => "char",
            BaseType::Void => "void",
        }
    }
}

/// A type as it was written in a declaration.
///
/// The three shapes the grammar allows — a scalar, an array with a length, and an array without
/// one — are built through the constructors below rather than by filling in fields, so the
/// combination that means nothing (a length *and* no length) is never constructed.
///
/// `is_unsized_array` is how `int a[]` is carried until Phase 3, which is the pass that turns a
/// parameter written that way into a pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSpec {
    /// The element type, or the whole type when this is not an array.
    pub base: BaseType,
    /// The length of a sized array, `int a[3]`.
    pub array_len: Option<u32>,
    /// Whether it was written as an array with no length, `int a[]`.
    pub is_unsized_array: bool,
    /// The source it was written across, including any brackets.
    pub span: Span,
}

impl TypeSpec {
    /// A plain `int`, `char`, or `void`.
    pub fn scalar(base: BaseType, span: Span) -> Self {
        Self {
            base,
            array_len: None,
            is_unsized_array: false,
            span,
        }
    }

    /// An array with a declared length, `int a[3]`.
    pub fn array(base: BaseType, len: u32, span: Span) -> Self {
        Self {
            base,
            array_len: Some(len),
            is_unsized_array: false,
            span,
        }
    }

    /// An array parameter written without a length, `int a[]`.
    pub fn unsized_array(base: BaseType, span: Span) -> Self {
        Self {
            base,
            array_len: None,
            is_unsized_array: true,
            span,
        }
    }

    /// Whether this names an array at all, sized or not.
    pub fn is_array(&self) -> bool {
        self.array_len.is_some() || self.is_unsized_array
    }
}

impl fmt::Display for TypeSpec {
    /// The type as a C programmer would write it: `int`, `int[3]`, `int[]`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.base.spelling())?;

        match (self.array_len, self.is_unsized_array) {
            (Some(len), _) => write!(formatter, "[{len}]"),
            (None, true) => formatter.write_str("[]"),
            (None, false) => Ok(()),
        }
    }
}

/// An identifier as it was written: the text, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Name {
    /// The identifier itself.
    pub text: String,
    /// Where it was written.
    pub span: Span,
}

/// One translation unit: everything in one source file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Program {
    /// The file's top-level declarations, in source order.
    pub items: Vec<Item>,
}

/// A top-level declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    /// A function with a body.
    FuncDef(FuncDef),
    /// A function declared without a body, so it can be called before it is defined.
    FuncDecl(FuncDecl),
    /// A variable at file scope.
    GlobalVar(VarDecl),
}

impl Item {
    /// This item's identity.
    pub fn id(&self) -> NodeId {
        match self {
            Item::FuncDef(def) => def.id,
            Item::FuncDecl(decl) => decl.id,
            Item::GlobalVar(var) => var.id,
        }
    }

    /// The source this item was written across.
    pub fn span(&self) -> Span {
        match self {
            Item::FuncDef(def) => def.span,
            Item::FuncDecl(decl) => decl.span,
            Item::GlobalVar(var) => var.span,
        }
    }
}

/// What a function returns, what it is called, and what it takes.
///
/// Shared by the two forms a function can appear in, which differ only in whether a body follows,
/// so the two can never disagree about what a signature is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncSig {
    /// The declared return type.
    pub return_type: TypeSpec,
    /// The function's name.
    pub name: Name,
    /// Its parameters. Empty for both `()` and `(void)`, which mean the same thing here.
    pub params: Vec<Param>,
    /// From the return type through the closing parenthesis.
    pub span: Span,
}

/// A function with a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncDef {
    /// This node's identity.
    pub id: NodeId,
    /// What it returns, what it is called, what it takes.
    pub signature: FuncSig,
    /// The body. It has no id of its own; this node's covers it.
    pub body: Block,
    /// From the return type through the closing brace.
    pub span: Span,
}

/// A function declared without a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncDecl {
    /// This node's identity.
    pub id: NodeId,
    /// What it returns, what it is called, what it takes.
    pub signature: FuncSig,
    /// From the return type through the semicolon.
    pub span: Span,
}

/// One parameter of a function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    /// This node's identity.
    pub id: NodeId,
    /// Its declared type.
    pub ty: TypeSpec,
    /// Its name.
    pub name: Name,
    /// The whole parameter.
    pub span: Span,
}

/// A variable declaration, at file scope or inside a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VarDecl {
    /// This node's identity.
    pub id: NodeId,
    /// Its declared type.
    pub ty: TypeSpec,
    /// Its name.
    pub name: Name,
    /// Its initializer, if it was given one.
    pub init: Option<Initializer>,
    /// From the type through the initializer. The terminating semicolon is not included, so a
    /// declaration in a `for` clause and one in a block span the same thing.
    pub span: Span,
}

/// What a declaration initializes its variable to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initializer {
    /// A single expression, `int n = 10;`.
    Expr(Expr),
    /// A brace list, `int a[3] = {1, 2, 3};`.
    List {
        /// The elements, in source order. May be empty, `{}`.
        elements: Vec<Expr>,
        /// From the opening brace through the closing one.
        span: Span,
    },
}

impl Initializer {
    /// The source this initializer was written across.
    pub fn span(&self) -> Span {
        match self {
            Initializer::Expr(expr) => expr.span,
            Initializer::List { span, .. } => *span,
        }
    }
}

/// A braced sequence of statements.
///
/// A block has no id of its own: a function body belongs to its [`FuncDef`] and a nested block to
/// the [`Stmt`] that holds it, so there is nothing left for a second id to identify.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The statements and declarations inside, in source order.
    pub stmts: Vec<Stmt>,
    /// From the opening brace through the closing one.
    pub span: Span,
}

/// One statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stmt {
    /// This node's identity.
    pub id: NodeId,
    /// Which statement it is.
    pub kind: StmtKind,
    /// The source it was written across.
    pub span: Span,
}

/// Which statement a [`Stmt`] is.
///
/// [`StmtKind::LocalVar`] is representable anywhere a statement is, but the grammar admits a
/// declaration only as a block item or a `for` initializer — the parser enforces that, so
/// `if (x) int y = 1;` is rejected rather than built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmtKind {
    /// A nested block.
    Block(Block),
    /// `if (condition) then_branch [else else_branch]`.
    If {
        /// The tested expression.
        condition: Expr,
        /// Run when the condition is non-zero.
        then_branch: Box<Stmt>,
        /// Run otherwise, if an `else` was written.
        else_branch: Option<Box<Stmt>>,
    },
    /// `while (condition) body`.
    While {
        /// Tested before each iteration.
        condition: Expr,
        /// The loop body.
        body: Box<Stmt>,
    },
    /// `for (init; condition; step) body`, with any of the three clauses omitted.
    For {
        /// Run once before the loop; a declaration or an expression.
        init: Option<Box<ForInit>>,
        /// Tested before each iteration. An absent condition loops forever.
        condition: Option<Expr>,
        /// Run after each iteration.
        step: Option<Expr>,
        /// The loop body.
        body: Box<Stmt>,
    },
    /// `return;` or `return expr;`.
    Return(Option<Expr>),
    /// `break;`
    Break,
    /// `continue;`
    Continue,
    /// An expression evaluated for its effect, `f(x);`.
    Expr(Expr),
    /// A variable declared inside a block.
    LocalVar(VarDecl),
    /// A lone `;`.
    Empty,
}

/// The first clause of a `for`, which may declare a variable or evaluate an expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForInit {
    /// `for (int i = 0; ...)`.
    Decl(VarDecl),
    /// `for (i = 0; ...)`.
    Expr(Expr),
}

impl ForInit {
    /// The source this clause was written across.
    pub fn span(&self) -> Span {
        match self {
            ForInit::Decl(decl) => decl.span,
            ForInit::Expr(expr) => expr.span,
        }
    }
}

/// One expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
    /// This node's identity.
    pub id: NodeId,
    /// Which expression it is.
    pub kind: ExprKind,
    /// The source it was written across.
    pub span: Span,
}

/// Which expression an [`Expr`] is.
///
/// There is no variant for a parenthesized expression. Parentheses exist to group, and once the
/// tree records the grouping they have nothing left to say — which is why `((((1))))` and `1`
/// produce the same tree, and why `(a) = 1` is assignable exactly as `a = 1` is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
    /// An integer literal, already decoded from its base by the lexer.
    IntLit(i32),
    /// A character literal, already decoded to the byte it denotes.
    CharLit(u8),
    /// A string literal, already decoded, without a trailing NUL.
    StrLit(Vec<u8>),
    /// A name being used as a value.
    Ident(String),
    /// A prefix operator applied to one operand.
    Unary {
        /// Which operator.
        op: UnOp,
        /// What it applies to.
        operand: Box<Expr>,
    },
    /// Two operands joined by an operator.
    Binary {
        /// Which operator.
        op: BinOp,
        /// The left operand.
        left: Box<Expr>,
        /// The right operand.
        right: Box<Expr>,
    },
    /// `target = value`.
    Assign {
        /// What is assigned to.
        target: Box<Expr>,
        /// What it is assigned.
        value: Box<Expr>,
    },
    /// `base[index]`.
    Index {
        /// The array being indexed.
        base: Box<Expr>,
        /// The subscript.
        index: Box<Expr>,
    },
    /// `callee(args)`.
    Call {
        /// What is being called.
        callee: Box<Expr>,
        /// The arguments, in source order.
        args: Vec<Expr>,
    },
    /// `operand++` or `operand--`.
    PostfixIncDec {
        /// Which of the two.
        op: IncDec,
        /// What it applies to.
        operand: Box<Expr>,
    },
}

/// A binary operator.
///
/// Exactly the operators the grammar has, so an operator this subset omits cannot be represented,
/// let alone reach code generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Subtract,
    /// `*`
    Multiply,
    /// `/`
    Divide,
    /// `%`
    Remainder,
    /// `==`
    Equal,
    /// `!=`
    NotEqual,
    /// `<`
    Less,
    /// `>`
    Greater,
    /// `<=`
    LessEqual,
    /// `>=`
    GreaterEqual,
    /// `&&`
    And,
    /// `||`
    Or,
}

impl BinOp {
    /// Every binary operator, loosest-binding first.
    pub const ALL: [BinOp; 13] = [
        BinOp::Or,
        BinOp::And,
        BinOp::Equal,
        BinOp::NotEqual,
        BinOp::Less,
        BinOp::Greater,
        BinOp::LessEqual,
        BinOp::GreaterEqual,
        BinOp::Add,
        BinOp::Subtract,
        BinOp::Multiply,
        BinOp::Divide,
        BinOp::Remainder,
    ];

    /// How this operator is spelled in source.
    pub fn spelling(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Subtract => "-",
            BinOp::Multiply => "*",
            BinOp::Divide => "/",
            BinOp::Remainder => "%",
            BinOp::Equal => "==",
            BinOp::NotEqual => "!=",
            BinOp::Less => "<",
            BinOp::Greater => ">",
            BinOp::LessEqual => "<=",
            BinOp::GreaterEqual => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
        }
    }
}

/// A prefix operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    /// `-`
    Negate,
    /// `!`
    Not,
    /// `+`
    Plus,
    /// `++`, applied before the value is read.
    PreIncrement,
    /// `--`, applied before the value is read.
    PreDecrement,
}

impl UnOp {
    /// Every prefix operator, in grammar order.
    pub const ALL: [UnOp; 5] = [
        UnOp::Negate,
        UnOp::Not,
        UnOp::Plus,
        UnOp::PreIncrement,
        UnOp::PreDecrement,
    ];

    /// How this operator is spelled in source.
    pub fn spelling(self) -> &'static str {
        match self {
            UnOp::Negate => "-",
            UnOp::Not => "!",
            UnOp::Plus => "+",
            UnOp::PreIncrement => "++",
            UnOp::PreDecrement => "--",
        }
    }
}

/// A postfix `++` or `--`, applied after the value is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IncDec {
    /// `++`
    Increment,
    /// `--`
    Decrement,
}

impl IncDec {
    /// Both forms, in grammar order.
    pub const ALL: [IncDec; 2] = [IncDec::Increment, IncDec::Decrement];

    /// How this operator is spelled in source.
    pub fn spelling(self) -> &'static str {
        match self {
            IncDec::Increment => "++",
            IncDec::Decrement => "--",
        }
    }
}

/// Whether a dump shows each node's source range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Spans {
    /// Leave them out, which is what keeps a snapshot readable.
    #[default]
    Hidden,
    /// Show each node's byte range as `@start..end`.
    Shown,
}

/// Render `program` as an S-expression, one node per line.
///
/// The output is a function of the tree alone — no hash, no address, no iteration over a map — so
/// two runs over the same source produce the same bytes, which is what makes it usable as a
/// snapshot. Ends with a newline, as a file dump should.
pub fn dump(program: &Program, spans: Spans) -> String {
    render(&program_tree(program), spans)
}

/// Render a single expression the same way [`dump`] renders a program.
///
/// Useful where a whole program would be scaffolding around the thing under test: a precedence
/// assertion is about one expression, and wrapping it in a function to dump it would bury the
/// shape being checked.
pub fn dump_expression(expr: &Expr, spans: Spans) -> String {
    render(&expr_tree(expr), spans)
}

/// Every node in `program` as an (id, span) pair, in the order the parser built them.
///
/// Phase 3 keys its annotations by [`NodeId`], so being able to enumerate what a parse produced is
/// what lets a test prove those keys are unique before anything relies on them.
pub fn nodes(program: &Program) -> Vec<(NodeId, Span)> {
    let mut found = Vec::new();
    collect(&program_tree(program), &mut found);

    found
}

/// One line of a dump: what the node is, what it covers, and what sits beneath it.
///
/// Building this intermediate tree rather than writing text directly is what lets [`dump`] and
/// [`nodes`] share a single walk of the AST. There is one place that knows the shape of every node
/// type, so a variant added without a dump is a compile error rather than a silently missing line.
#[derive(Debug)]
struct DumpNode {
    /// The head of the S-expression: the node's name and any inline detail.
    head: String,
    /// The AST node this came from, or `None` for a grouping line such as `(params ...)`.
    id: Option<NodeId>,
    /// What it covers, or `None` for a grouping line.
    span: Option<Span>,
    /// The nodes beneath it.
    children: Vec<DumpNode>,
}

impl DumpNode {
    /// A line standing for a real AST node.
    fn node(head: impl Into<String>, id: NodeId, span: Span, children: Vec<DumpNode>) -> Self {
        Self {
            head: head.into(),
            id: Some(id),
            span: Some(span),
            children,
        }
    }

    /// A line that only groups its children, such as `(params ...)` or `(then ...)`.
    fn group(head: impl Into<String>, children: Vec<DumpNode>) -> Self {
        Self {
            head: head.into(),
            id: None,
            span: None,
            children,
        }
    }
}

/// Write `node` and everything under it, one node per line, two spaces per level.
fn render(node: &DumpNode, spans: Spans) -> String {
    let mut rendered = String::new();
    write_node(node, 0, spans, &mut rendered);
    rendered.push('\n');

    rendered
}

/// Append `node` at `indent`, then its children one level deeper.
fn write_node(node: &DumpNode, indent: usize, spans: Spans, out: &mut String) {
    out.extend(std::iter::repeat_n(' ', indent));
    out.push('(');
    out.push_str(&node.head);

    if let (Spans::Shown, Some(span)) = (spans, node.span) {
        out.push_str(&format!("@{}..{}", span.start, span.end));
    }

    for child in &node.children {
        out.push('\n');
        write_node(child, indent + 2, spans, out);
    }

    out.push(')');
}

/// Collect the identified nodes of this subtree, parents before children.
fn collect(node: &DumpNode, out: &mut Vec<(NodeId, Span)>) {
    if let (Some(id), Some(span)) = (node.id, node.span) {
        out.push((id, span));
    }

    for child in &node.children {
        collect(child, out);
    }
}

/// The dump tree for a whole program.
fn program_tree(program: &Program) -> DumpNode {
    DumpNode::group("program", program.items.iter().map(item_tree).collect())
}

/// The dump tree for one top-level item.
fn item_tree(item: &Item) -> DumpNode {
    match item {
        Item::FuncDef(def) => DumpNode::node(
            signature_head("func-def", &def.signature),
            def.id,
            def.span,
            vec![params_tree(&def.signature), block_tree(&def.body)],
        ),
        Item::FuncDecl(decl) => DumpNode::node(
            signature_head("func-decl", &decl.signature),
            decl.id,
            decl.span,
            vec![params_tree(&decl.signature)],
        ),
        Item::GlobalVar(var) => var_tree("global-var", var),
    }
}

/// `func-def int main`: the head line of a function, whichever form it takes.
fn signature_head(head: &str, signature: &FuncSig) -> String {
    format!("{head} {} {}", signature.return_type, signature.name.text)
}

/// `(params ...)`, empty when the function takes none.
fn params_tree(signature: &FuncSig) -> DumpNode {
    DumpNode::group(
        "params",
        signature
            .params
            .iter()
            .map(|param| {
                DumpNode::node(
                    format!("param {} {}", param.ty, param.name.text),
                    param.id,
                    param.span,
                    Vec::new(),
                )
            })
            .collect(),
    )
}

/// The dump tree for a declaration, named `global-var` or `local-var` by where it was written.
fn var_tree(head: &str, var: &VarDecl) -> DumpNode {
    let children = match &var.init {
        Some(Initializer::Expr(expr)) => vec![DumpNode::group("init", vec![expr_tree(expr)])],
        Some(Initializer::List { elements, .. }) => vec![DumpNode::group(
            "init-list",
            elements.iter().map(expr_tree).collect(),
        )],
        None => Vec::new(),
    };

    DumpNode::node(
        format!("{head} {} {}", var.ty, var.name.text),
        var.id,
        var.span,
        children,
    )
}

/// The dump tree for a block.
fn block_tree(block: &Block) -> DumpNode {
    DumpNode::group("block", block.stmts.iter().map(stmt_tree).collect())
}

/// The dump tree for one statement.
fn stmt_tree(stmt: &Stmt) -> DumpNode {
    let (head, children): (&str, Vec<DumpNode>) = match &stmt.kind {
        StmtKind::Block(block) => ("block", block.stmts.iter().map(stmt_tree).collect()),
        StmtKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            // The branches are wrapped rather than left bare so an `if` with an `else` and one
            // without differ by more than how many children happen to follow the condition.
            let mut children = vec![
                expr_tree(condition),
                DumpNode::group("then", vec![stmt_tree(then_branch)]),
            ];
            if let Some(branch) = else_branch {
                children.push(DumpNode::group("else", vec![stmt_tree(branch)]));
            }
            ("if", children)
        }
        StmtKind::While { condition, body } => {
            ("while", vec![expr_tree(condition), stmt_tree(body)])
        }
        StmtKind::For {
            init,
            condition,
            step,
            body,
        } => {
            // Each clause keeps its own line whether or not it was written, so the eight
            // combinations of present and absent read off the dump directly.
            let init = match init.as_deref() {
                Some(ForInit::Decl(decl)) => vec![var_tree("local-var", decl)],
                Some(ForInit::Expr(expr)) => vec![expr_tree(expr)],
                None => Vec::new(),
            };
            (
                "for",
                vec![
                    DumpNode::group("init", init),
                    DumpNode::group("cond", condition.iter().map(expr_tree).collect()),
                    DumpNode::group("step", step.iter().map(expr_tree).collect()),
                    stmt_tree(body),
                ],
            )
        }
        StmtKind::Return(value) => ("return", value.iter().map(expr_tree).collect()),
        StmtKind::Break => ("break", Vec::new()),
        StmtKind::Continue => ("continue", Vec::new()),
        StmtKind::Expr(expr) => ("expr-stmt", vec![expr_tree(expr)]),
        StmtKind::LocalVar(decl) => return var_tree("local-var", decl),
        StmtKind::Empty => ("empty", Vec::new()),
    };

    DumpNode::node(head, stmt.id, stmt.span, children)
}

/// The dump tree for one expression.
fn expr_tree(expr: &Expr) -> DumpNode {
    let (head, children): (String, Vec<DumpNode>) = match &expr.kind {
        ExprKind::IntLit(value) => (format!("int-lit {value}"), Vec::new()),
        ExprKind::CharLit(byte) => (
            format!("char-lit '{}'", token::spell_literal(&[*byte])),
            Vec::new(),
        ),
        ExprKind::StrLit(bytes) => (
            format!("str-lit \"{}\"", token::spell_literal(bytes)),
            Vec::new(),
        ),
        ExprKind::Ident(name) => (format!("ident {name}"), Vec::new()),
        ExprKind::Unary { op, operand } => {
            (format!("unary {}", op.spelling()), vec![expr_tree(operand)])
        }
        ExprKind::Binary { op, left, right } => (
            format!("binary {}", op.spelling()),
            vec![expr_tree(left), expr_tree(right)],
        ),
        ExprKind::Assign { target, value } => (
            "assign".to_string(),
            vec![expr_tree(target), expr_tree(value)],
        ),
        ExprKind::Index { base, index } => {
            ("index".to_string(), vec![expr_tree(base), expr_tree(index)])
        }
        ExprKind::Call { callee, args } => (
            "call".to_string(),
            vec![
                expr_tree(callee),
                DumpNode::group("args", args.iter().map(expr_tree).collect()),
            ],
        ),
        ExprKind::PostfixIncDec { op, operand } => (
            format!("postfix {}", op.spelling()),
            vec![expr_tree(operand)],
        ),
    };

    DumpNode::node(head, expr.id, expr.span, children)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashSet;

    /// A span standing in for wherever a hand-built node would have come from.
    ///
    /// The trees below are built rather than parsed, so that this module tests the node types and
    /// the dumper on their own. What the parser puts in a span is the parser's test to write.
    const ANYWHERE: Span = Span { start: 0, end: 1 };

    /// Builds the sample trees, handing out an identity to each node as the parser would.
    struct Builder {
        ids: NodeIds,
    }

    impl Builder {
        /// A builder with no ids handed out yet.
        fn new() -> Self {
            Self {
                ids: NodeIds::new(),
            }
        }

        /// An expression of `kind`.
        fn expr(&mut self, kind: ExprKind) -> Expr {
            Expr {
                id: self.ids.next_id(),
                kind,
                span: ANYWHERE,
            }
        }

        /// The integer literal `value`.
        fn int(&mut self, value: i32) -> Expr {
            self.expr(ExprKind::IntLit(value))
        }

        /// The name `text` used as a value.
        fn ident(&mut self, text: &str) -> Expr {
            self.expr(ExprKind::Ident(text.to_string()))
        }

        /// A statement of `kind`.
        fn stmt(&mut self, kind: StmtKind) -> Stmt {
            Stmt {
                id: self.ids.next_id(),
                kind,
                span: ANYWHERE,
            }
        }

        /// A variable named `name` of type `ty`, optionally initialized.
        fn var(&mut self, ty: TypeSpec, name: &str, init: Option<Initializer>) -> VarDecl {
            VarDecl {
                id: self.ids.next_id(),
                ty,
                name: self.name(name),
                init,
                span: ANYWHERE,
            }
        }

        /// A parameter named `name` of type `ty`.
        fn param(&mut self, ty: TypeSpec, name: &str) -> Param {
            Param {
                id: self.ids.next_id(),
                ty,
                name: self.name(name),
                span: ANYWHERE,
            }
        }

        /// A written identifier.
        fn name(&self, text: &str) -> Name {
            Name {
                text: text.to_string(),
                span: ANYWHERE,
            }
        }

        /// A function returning `int`, named `name`, taking `params`, whose body is `stmts`.
        fn func(&mut self, name: &str, params: Vec<Param>, stmts: Vec<Stmt>) -> Item {
            Item::FuncDef(FuncDef {
                id: self.ids.next_id(),
                signature: FuncSig {
                    return_type: TypeSpec::scalar(BaseType::Int, ANYWHERE),
                    name: self.name(name),
                    params,
                    span: ANYWHERE,
                },
                body: Block {
                    stmts,
                    span: ANYWHERE,
                },
                span: ANYWHERE,
            })
        }
    }

    /// `int twice(int n) { return n * 2; }`, hand-built.
    ///
    /// The whole point of this fixture is that it exists without the parser: the AST is the
    /// contract between three passes, so it has to be constructible by anything that holds to it,
    /// not only by the one pass that happens to build it today.
    fn twice() -> Program {
        let mut build = Builder::new();
        let param = build.param(TypeSpec::scalar(BaseType::Int, ANYWHERE), "n");

        let n = build.ident("n");
        let two = build.int(2);
        let product = build.expr(ExprKind::Binary {
            op: BinOp::Multiply,
            left: Box::new(n),
            right: Box::new(two),
        });
        let ret = build.stmt(StmtKind::Return(Some(product)));

        Program {
            items: vec![build.func("twice", vec![param], vec![ret])],
        }
    }

    /// Ids are handed out in order and never repeat.
    #[test]
    fn ids_are_handed_out_in_order() {
        let mut ids = NodeIds::new();

        assert_eq!(ids.next_id(), NodeId(0));
        assert_eq!(ids.next_id(), NodeId(1));
        assert_eq!(ids.next_id().index(), 2);
    }

    /// Walking a tree finds every node once. Phase 3 keys its annotations by these ids, so a
    /// collision would quietly give two nodes the same type.
    #[test]
    fn node_ids_are_unique_across_a_program() {
        let nodes = nodes(&twice());
        let distinct: HashSet<NodeId> = nodes.iter().map(|(id, _)| *id).collect();

        // The function, its parameter, the `return`, the product, and the product's two operands.
        // The body block is not among them: it has no identity of its own.
        assert_eq!(nodes.len(), 6, "walk found {nodes:?}");
        assert_eq!(distinct.len(), nodes.len(), "ids repeat: {nodes:?}");
    }

    /// The AST carries no interior mutability, which is what ADR 0004 forbids.
    ///
    /// `Cell` and `RefCell` are `!Sync`, so a node that quietly gained one to stash an analysis
    /// result would fail to compile here rather than be caught in review.
    #[test]
    fn the_ast_has_no_interior_mutability() {
        /// Fails to compile if `T` holds anything mutable behind a shared reference.
        fn require_sync<T: Sync>() {}

        require_sync::<Program>();
        require_sync::<Stmt>();
        require_sync::<Expr>();
    }

    /// The same tree dumps to the same bytes every time, which is what a snapshot depends on.
    #[test]
    fn the_dump_is_deterministic() {
        let program = twice();

        assert_eq!(dump(&program, Spans::Hidden), dump(&program, Spans::Hidden));
        assert_eq!(dump(&program, Spans::Shown), dump(&program, Spans::Shown));
    }

    /// A hand-built tree round-trips through the dumper, indented one level per depth.
    #[test]
    fn a_hand_built_tree_dumps() {
        assert_eq!(
            dump(&twice(), Spans::Hidden),
            concat!(
                "(program\n",
                "  (func-def int twice\n",
                "    (params\n",
                "      (param int n))\n",
                "    (block\n",
                "      (return\n",
                "        (binary *\n",
                "          (ident n)\n",
                "          (int-lit 2))))))\n",
            )
        );
    }

    /// A dump is a file's worth of output, so it ends with a newline.
    #[test]
    fn the_dump_ends_with_a_newline() {
        assert!(dump(&twice(), Spans::Hidden).ends_with(")\n"));
    }

    /// Showing spans annotates each node that has one, and leaves the grouping lines bare.
    #[test]
    fn shown_spans_annotate_only_the_nodes_that_have_them() {
        let mut build = Builder::new();
        let one = build.int(1);
        let program = Program {
            items: vec![Item::GlobalVar(build.var(
                TypeSpec::scalar(BaseType::Int, ANYWHERE),
                "n",
                Some(Initializer::Expr(one)),
            ))],
        };

        assert_eq!(
            dump(&program, Spans::Shown),
            concat!(
                "(program\n",
                "  (global-var int n@0..1\n",
                "    (init\n",
                "      (int-lit 1@0..1))))\n",
            )
        );
    }

    /// One expression dumps on its own, without a program built around it to hold it.
    #[test]
    fn an_expression_dumps_on_its_own() {
        let mut build = Builder::new();
        let expr = build.int(7);

        assert_eq!(dump_expression(&expr, Spans::Hidden), "(int-lit 7)\n");
    }

    /// Every statement and expression form has a line in the dump, and no two forms share one.
    ///
    /// The exhaustive matches in the dumper are the real guard — a variant added without a line
    /// fails to compile — and this checks the lines are also told apart from one another.
    #[test]
    fn every_form_dumps_to_a_distinct_line() {
        let mut build = Builder::new();
        let condition = build.ident("c");
        let body = build.stmt(StmtKind::Empty);
        let init = build.var(TypeSpec::scalar(BaseType::Int, ANYWHERE), "i", None);
        let operand = build.ident("x");

        let statements = vec![
            StmtKind::Break,
            StmtKind::Continue,
            StmtKind::Empty,
            StmtKind::Return(None),
        ];
        let expressions = vec![
            ExprKind::IntLit(1),
            ExprKind::CharLit(b'a'),
            ExprKind::StrLit(b"hi".to_vec()),
            ExprKind::Ident("x".to_string()),
            ExprKind::Unary {
                op: UnOp::Negate,
                operand: Box::new(operand.clone()),
            },
            ExprKind::PostfixIncDec {
                op: IncDec::Increment,
                operand: Box::new(operand.clone()),
            },
            ExprKind::Binary {
                op: BinOp::Add,
                left: Box::new(operand.clone()),
                right: Box::new(operand.clone()),
            },
            ExprKind::Assign {
                target: Box::new(operand.clone()),
                value: Box::new(operand.clone()),
            },
            ExprKind::Index {
                base: Box::new(operand.clone()),
                index: Box::new(operand.clone()),
            },
            ExprKind::Call {
                callee: Box::new(operand.clone()),
                args: Vec::new(),
            },
        ];

        let mut heads: Vec<String> = Vec::new();
        for kind in statements {
            let stmt = build.stmt(kind);
            heads.push(first_line(&stmt_tree(&stmt)));
        }
        for kind in expressions {
            let expr = build.expr(kind);
            heads.push(first_line(&expr_tree(&expr)));
        }
        for kind in [
            StmtKind::Block(Block {
                stmts: Vec::new(),
                span: ANYWHERE,
            }),
            StmtKind::If {
                condition: condition.clone(),
                then_branch: Box::new(body.clone()),
                else_branch: None,
            },
            StmtKind::While {
                condition: condition.clone(),
                body: Box::new(body.clone()),
            },
            StmtKind::For {
                init: None,
                condition: None,
                step: None,
                body: Box::new(body.clone()),
            },
            StmtKind::LocalVar(init),
        ] {
            let stmt = build.stmt(kind);
            heads.push(first_line(&stmt_tree(&stmt)));
        }

        let distinct: HashSet<&String> = heads.iter().collect();
        assert_eq!(
            distinct.len(),
            heads.len(),
            "a form shares a line: {heads:?}"
        );
    }

    /// The head of a dumped node, without its children.
    fn first_line(node: &DumpNode) -> String {
        node.head.clone()
    }

    /// A literal dumps in the form it was written, escapes and all, through the same table the
    /// lexer decoded it with.
    #[test]
    fn literals_dump_as_they_were_written() {
        let mut build = Builder::new();

        for (kind, expected) in [
            (ExprKind::IntLit(-5), "(int-lit -5)\n"),
            (ExprKind::CharLit(b'a'), "(char-lit 'a')\n"),
            (ExprKind::CharLit(b'\n'), "(char-lit '\\n')\n"),
            (ExprKind::StrLit(b"a\tb".to_vec()), "(str-lit \"a\\tb\")\n"),
            (ExprKind::StrLit(Vec::new()), "(str-lit \"\")\n"),
        ] {
            let expr = build.expr(kind);

            assert_eq!(dump_expression(&expr, Spans::Hidden), expected);
        }
    }

    /// An item reports the identity and extent of whichever form it is.
    #[test]
    fn an_item_reports_its_own_identity() {
        let mut build = Builder::new();
        let signature = FuncSig {
            return_type: TypeSpec::scalar(BaseType::Int, ANYWHERE),
            name: build.name("f"),
            params: Vec::new(),
            span: ANYWHERE,
        };
        let items = [
            build.func("m", Vec::new(), Vec::new()),
            Item::FuncDecl(FuncDecl {
                id: build.ids.next_id(),
                signature,
                span: ANYWHERE,
            }),
            Item::GlobalVar(build.var(TypeSpec::scalar(BaseType::Int, ANYWHERE), "g", None)),
        ];

        let ids: HashSet<NodeId> = items.iter().map(Item::id).collect();

        assert_eq!(ids.len(), 3);
        assert!(items.iter().all(|item| item.span() == ANYWHERE));
    }

    /// A type spells itself the way it was declared.
    #[test]
    fn types_spell_themselves() {
        assert_eq!(TypeSpec::scalar(BaseType::Int, ANYWHERE).to_string(), "int");
        assert_eq!(
            TypeSpec::scalar(BaseType::Char, ANYWHERE).to_string(),
            "char"
        );
        assert_eq!(
            TypeSpec::scalar(BaseType::Void, ANYWHERE).to_string(),
            "void"
        );
        assert_eq!(
            TypeSpec::array(BaseType::Int, 3, ANYWHERE).to_string(),
            "int[3]"
        );
        assert_eq!(
            TypeSpec::unsized_array(BaseType::Char, ANYWHERE).to_string(),
            "char[]"
        );
    }

    /// The three type shapes are distinguishable, and only an array claims to be one.
    #[test]
    fn only_arrays_are_arrays() {
        assert!(!TypeSpec::scalar(BaseType::Int, ANYWHERE).is_array());
        assert!(TypeSpec::array(BaseType::Int, 3, ANYWHERE).is_array());
        assert!(TypeSpec::unsized_array(BaseType::Int, ANYWHERE).is_array());
    }

    /// Every operator spells itself as source text, and no two share a spelling within a set.
    #[test]
    fn operators_spell_themselves_distinctly() {
        for spellings in [
            BinOp::ALL
                .iter()
                .map(|op| op.spelling())
                .collect::<Vec<_>>(),
            UnOp::ALL.iter().map(|op| op.spelling()).collect(),
            IncDec::ALL.iter().map(|op| op.spelling()).collect(),
            BaseType::ALL.iter().map(|ty| ty.spelling()).collect(),
        ] {
            let distinct: HashSet<_> = spellings.iter().collect();

            assert!(spellings.iter().all(|spelling| !spelling.is_empty()));
            assert_eq!(
                distinct.len(),
                spellings.len(),
                "duplicate in {spellings:?}"
            );
        }
    }

    /// The `ALL` lists are complete: a variant added without being listed fails to compile here.
    #[test]
    fn every_operator_variant_is_listed() {
        for op in BinOp::ALL {
            match op {
                BinOp::Add
                | BinOp::Subtract
                | BinOp::Multiply
                | BinOp::Divide
                | BinOp::Remainder
                | BinOp::Equal
                | BinOp::NotEqual
                | BinOp::Less
                | BinOp::Greater
                | BinOp::LessEqual
                | BinOp::GreaterEqual
                | BinOp::And
                | BinOp::Or => {}
            }
        }
        for op in UnOp::ALL {
            match op {
                UnOp::Negate | UnOp::Not | UnOp::Plus | UnOp::PreIncrement | UnOp::PreDecrement => {
                }
            }
        }
        for op in IncDec::ALL {
            match op {
                IncDec::Increment | IncDec::Decrement => {}
            }
        }
        for base in BaseType::ALL {
            match base {
                BaseType::Int | BaseType::Char | BaseType::Void => {}
            }
        }
    }
}
