# Unit Test Report — Runtime Shim

## Unit

Source under test: `runtime/shim.c` (three C functions — `print_int`, `print_char`, `print_string`,
plus the internal `write_all` retry-loop helper — compiled by `clang`, not by `rustycc`, and linked
into every compiled program), `src/runtime.rs` (`SHIM_OBJECT`/`shim_object()`, the Rust-side handle
to the object the build script compiles), and `tests/runtime_shim.rs` (the integration suite that
compiles, links, and runs real programs against the shim). Per ADR 0006, this is deliberately the
one piece of the "compiled output" side of the system that exists before code generation does: it is
ordinary C, built once by `clang`, and the same object will be linked into both `rustycc`-compiled
and `clang`-compiled binaries during Phase 5's differential testing — so a bug in the shim itself
would corrupt *both* sides of every future differential comparison identically, which is exactly why
it is tested exhaustively and independently now, ahead of code generation existing at all.

## Date

2026-08-23

## Engineers

Sidharth Anandkumar (sole engineer)

## Test Methodology

**Approach: black-box behavioral testing via full compile-link-run cycles (never unit-testing the C
functions by reading their source or by linking them into a Rust test harness directly), organized
by boundary-value analysis at the specific numeric and control-flow edges the implementation's own
comments identify as risk points, plus a build-hygiene check independent of runtime behavior.**

The methodology here is dictated by what `tests/runtime_shim.rs`'s own module doc comment states
directly: *"reading the source cannot tell you what `print_int(INT_MIN)` writes"* — since these are
functions whose entire job is a side effect (bytes written to stdout via a raw `write(2)` loop), the
only methodology that actually proves anything is running compiled, linked machine code and
inspecting the real output, not reasoning about the C source. Every test therefore follows the same
shape: write a `main()` that calls the shim functions, compile it with `clang -std=c99 -O0 -Wall
-Wextra -Werror` linked against the identical `SHIM_OBJECT` the driver will use, run the resulting
binary, and assert on its literal captured stdout.

1. **Boundary-value analysis on `print_int`'s numeric range**, targeting exactly the values the
   implementation's own comments flag as hazardous: `print_int_covers_the_whole_int_range` tests
   `0` (the do-while-must-still-print-something boundary — a naive digit-extraction loop can print
   nothing for zero), `-1` and `1` (sign boundary), and critically `i32::MAX` (`2147483647`) and
   `i32::MIN` written as `-2147483647 - 1` (the exact idiom the source comment specifies for writing
   `INT_MIN` without triggering the same UB the implementation itself works around) — the comment in
   the test explains *why* `INT_MIN` is spelled that way: negating the literal `2147483648` is
   exactly the naive-implementation bug the shim's `0u - (unsigned int)n` unsigned-wraparound trick
   exists to avoid, so this test is specifically targeted at validating that workaround, not a
   generic large-number check. `print_int_prints_digits_in_order` is a second, independent
   correctness axis for the same function — digits could be individually correct while emitted in
   reversed order (a classic bug in "extract digits into a buffer from the end" implementations like
   this one) — tested with a multi-digit positive and negative value chosen specifically because a
   reversal would be visually obvious in the assertion (`1024` → `4201` would fail loudly).

2. **Exhaustive-enough black-box coverage of `print_char`**: control characters, letters, and
   digits (`a`, `\t`, `Z`, `\n`, `0`) in one sequence, chosen to cover the ASCII partitions that
   matter for a byte-for-byte `write` call — printable letters, a control character, and a digit
   character that could be confused with the *value* zero if the implementation mishandled the
   char/int boundary.

3. **Boundary-value analysis on `print_string`**: `print_string_writes_up_to_the_terminator` tests
   the empty string (`""`) — the boundary where the `while (*s != '\0')` loop must execute zero
   times, not underflow or read past the pointer — interleaved *between* non-empty strings
   (`""`, `"hello"`, `""`, `" line\nnext\n"`, `""`) specifically so that an off-by-one that
   accidentally consumed one byte past a terminator would corrupt a subsequent call's output and be
   caught, rather than being invisible if each case were tested in isolation.

4. **Composition/integration testing across all three functions together**:
   `output_is_unbuffered_and_in_call_order` interleaves all three functions and asserts the combined
   output matches the exact call order — this directly tests the "unbuffered, because nothing is
   buffered" design property (ADR 0006's stated reason for not using `printf`: buffering could let
   two binaries flush at different points and produce a spurious differential mismatch that has
   nothing to do with either compiler's correctness). This is the test that specifically validates
   the property the differential-testing strategy in Phase 5 will depend on — it is testing a
   property of the *design decision*, not just of the individual functions.

