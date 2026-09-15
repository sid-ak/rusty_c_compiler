//! Unit tests for the AST: node identities, immutability, and the tree dump.

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

/// Every statement and expression form has a line of its own in the dump, carrying its own id
/// and span, and no two forms share one.
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
        heads.push(own_line(stmt_tree(&stmt), stmt.id, stmt.span));
    }
    for kind in expressions {
        let expr = build.expr(kind);
        heads.push(own_line(expr_tree(&expr), expr.id, expr.span));
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
        heads.push(own_line(stmt_tree(&stmt), stmt.id, stmt.span));
    }

    let distinct: HashSet<&String> = heads.iter().collect();
    assert_eq!(
        distinct.len(),
        heads.len(),
        "a form shares a line: {heads:?}"
    );
}

/// The head of a dumped node, after asserting the line stands for the node it was built from.
///
/// The dump walk is also how [`nodes`] enumerates a program, so a form whose line carries a
/// child's identity instead of its own would silently drop the node from that enumeration.
fn own_line(line: DumpNode, id: NodeId, span: Span) -> String {
    assert_eq!(
        (line.id, line.span),
        (Some(id), Some(span)),
        "{} does not carry its own identity",
        line.head
    );

    line.head
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
            UnOp::Negate | UnOp::Not | UnOp::Plus | UnOp::PreIncrement | UnOp::PreDecrement => {}
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
