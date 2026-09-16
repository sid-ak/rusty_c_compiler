# Corpus coverage

Every program in this directory is valid subset C. Each one is compiled twice — once by `rustycc`
and once by `clang -O0 -std=c99 -Wall` — and both binaries are run and compared, so a program here
is not a test with an expected answer written beside it, it is a question put to two compilers.
`cargo test --test differential` is that comparison, one test per program.

This matrix is the record of which language features the corpus actually reaches. It is checked: a
`.c` file in this directory with no row below fails `cargo test`, because a program nobody wrote a
row for is a program nobody knows the purpose of.

Each program also carries the exit code and stdout it should produce, in an `// expect-exit:` and
`// expect-stdout:` header. Those values were recorded from `clang -O0 -std=c99`, not from
`rustycc`, so they are an independent answer rather than a note of what this compiler happened to do.
`cargo test --test codegen_exec` holds every program to them, one test per program.

A few programs also carry a `// clang-warns:` header, naming the warnings `clang` has about them
under `-Wall -Wextra`. Those programs are testing a corner of the grammar that warning exists to
point at — `a || b && c` without parentheses, an `else` with two `if`s to choose from — so the
warning is declared rather than silenced, and a test fails if the declared set and the reported set
ever stop matching.

Everything here stays inside behavior C defines. No division by zero, no signed overflow, no
out-of-bounds subscript, no read of an uninitialized object, and no expression that modifies and
reads the same object without a sequence point between them. An undefined program has no right
answer, so two compilers disagreeing about it would prove nothing and two agreeing would prove less.

## Programs

