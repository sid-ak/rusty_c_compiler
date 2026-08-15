# A Compiler for C, in Rust

## Overview

This project is a compiler, implemented in Rust, for a well-defined subset of the C programming
language. Since it is only for a subset of C and not all of C, it is hence a **_Rusty C Compiler._**

Source files are processed through a lexer, a recursive-descent parser, a semantic analysis pass,
and a code generator that emits ARM64 (AArch64) assembly, which is then assembled and linked with
the native macOS toolchain into a real executable.

The reference compiler, clang, will serve directly as a testing oracle: any subset-C program can be
compiled and run through both clang and this project's compiler, and the two executables' behavior
shall be compared. This gives the testing effort a precise, external notion of “correct” that a
custom language would not provide, and is the primary reason a C subset was chosen over a
purpose-built toy language.

Rust was chosen for the implementation because its enum-and-pattern-matching model suits Abstract
Syntax Tree (AST) representation well, and its compile-time safety guarantees keep the
implementation effort focused on compiler logic rather than memory-management bugs. Both the
compiler and the code it emits target Apple Silicon Macs (macOS/ARM64), matching the development
machine and avoiding any cross-compilation or emulation.

## Major Features

### Lexer

Tokenizes C source (identifiers, keywords, integer/character literals, operators, punctuation), with
line/column tracking for diagnostics.

> **NOTE:** The preprocessor is out of scope; input is single-file, macro-free C.

### Parser

A recursive-descent parser producing an abstract syntax tree (AST) for the supported subset,
enforcing C's operator precedence and associativity, and reporting syntax errors without panicking
on malformed input.

### Language Subset

- `int` and `char` types
- global and local variables
- one-dimensional arrays
- `if/else`, `while`, `for`, and `return`
- function declarations, calls, and recursion
- arithmetic, comparison, and logical operators
- Pointers for simple array indexing.

> **NOTE:** Pointers beyond simple array indexing, structs/unions, and the preprocessor are excluded
> to keep the grammar and semantics tractable for the timeframe.

### Semantic Analysis

A pass over the AST that resolves declarations and scopes, checks function signatures against call
sites, and rejects type-mismatched or undeclared-identifier programs before code generation.

### Code generation

Emits ARM64 (AArch64) assembly following Apple's calling convention (AAPCS64), then invokes the
system assembler and linker (via Xcode Command Line Tools) to produce a native macOS executable.

### Command-line driver

`mycc program.c -o program`, mirroring the usage of a real compiler, to keep test scripts and the
grading demo straightforward.

## Testing Plan

### Lexer Module

Unit tests for each token category and edge cases such as malformed literals, unterminated character
constants, and unusual whitespace/comment placement.

### Parser Module

Unit tests verifying operator precedence, associativity, and correct AST shape for representative
programs, plus tests confirming that malformed input yields a graceful parse error instead of a
panic.

### Semantic Analysis Module

Unit tests for scope resolution, redeclaration errors, undeclared-identifier detection, and
function-signature/call-site mismatches.

### Code Generation Module

Unit tests checking that each construct (arithmetic expression, loop, function call) generates
assembly that assembles cleanly and produces the expected value when run.

### Differential Testing Against clang

The same subset-C source is compiled with both this project's compiler and clang -O0; the resulting
binaries are run and their stdout and exit codes are diffed. This is the project's central
correctness methodology, since clang's behavior stands in for the C standard as ground truth.

### Fuzz Testing

`cargo-fuzz` will be run against the lexer and parser with raw byte input to surface panics or
crashes on malformed or adversarial source code.

---

Once the system is feature-complete, a single final system test will validate end-to-end
functionality. This will consist of a curated suite of representative C-subset programs covering
every supported language feature (arithmetic, control flow, recursion, arrays, function calls). Each
program will be compiled and run through both this project's compiler and clang -O0, with stdout and
exit codes diffed between the two, for every program in the suite. This differential result is the
system-level acceptance criterion: the project is considered functionally complete only when every
program in the suite produces identical, correct behavior under both compilers.

## Platform and Tooling

The compiler itself is implemented in Rust and builds via Cargo. Its code-generation target is ARM64
macOS (Apple Silicon, AAPCS64 ABI), since supporting multiple target architectures is out of scope
for this timeframe; this also matches the development machine directly, requiring no
cross-compilation, emulation, or containerization. The project will be developed and demonstrated on
macOS using the Xcode Command Line Tools' assembler and linker, with clang (already present on the
system) available as the testing oracle. Testing relies on Rust's built-in test framework (cargo
test) and cargo-fuzz for fuzz testing.

## Identified Gaps and Design Resolutions

Few gaps needed explicit resolution before implementation. Each is addressed below.

### Observable output without a preprocessor

Excluding the preprocessor does not require excluding the standard library, since C permits
declaring external functions directly in source (e.g., extern int printf(const char _, ...);) with
no #include. To avoid the added complexity of variadic-argument handling under AAPCS64, the compiler
will instead support a small fixed-arity runtime shim: print_int(int), print_char(char), and
print_string(char_). This will be compiled once by clang into an object file and linked into every
test binary. print_string takes a pointer, matching the type a string literal actually decays to
(rather than the type mismatch that would result from passing it to print_char or print_int), and is
implemented internally as a loop over print_char until a null terminator is reached. Both this
project's compiler and clang will link the same shim when building the same test program, keeping
the differential-testing comparison apples-to-apples.

### Arrays at function boundaries

C relies on array-to-pointer decay to pass arrays into functions and pointers, beyond simple
applications, are out of scope for this project. This ambiguity is resolved by supporting
array-to-pointer decay only at the function-parameter boundary (an array argument is passed as a
pointer to its first element, matching real C behavior) while general pointer arithmetic and pointer
variables elsewhere remain out of scope. This keeps arrays usable in realistic function-based
programs without reopening full pointer semantics.

### Code generation strategy

The strategy for how AST values would be mapped onto ARM64 registers requires clarification. Given
the project's timeframe, code generation will use a straightforward stack-spill strategy: every
local variable and intermediate value is assigned a fixed stack slot, values are loaded into
registers only for the duration of a single operation, and no cross-instruction register allocation
is attempted. This is the standard approach for a first compiler backend, and is called out
explicitly here so it is planned for rather than discovered mid-implementation.

### String literals

Needed as arguments to print_string but absent from the original feature list. String literals will
be lexed as a token type, and each distinct literal will be emitted by the code generator as
labeled, null-terminated bytes in a read-only data section (the Mach-O **TEXT,**cstring section),
referenced by address at its use site, that address is exactly the char\* value print_string
expects.
