# Unit Test Report — Parser: Expressions

## Unit

Source under test: `src/parser/expr.rs` — precedence-climbing expression parsing
(`Parser::expression`, `assignment`, `binary`, `unary`, `postfix`, `primary`, `call_args`), the
operator tables (`binary_operator`, `prefix_operator`, `postfix_operator`), the
two-token-lookahead detection of compound-assignment/shift operators this subset omits
(`adjacent_operator`), and the syntactic-lvalue check (`is_assignable`). This unit depends on the
Lexer (Token Model + Scanner) units for its input tokens and on the shared `Parser`/`Bail` machinery
defined in `src/parser/mod.rs` (reported separately as the Statements/Declarations/Recovery unit),
but the expression-grammar logic itself — precedence, associativity, the postfix chain, and
lvalue-shape checking — is fully contained in this one file.

## Date

2026-08-23

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

**Approach: white-box unit testing with structural (AST-shape) assertions as the primary technique,
applied through equivalence partitioning over the operator/precedence table, deliberately chosen
over any value-evaluating assertion for the reason stated directly in this file's own test
comments.**

This unit implements precedence climbing, a compact algorithm whose entire correctness rests on one
table (`binary_operator`'s precedence numbers) and one recursive structure. The single most
important methodological decision, and the one explicitly called out in the code
(`precedence_and_associativity_match_c`'s doc comment: *"asserted on the tree rather than on a
result: `1+2*3 == 7` would also hold if precedence were wrong in a way that cancelled out"*), is
that **every** test in this unit asserts on the shape of the parsed AST — via `ast::dump_expression`
— never on an evaluated numeric result (there is no evaluator yet; even if there were, this is the
correct methodology regardless). This matters because precedence bugs frequently compensate for each
other numerically (e.g., swapping the relative precedence of `+` and `-` produces wrong trees that
can still evaluate correctly on symmetric test inputs) — only inspecting the tree's shape rules that
class of false-negative out.

1. **Equivalence partitioning over the full precedence table**, exercised two ways:
   `every_binary_operator_parses` iterates every entry of `BinOp::ALL` (13 operators, all 6
   precedence tiers) individually, proving each operator is reachable and self-identifies correctly
   in isolation; `precedence_and_associativity_match_c` then tests 16 hand-picked *combinations*
   across adjacent and non-adjacent precedence tiers (`1+2*3`, `1*2+3`, `a<b==c<d`, `a||b&&c`,
   `a%b*c`, mixed with unary/postfix/index/call forms like `-f(x)[i]`), which is where an actual
   precedence-table bug would surface — two operators individually parsing correctly does not imply
   they combine correctly, so pairwise combination testing is necessary in addition to the
   per-operator sweep.

2. **Associativity testing, partitioned by direction.** Left-associativity of the binary tiers is
   tested via same-operator chains (`1-2-3`, `1/2/3` in the combination test above — chosen because
   subtraction and division are non-commutative, so a left/right-associativity bug is directly
   visible in the tree shape, unlike with `+`/`*`). Right-associativity is tested separately and
   explicitly for the two constructs that are right-associative by grammar rule rather than by the
   climbing loop: assignment (`a=b=c` in the combination test) and prefix operators
   (`prefix_operators_are_right_associative`: `- -x`, `!!a`, `++--a`).

3. **Boundary-value analysis at the whitespace/adjacency boundary.** Because this subset lexes `+=`
   as two separate tokens (`+` `=`) rather than as one, the parser must distinguish "these two
   tokens are one omitted compound operator" from "these are two real operators that happen to be
   adjacent." `separated_operators_are_not_mistaken_for_a_pair` is the boundary test: `a < -b` and
   `a > + b` must **not** be misread as `<-`/`>+`-like compound operators, verified by requiring a
   space in the source specifically so the tokens' spans are non-adjacent — this directly tests the
   `first.span.end != second.span.start` boundary condition inside `adjacent_operator`.
   `operators_spelled_as_two_tokens_are_named` is the positive case: all 7 omitted two-token
   operators (`+= -= *= /= %= << >>`) are asserted to produce a specific, helpful diagnostic rather
   than a generic "unexpected token."

4. **Negative-space / equivalence-partition testing for the syntactic-lvalue rule.**
   `only_syntactic_lvalues_may_be_assigned_to` partitions assignment targets into "shape the grammar
   allows" (identifier, index expression — including through parentheses, since `(a) = 1` must
   still work) and "shape the grammar rejects outright, before any type is known" (integer literal,
   a call result, a parenthesized binary expression, a postfix-incremented value) — both partitions
   tested in one function, which is important because a bug that accidentally widened or narrowed
   `is_assignable`'s `matches!` would only be caught by having representatives from *both* sides of
   that boundary, not just the accepting side.

5. **Robustness/error-message testing**: `a_missing_operand_names_what_was_wanted` covers the case
   where an operand is entirely missing (`1 +`, `a[i][` — asserts the exact "expected an expression,
   found end of file" message) versus present-but-wrong (`*x` — this subset has no dereference
   operator, so `*` cannot start an expression; asserts "expected an expression, found '*'"). This
   distinguishes the EOF-boundary error path from the wrong-token error path, which are different
   code paths in `primary`'s fallthrough (`Err(self.expected("an expression"))`).

6. **Structural regression tests for compound postfix/primary forms**: `postfix_operators_chain`
   proves the postfix loop (index, call, `++`/`--`) composes in arbitrary combination
   (`a[i]++`, `f(x)[0]`, `a[i][j]`) from one shared loop with no per-combination special case — the
   methodological point being that testing each postfix operator alone would not catch a bug in how
   the loop transitions between operator kinds. `call_arguments_parse_at_assignment_precedence`
   specifically tests that an assignment inside a call argument (`f(a, b = c)`) is parsed as one
   argument rather than mis-parsed around the comma, since this subset lexes no comma operator.
   `parentheses_leave_no_trace` (equivalence class: parenthesization is transparent to tree shape)
   asserts `((((1))))` produces the *identical* dump string as `1`, and that `(a) = 1` is assignable
   exactly as `a = 1` is — both direct tests of the "parentheses group and then vanish" design
   decision documented in the AST unit.

**Coverage assessment.** All 13 `BinOp` variants, all 5 `UnOp` variants, and both `IncDec` variants
are exercised individually (via the `ALL` sweep) and in combination (via the precedence/associativity
table). All primary forms (int/char/string literal, identifier, parenthesized expression) are
covered. Both branches of `is_assignable` are covered on both sides of the boundary. All 7 adjacent
two-token operators this subset omits are covered, plus their negative-space companion (real
adjacent-but-separated operators). The two distinct "expression parse failed" error paths (missing
operand at EOF vs. an unexpected token) are both covered. What is out of scope for this unit and
deliberately left to the Statements/Declarations/Recovery unit: how expression parsing interacts
with statement-level recovery after a syntax error (e.g. resynchronization), since that is
`parser/mod.rs`'s responsibility, not `expr.rs`'s.

## Automated Test Code

All 12 tests live in `src/parser/expr.rs` under `#[cfg(test)] mod tests`, using two shared helpers:
`shape(source)` (lex + parse one expression, assert it lexed/parsed cleanly and consumed the whole
input, return its one-line AST dump) and `errors(source)` (lex + parse, return the sorted diagnostic
messages).

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 1 | `primaries_parse_to_themselves` | `42`, `0xff`, `'a'`, `"hi"`, `total` | `(int-lit 42)`, `(int-lit 255)`, `(char-lit 'a')`, `(str-lit "hi")`, `(ident total)` |
| 2 | `precedence_and_associativity_match_c` | 16 cases incl. `1+2*3`, `1*2+3`, `a=b=c`, `-x*y`, `a||b&&c`, `a<b==c<d`, `-f(x)[i]`, `1++ +2` | Exact tree-shape string per case (see source; e.g. `1+2*3` → `(binary + (int-lit 1) (binary * (int-lit 2) (int-lit 3)))`) |
| 3 | `prefix_operators_are_right_associative` | `- -x`, `!!a`, `++--a` | `(unary - (unary - (ident x)))`, `(unary ! (unary ! (ident a)))`, `(unary ++ (unary -- (ident a)))` |
| 4 | `postfix_operators_chain` | `a[i]++`, `f(x)[0]`, `a[i][j]`, `a--` | Nested `postfix`/`index`/`call` trees per case |
| 5 | `call_arguments_parse_at_assignment_precedence` | `f()`, `f(a, b = c)`, `f(a + b, c)` | `(call (ident f) (args))`; args include an `assign` node for the 2nd; args include a `binary +` for the 1st |
| 6 | `parentheses_leave_no_trace` | `((((1))))` vs `1`; `(a+b)*c`; `(a) = 1` | First pair: identical dump string. `(a+b)*c` → `(binary * (binary + a b) c)`. `(a) = 1` → `(assign (ident a) (int-lit 1))` |
| 7 | `only_syntactic_lvalues_may_be_assigned_to` | Rejects: `1 = 2`, `f() = 1`, `(a + b) = 1`, `a++ = 1`. Accepts: `a = 1`, `a[i] = 1`, `(a) = 1`, `(a[i]) = 1` | Rejects: `["expression is not assignable"]` each. Accepts: parse without error |
| 8 | `operators_spelled_as_two_tokens_are_named` | `a += b`, `-=`, `*=`, `/=`, `%=`, `a << b`, `a >> b` | `["unsupported in this C subset: '+='"]` etc., one per operator |
| 9 | `separated_operators_are_not_mistaken_for_a_pair` | `a < -b`, `a > + b` | `(binary < a (unary - b))`; `(binary > a (unary + b))` (not misread as `<-`/`>+`) |
| 10 | `a_missing_operand_names_what_was_wanted` | `1 +`, `a[i][`, `*x` | `["expected an expression, found end of file"]` (×2), `["expected an expression, found '*'"]` |
| 11 | `every_binary_operator_parses` | Each of `BinOp::ALL` as `a <op> b` | `(binary <op> (ident a) (ident b))` for all 13 |
| 12 | `every_unary_operator_parses` | Each of `UnOp::ALL` as `<op>a`, each of `IncDec::ALL` as `a<op>` | `(unary <op> (ident a))` ×5; `(postfix <op> (ident a))` ×2 |

## Actual Outputs

Executed as part of `cargo test --lib` (full unedited capture in
`reports/unit_tests/cargo_test_output.txt`):

```
test parser::expr::tests::a_missing_operand_names_what_was_wanted ... ok
test parser::expr::tests::call_arguments_parse_at_assignment_precedence ... ok
test parser::expr::tests::only_syntactic_lvalues_may_be_assigned_to ... ok
test parser::expr::tests::parentheses_leave_no_trace ... ok
test parser::expr::tests::every_unary_operator_parses ... ok
test parser::expr::tests::postfix_operators_chain ... ok
test parser::expr::tests::operators_spelled_as_two_tokens_are_named ... ok
test parser::expr::tests::prefix_operators_are_right_associative ... ok
test parser::expr::tests::separated_operators_are_not_mistaken_for_a_pair ... ok
test parser::expr::tests::primaries_parse_to_themselves ... ok
test parser::expr::tests::every_binary_operator_parses ... ok
test parser::expr::tests::precedence_and_associativity_match_c ... ok

test result: ok. (12 of these ran as part of the 142 in `unittests src/lib.rs`)
```

**Result: all 12 tests passed.** No failures. `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check` reported no violations against this file.