| Program | What it is for |
| --- | --- |
| [`add_sub.c`](add_sub.c) | Addition and subtraction, how a chain of them groups, and unary sign |
| [`arithmetic.c`](arithmetic.c) | Every operator, how they group, and the increment forms |
| [`array_as_grid.c`](array_as_grid.c) | A two-dimensional grid in a flat array, by row, column, and diagonal |
| [`array_basics.c`](array_basics.c) | Declaring, writing, and reading an array, by literal and computed index |
| [`array_initializers.c`](array_initializers.c) | Full, short, empty, and absent initializer lists, local and global |
| [`array_loops.c`](array_loops.c) | Forwards, backwards, strided, and two-ended walks over one array |
| [`array_min_max.c`](array_min_max.c) | A loop carrying state that updates only on some passes |
| [`array_parameters.c`](array_parameters.c) | Arrays across the function boundary, mutated in place and forwarded on |
| [`array_recursion.c`](array_recursion.c) | An array walked and filled by recursion rather than by a loop |
| [`array_reverse.c`](array_reverse.c) | Reversing in place: two indices, a swap, and a loop that stops halfway |
| [`array_search.c`](array_search.c) | Linear search: found, absent, duplicated, and at either end |
| [`arrays.c`](arrays.c) | Declaring, indexing, initializing, and passing arrays |
| [`associativity.c`](associativity.c) | Left-associative operators, and right-associative assignment |
| [`binary_search.c`](binary_search.c) | The same search as a loop and as a recursion, held to each other |
| [`bubble_sort.c`](bubble_sort.c) | Nested loops with a data-dependent swap and an early exit |
| [`call_basics.c`](call_basics.c) | Arguments in, a value back, and a call used where it was written |
| [`calls_in_conditions.c`](calls_in_conditions.c) | Calls in an `if`, a loop test, every `for` clause, and a short-circuit |
| [`chained_assignment.c`](chained_assignment.c) | Assignment as an expression, chained and inside a condition |
| [`char_arithmetic.c`](char_arithmetic.c) | `char` promoted to `int`, and truncated back on the way in |
| [`char_comparison.c`](char_comparison.c) | Character ranges, and the signedness of a byte above 127 |
| [`char_conversion.c`](char_conversion.c) | Case changes and digit values, both directions, round-tripped |
| [`comparisons.c`](comparisons.c) | All six comparisons, each answered both ways, as a 0 or a 1 |
| [`continue_runs_the_step.c`](continue_runs_the_step.c) | `continue` reaching the `for` step clause rather than the condition |
| [`control_flow.c`](control_flow.c) | Branches, loops, every `for` clause combination, `break` and `continue` |
| [`dangling_else.c`](dangling_else.c) | An `else` binding to the nearer `if`, against the braced alternative |
| [`deep_recursion.c`](deep_recursion.c) | A thousand frames laid out and torn down, each holding locals |
| [`early_return.c`](early_return.c) | Returning out of a branch, a loop, and a loop inside a loop |
| [`empty_statements.c`](empty_statements.c) | The statement forms that do nothing, including as a loop body |
| [`exit_status.c`](exit_status.c) | A `main` return above 255, observable only as its low eight bits |
| [`exit_status_zero.c`](exit_status_zero.c) | Falling off the end of `main`, which returns zero |
| [`expression_temporaries.c`](expression_temporaries.c) | Trees deep enough that intermediates must not share a slot |
| [`for_clauses.c`](for_clauses.c) | All eight present/absent combinations of the three `for` clauses |
| [`forward_declarations.c`](forward_declarations.c) | Declaring before defining, and calling from above the definition |
| [`functions.c`](functions.c) | Forward declarations, recursion, `void`, and parameter counts across the ABI boundary |
| [`gcd_and_power.c`](gcd_and_power.c) | Two numeric routines written twice, as a loop and as a recursion |
| [`global_arrays.c`](global_arrays.c) | Global arrays: full, partial, and absent initializers, mutated in place |
| [`global_scalars.c`](global_scalars.c) | Globals read and written across functions, and shadowed by a local |
| [`globals_and_recursion.c`](globals_and_recursion.c) | One piece of storage reached from every frame of a recursion |
| [`if_chains.c`](if_chains.c) | `if`, `if`/`else`, and else-if chains, every arm reached |
| [`increment_forms.c`](increment_forms.c) | The value each of the four increment forms produces, not only its effect |
| [`logical_ops.c`](logical_ops.c) | `&&`, `\|\|`, and `!`, and the 0-or-1 they yield |
| [`many_parameters.c`](many_parameters.c) | Seven, eight, nine, and twelve parameters across the register boundary |
| [`matrix_multiply.c`](matrix_multiply.c) | Three nested loops with an accumulator and two index strides |
| [`mul_div_mod.c`](mul_div_mod.c) | Multiplication, and the signs C gives a quotient and a remainder |
| [`mutual_recursion.c`](mutual_recursion.c) | Two functions calling each other, and a three-way cycle |
| [`nested_blocks.c`](nested_blocks.c) | Blocks inside blocks, and the shadowing that comes with them |
| [`nested_calls.c`](nested_calls.c) | Calls inside calls, in every argument position, past the register boundary |
| [`nested_loops.c`](nested_loops.c) | `break` and `continue` binding to the innermost loop containing them |
| [`precedence.c`](precedence.c) | Precedence across the whole operator table, with asymmetric operands |
| [`prime_sieve.c`](prime_sieve.c) | A sieve over a global array, checked against trial division |
| [`recursion.c`](recursion.c) | Mutual recursion, Ackermann, recursion filling an array, and all three together with string output |
| [`recursion_factorial.c`](recursion_factorial.c) | Single recursion, checked against the same function as a loop |
| [`recursion_fibonacci.c`](recursion_fibonacci.c) | Double recursion, where the call stack branches rather than lines up |
| [`short_circuit.c`](short_circuit.c) | Short-circuiting made visible by side effects rather than inferred |
| [`sorting.c`](sorting.c) | Reversing, sorting, and searching an array through helpers that mutate it in place |
| [`stack_machine.c`](stack_machine.c) | An array used as a stack, pushed and popped through functions |
| [`string_building.c`](string_building.c) | Copying, appending, filling, and writing the null terminator by hand |
| [`string_literals.c`](string_literals.c) | Literals printed, passed on, repeated, and every escape decoded |
| [`string_walking.c`](string_walking.c) | Walking to the null terminator, counting, finding, and reversing |
| [`strings.c`](strings.c) | String literals, `char` arrays, escapes, and the null terminator |
| [`unary_ops.c`](unary_ops.c) | The unary operators stacked, and what each one binds to |
| [`unbraced_bodies.c`](unbraced_bodies.c) | Single-statement bodies on every construct that takes one |
| [`void_functions.c`](void_functions.c) | `void` calls as statements, bare `return`, and falling off the end |
| [`while_loops.c`](while_loops.c) | A `while` that runs, one that does not, `break`, and `continue` |

## Features

Every feature in the grammar appears in at least three programs, so no feature rests on a single
file continuing to exist.

