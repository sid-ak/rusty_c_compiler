# Invalid-program coverage

Every program in this directory is rejected by `rustycc --check`. Each one breaks exactly one rule,
and its header says which rule, what the message should be, and whether `clang -O0 -std=c99` rejects
it too.

This corpus guards the direction nothing else does. Differential testing compares programs both
compilers build; a check that quietly stops working makes this compiler accept something it should
reject, and no comparison of two working binaries would ever notice. These files are what notices.

The last column is the one to read closely. A `no` means real C that `clang` builds and this
compiler turns down on purpose — a restriction, not a bug. Each is decided by an ADR named in the
file's own header, and the full list is in
[`docs/architecture.md`](../../../docs/architecture.md#where-this-subset-is-stricter-than-c). A test
fails if those two lists disagree, and another fails if a file here has no row below.

## Programs

| Program | Rule it breaks | Expected message | `clang` rejects it too |
| --- | --- | --- | --- |
| [`array_as_condition.c`](array_as_condition.c) | an array used as a condition | `value of type 'int[2]' is not a condition` | **no** — deviation |
| [`array_in_arithmetic.c`](array_in_arithmetic.c) | an array used in arithmetic | `invalid operands to binary '+': 'int[2]' and 'int'` | yes |
| [`array_of_void.c`](array_of_void.c) | an array of void | `array has incomplete element type 'void'` | yes |
| [`assign_incompatible_type.c`](assign_incompatible_type.c) | assigning a value of an incompatible type | `cannot assign 'int[2]' to 'int'` | yes |
| [`assign_to_array.c`](assign_to_array.c) | assigning to an array name | `array name is not assignable` | yes |
| [`bare_return_in_non_void.c`](bare_return_in_non_void.c) | bare return in a non-void function | `'return' with no value in a function returning 'int'` | yes |
| [`break_outside_loop.c`](break_outside_loop.c) | break outside a loop | `'break' outside of a loop` | yes |
| [`call_argument_type.c`](call_argument_type.c) | call argument type mismatch | `argument 1 of 'sum' has type 'int', but 'int *' was expected` | yes |
| [`call_arity.c`](call_arity.c) | call arity mismatch | `'add' takes 2 arguments, but 1 was passed` | yes |
| [`calling_a_non_function.c`](calling_a_non_function.c) | calling a non-function | `called object is not a function` | yes |
| [`conflicting_signature.c`](conflicting_signature.c) | conflicting redeclaration of a function signature | `conflicting declaration of 'helper'` | yes |
| [`continue_outside_loop.c`](continue_outside_loop.c) | continue outside a loop | `'continue' outside of a loop` | yes |
| [`definition_disagrees.c`](definition_disagrees.c) | a definition disagreeing with an earlier declaration | `conflicting declaration of 'helper'` | yes |
| [`falls_off_the_end.c`](falls_off_the_end.c) | control reaching the end of a non-void function | `control reaches the end of non-void function 'one'` | **no** — deviation |
| [`function_as_value.c`](function_as_value.c) | using a function name as a value | `'helper' is a function; it can only be called` | yes |
| [`index_non_array.c`](index_non_array.c) | indexing a non-array and non-pointer | `subscripted value is not an array or a pointer` | yes |
| [`initialize_incompatible_type.c`](initialize_incompatible_type.c) | initializing a variable with an incompatible type | `cannot initialize 'int' with 'int[2]'` | yes |
| [`initializer_element_incompatible_type.c`](initializer_element_incompatible_type.c) | an array initializer element of an incompatible type | `cannot initialize 'int' with 'int[2]'` | yes |
| [`initializer_too_long.c`](initializer_too_long.c) | an array initializer longer than the array | `3 initializers for an array of 2` | **no** — deviation |
| [`non_constant_global.c`](non_constant_global.c) | a non-constant global initializer | `global initializer is not a constant` | yes |
| [`non_integer_subscript.c`](non_integer_subscript.c) | non-integer subscript | `array subscript is not an integer` | yes |
| [`redeclaration_in_scope.c`](redeclaration_in_scope.c) | redeclaration in the same scope | `redeclaration of 'x' in this scope` | yes |
| [`redefinition.c`](redefinition.c) | multiple definitions of one function | `redefinition of 'helper'` | yes |
| [`return_incompatible_type.c`](return_incompatible_type.c) | returning a value of an incompatible type | `cannot return 'int[2]' from a function returning 'int'` | yes |
| [`return_value_in_void.c`](return_value_in_void.c) | return with a value in a void function | `'return' with a value in a function returning 'void'` | yes |
| [`undeclared_function.c`](undeclared_function.c) | undeclared function | `undeclared identifier 'helper'` | yes |
| [`undeclared_identifier.c`](undeclared_identifier.c) | undeclared identifier | `undeclared identifier 'x'` | yes |
| [`use_before_declaration.c`](use_before_declaration.c) | use before declaration in the same scope | `undeclared identifier 'x'` | yes |
| [`void_parameter.c`](void_parameter.c) | void as a parameter type | `parameter has incomplete type 'void'` | yes |
| [`void_variable.c`](void_variable.c) | void as a variable type | `variable has incomplete type 'void'` | yes |
| [`zero_length_array.c`](zero_length_array.c) | a zero-length array | `array size must be greater than zero` | **no** — deviation |
