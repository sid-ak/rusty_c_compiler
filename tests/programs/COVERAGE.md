# Corpus coverage

Every program in this directory is valid subset C. Each one is compiled by `rustycc` and, from
Phase 5 on, by `clang -O0` as well, with both binaries run and compared — so a program here is not
a test with an expected answer written beside it, it is a question put to two compilers.

This matrix is the record of which language features the corpus actually reaches. It is checked:
a `.c` file in this directory with no row below fails `cargo test`, because a program nobody wrote
a row for is a program nobody knows the purpose of.

## Programs

| Program | What it is for |
| --- | --- |
| [`arithmetic.c`](arithmetic.c) | Every operator, how they group, and the increment forms |
| [`control_flow.c`](control_flow.c) | Branches, loops, every `for` clause combination, `break` and `continue` |
| [`functions.c`](functions.c) | Forward declarations, recursion, `void`, and parameter counts across the ABI boundary |
| [`arrays.c`](arrays.c) | Declaring, indexing, initializing, and passing arrays |
| [`strings.c`](strings.c) | String literals, `char` arrays, escapes, and the null terminator |

## Features

| Feature | Covered by |
| --- | --- |
| `int` and `char` declarations | `arithmetic.c`, `strings.c` |
| Global variables | `functions.c`, `arrays.c` |
| Arithmetic `+ - * / %` | `arithmetic.c` |
| Unary `- + !` | `arithmetic.c` |
| Prefix and postfix `++ --` | `arithmetic.c`, `arrays.c` |
| Relational and equality operators | `arithmetic.c`, `control_flow.c` |
| `&&` and `\|\|`, including short-circuiting | `arithmetic.c` |
| Precedence and associativity | `arithmetic.c` |
| Assignment, including chained | `arithmetic.c` |
| `char`-to-`int` promotion | `arithmetic.c`, `strings.c` |
| `if` / `else` and else-if chains | `control_flow.c` |
| Unbraced single-statement bodies | `control_flow.c` |
| Dangling `else` | `control_flow.c` |
| `while` | `control_flow.c` |
| `for`, all eight clause combinations | `control_flow.c` |
| `break` and `continue`, including nested loops | `control_flow.c` |
| Nested blocks and the empty statement | `control_flow.c` |
| Function definitions and calls | `functions.c` |
| Forward declarations | `functions.c`, `arrays.c`, `strings.c` |
| Recursion, single and double | `functions.c` |
| `void` return and `(void)` parameter lists | `functions.c` |
| Eight and nine parameters | `functions.c` |
| Array declaration and indexing | `arrays.c` |
| Array initializer lists, full and partial | `arrays.c` |
| Array parameters `int a[]` | `arrays.c`, `strings.c` |
| Writing through an index | `arrays.c` |
| String literals as arguments | `strings.c` |
| `char` array initialized from a string literal | `strings.c` |
| Escape sequences | `strings.c` |
| The null terminator | `strings.c` |
| The runtime shim's three functions | all five |

## Not yet covered

Nothing in the subset is deliberately left out of this matrix. Phase 5 grows the corpus to the
combinations no one thought to write by hand ([issue #35](https://github.com/sid-ak/rusty_c_compiler/issues/35)),
and adds `invalid/` beside this directory for the programs that must be rejected rather than run.