| Feature | Covered by |
| --- | --- |
| `int` declarations and initialization | `arithmetic.c`, `add_sub.c`, `while_loops.c`, `nested_blocks.c` |
| `char` declarations and initialization | `strings.c`, `char_arithmetic.c`, `char_comparison.c`, `char_conversion.c` |
| Global variables | `functions.c`, `global_scalars.c`, `global_arrays.c`, `prime_sieve.c`, `stack_machine.c` |
| Uninitialized globals reading as zero | `global_scalars.c`, `global_arrays.c`, `array_initializers.c` |
| Integer literals, decimal and at the edges of `int` | `arithmetic.c`, `precedence.c`, `mul_div_mod.c` |
| Character literals, plain and escaped | `strings.c`, `char_comparison.c`, `string_literals.c` |
| Arithmetic `+ -` | `add_sub.c`, `arithmetic.c`, `precedence.c`, `associativity.c` |
| Arithmetic `* / %` | `mul_div_mod.c`, `arithmetic.c`, `precedence.c`, `gcd_and_power.c` |
| Division and remainder with a negative operand | `mul_div_mod.c`, `arithmetic.c`, `expression_temporaries.c` |
| Unary `- + !` | `unary_ops.c`, `arithmetic.c`, `logical_ops.c`, `precedence.c` |
| Prefix and postfix `++ --` | `increment_forms.c`, `arithmetic.c`, `array_basics.c`, `empty_statements.c` |
| Relational operators | `comparisons.c`, `char_comparison.c`, `array_min_max.c`, `while_loops.c` |
| Equality operators | `comparisons.c`, `array_search.c`, `string_walking.c`, `precedence.c` |
| `&&` and `\|\|`, including short-circuiting | `short_circuit.c`, `logical_ops.c`, `char_comparison.c`, `calls_in_conditions.c` |
| Precedence across the operator table | `precedence.c`, `arithmetic.c`, `unary_ops.c`, `expression_temporaries.c` |
| Associativity, left and right | `associativity.c`, `add_sub.c`, `mul_div_mod.c`, `chained_assignment.c` |
| Assignment, plain and chained | `chained_assignment.c`, `associativity.c`, `arithmetic.c`, `array_basics.c` |
| Assignment through an array subscript | `array_basics.c`, `chained_assignment.c`, `string_building.c`, `stack_machine.c` |
| Parenthesized grouping | `precedence.c`, `expression_temporaries.c`, `unary_ops.c`, `add_sub.c` |
| `char`-to-`int` promotion | `char_arithmetic.c`, `char_comparison.c`, `char_conversion.c`, `strings.c` |
| `if` / `else` and else-if chains | `if_chains.c`, `control_flow.c`, `early_return.c`, `array_min_max.c` |
| Dangling `else` | `dangling_else.c`, `control_flow.c`, `empty_statements.c` |
| Unbraced single-statement bodies | `unbraced_bodies.c`, `dangling_else.c`, `control_flow.c`, `while_loops.c` |
| `while` | `while_loops.c`, `control_flow.c`, `binary_search.c`, `string_walking.c` |
| `for`, all eight clause combinations | `for_clauses.c`, `control_flow.c`, `calls_in_conditions.c` |
| `break` | `while_loops.c`, `nested_loops.c`, `for_clauses.c`, `control_flow.c` |
| `continue` | `continue_runs_the_step.c`, `nested_loops.c`, `while_loops.c`, `control_flow.c` |
| Nested blocks and shadowing | `nested_blocks.c`, `global_scalars.c`, `control_flow.c` |
| The empty statement and the empty block | `empty_statements.c`, `nested_blocks.c`, `unbraced_bodies.c` |
| An expression statement whose value is discarded | `empty_statements.c`, `void_functions.c`, `stack_machine.c` |
| Function definitions and calls | `call_basics.c`, `functions.c`, `nested_calls.c`, `gcd_and_power.c` |
| Forward declarations | `forward_declarations.c`, `mutual_recursion.c`, `functions.c`, `arrays.c` |
| Recursion, single | `recursion_factorial.c`, `deep_recursion.c`, `gcd_and_power.c`, `array_recursion.c` |
| Recursion, double | `recursion_fibonacci.c`, `recursion.c`, `globals_and_recursion.c` |
| Mutual recursion | `mutual_recursion.c`, `recursion.c`, `functions.c` |
| Recursion nested inside an argument list | `recursion.c`, `nested_calls.c`, `gcd_and_power.c` |
| Calls as arguments to calls | `nested_calls.c`, `gcd_and_power.c`, `binary_search.c`, `calls_in_conditions.c` |
| Calls in a condition | `calls_in_conditions.c`, `string_walking.c`, `stack_machine.c` |
| `void` return and `(void)` parameter lists | `void_functions.c`, `functions.c`, `array_parameters.c`, `stack_machine.c` |
| A bare `return` from a `void` function | `void_functions.c`, `early_return.c`, `array_recursion.c` |
| Early `return` from inside a loop | `early_return.c`, `array_search.c`, `bubble_sort.c`, `prime_sieve.c` |
| Eight and nine parameters | `many_parameters.c`, `functions.c`, `nested_calls.c` |
| Ten or more parameters | `many_parameters.c` |
| Array declaration and indexing | `array_basics.c`, `arrays.c`, `array_loops.c`, `prime_sieve.c` |
| Array initializer lists, full and partial | `array_initializers.c`, `arrays.c`, `global_arrays.c`, `matrix_multiply.c` |
| An empty initializer list | `array_initializers.c` |
| Array parameters `int a[]` | `array_parameters.c`, `arrays.c`, `bubble_sort.c`, `matrix_multiply.c` |
| Writing through an index | `array_basics.c`, `arrays.c`, `sorting.c`, `string_building.c` |
| An array mutated in place through a helper | `array_parameters.c`, `array_reverse.c`, `sorting.c`, `recursion.c` |
| An already-decayed parameter forwarded on | `array_parameters.c`, `array_recursion.c`, `sorting.c` |
| Swapping, reversing, and sorting an array | `array_reverse.c`, `bubble_sort.c`, `sorting.c`, `string_building.c` |
| Nested loops over one array | `bubble_sort.c`, `matrix_multiply.c`, `sorting.c`, `prime_sieve.c` |
| String literals as arguments | `string_literals.c`, `string_walking.c`, `strings.c`, `recursion.c` |
| `char` array initialized from a string literal | `strings.c`, `char_conversion.c`, `string_walking.c`, `global_arrays.c` |
| Escape sequences | `string_literals.c`, `strings.c`, `char_conversion.c` |
| The null terminator | `string_walking.c`, `string_building.c`, `strings.c`, `global_arrays.c` |
| Falling off the end of `main` | `exit_status_zero.c`, `strings.c` |
| A non-zero exit status | `exit_status.c`, `exit_status_zero.c`, `arithmetic.c` |
| Exit status masked to the low eight bits | `exit_status.c` |
| The runtime shim's three functions | every program |