5. **Build-hygiene / static testing, independent of runtime behavior**:
   `the_shim_compiles_without_warnings` recompiles `runtime/shim.c` directly (not via the build
   script) under `-Wall -Wextra` and asserts stderr is exactly empty. This is a distinct test
   category from the behavioral tests above — a warning-free build is a static property of the
   source, checked once, decoupled from whatever flags the build script happens to use, so a future
   change to `build.rs` cannot silently stop enforcing it.

6. **Precondition testing at the Rust/C boundary** (`src/runtime.rs`): `the_shim_object_exists`
   asserts the compiled `.o` file the build script was supposed to produce is actually present on
   disk before any test that links against it runs — this is the load-bearing precondition for every
   other test in this unit (and, later, for the compiler driver itself), so it is checked directly
   rather than left to surface as a confusing linker error in an unrelated test.

**Coverage assessment.** All three public functions (`print_int`, `print_char`, `print_string`) are
exercised through real compiled-and-linked execution, not simulation. `print_int` is tested at both
range extremes (`INT_MIN`, `INT_MAX`), at the zero/sign boundaries, and for digit ordering.
`print_string` is tested at its empty-input boundary, interleaved with non-empty calls specifically
to catch off-by-one bugs a purely isolated empty-string test would miss. The functions' composition
(call-order/unbuffered-ness) is tested together, which is the property differential testing will
depend on later. The build itself is checked for warning-cleanliness independent of behavior. What
is intentionally out of scope for this unit: the shim's interaction with `rustycc`-generated code
specifically (as opposed to `clang`-generated test harness code) — that is Phase 4/5 territory, since
code generation does not exist yet; every test here links the shim against hand-written `clang`-
compiled C, which is a faithful stand-in for what the driver will do (per `src/runtime.rs`'s own doc
comment: "One object per build, linked by the driver and by both test harnesses, so every binary in
a differential comparison contains the identical runtime") but is not itself a test of the driver.

## Automated Test Code

7 tests total: 1 in `src/runtime.rs`, 6 in `tests/runtime_shim.rs`. The integration tests share a
`run_program(name, body)` helper that writes `body` as the contents of `main()`, compiles it with
`clang -std=c99 -O0 -Wall -Wextra -Werror` linked against `SHIM_OBJECT`, runs the resulting binary,
and returns its captured stdout as a `String`.

| # | Test | Input | Expected output |
|---|------|-------|------------------|
| 1 | `the_shim_object_exists` (`src/runtime.rs`) | `shim_object()` (path from build script) | `.is_file() == true` |
| 2 | `print_int_covers_the_whole_int_range` | `print_int(0)`, `(-1)`, `(1)`, `(2147483647)`, `(-2147483647 - 1)`, each followed by `\n` | Stdout: `"0\n-1\n1\n2147483647\n-2147483648\n"` |
| 3 | `print_int_prints_digits_in_order` | `print_int(1024); print_char(' '); print_int(-9070);` | Stdout: `"1024 -9070"` |
| 4 | `print_char_writes_one_byte` | `print_char('a')`, `'\t'`, `'Z'`, `'\n'`, `'0'` | Stdout: `"a\tZ\n0"` |
| 5 | `print_string_writes_up_to_the_terminator` | `print_string("")`, `"hello"`, `""`, `" line\nnext\n"`, `""` (interleaved empties) | Stdout: `"hello line\nnext\n"` |
| 6 | `output_is_unbuffered_and_in_call_order` | Interleaved `print_string`/`print_int`/`print_char` for `n=42` then `m=-42` | Stdout: `"n=42\nm=-42\n"` |
| 7 | `the_shim_compiles_without_warnings` | `clang -std=c99 -O0 -Wall -Wextra -c runtime/shim.c` | Exit success; stderr == `""` (no warnings) |

## Actual Outputs

Executed as part of `cargo test --lib` (test 1) and `cargo test --test runtime_shim` (tests 2–7);
full unedited capture in `reports/unit_tests/cargo_test_output.txt`:

```
test runtime::tests::the_shim_object_exists ... ok

     Running tests/runtime_shim.rs
test the_shim_compiles_without_warnings ... ok
test output_is_unbuffered_and_in_call_order ... ok
test print_char_writes_one_byte ... ok
test print_int_prints_digits_in_order ... ok
test print_int_covers_the_whole_int_range ... ok
test print_string_writes_up_to_the_terminator ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.12s
```

**Result: all 7 tests passed.** No failures. The 1.12s wall time for the 6 integration tests
reflects that each one genuinely invokes `clang` to compile and link a fresh binary and then
executes it — this is real compiled-code execution against the actual toolchain on Sidharth's
machine (see `reports/unit_tests/00-environment.md` for the exact `clang`/Xcode versions), not a
simulation. `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` reported no
violations against `src/runtime.rs` or `tests/runtime_shim.rs` (clippy does not lint `runtime/
shim.c`, since it is C, not Rust; its own warning-cleanliness is instead covered directly by test 7
above).