## Feature interactions

A feature working on its own says nothing about it working next to another one. Every pair below is
one where the two have to agree about something — a width, a register, an order of evaluation — and
each has at least one program that puts them together.

| Interaction | Covered by |
| --- | --- |
| Recursion with arrays | `array_recursion.c`, `binary_search.c`, `recursion.c` |
| Recursion with globals | `globals_and_recursion.c`, `prime_sieve.c` |
| Recursion past the register boundary | `deep_recursion.c`, `recursion.c` |
| `char` with promotion and comparison | `char_comparison.c`, `char_conversion.c`, `char_arithmetic.c` |
| `char` across a function boundary | `char_conversion.c`, `many_parameters.c`, `strings.c` |
| Arrays across a function boundary with in-place mutation | `array_parameters.c`, `array_reverse.c`, `bubble_sort.c` |
| Arrays past the register boundary | `many_parameters.c`, `arrays.c` |
| Short-circuit with side effects | `short_circuit.c`, `calls_in_conditions.c` |
| Nested loops with `break` and `continue` | `nested_loops.c`, `control_flow.c` |
| Loops with an early `return` out of them | `early_return.c`, `array_search.c`, `bubble_sort.c` |
| Globals with array subscripting | `global_arrays.c`, `prime_sieve.c`, `stack_machine.c` |
| String literals with array subscripting | `string_walking.c`, `char_conversion.c`, `strings.c` |
| Calls with array arguments inside a condition | `calls_in_conditions.c`, `string_walking.c` |
| Calls nested inside a call's own argument list | `nested_calls.c`, `gcd_and_power.c` |
| Assignment nested inside a larger expression | `chained_assignment.c`, `expression_temporaries.c` |
| Shadowing with a global of the same name | `global_scalars.c`, `nested_blocks.c` |

## Not yet covered

Nothing in the subset is left out of this matrix. What is outside the subset — the preprocessor,
`struct`, `switch`, pointers as declared types, and the rest of the list in
[the language subset](../../docs/architecture.md#the-language-subset) — is not covered here because
it is rejected rather than compiled; `invalid/` beside this directory holds the programs that must
be turned down, with the rule each one violates.
