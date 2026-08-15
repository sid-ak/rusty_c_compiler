# Code Generation

The strategy is stack spilling — every local variable and every intermediate value gets a fixed slot
in the function's stack frame, and values are loaded into registers only for the duration of a
single operation before being written straight back. The reasoning for choosing this over register
allocation is [ADR 0005](decisions/0005-stack-spilling-instead-of-register-allocation.md); what
follows here is the shape it produces in the emitted assembly.

## The six-step shape of a binary operation

The operand convention is that the instruction always sees the left operand in `w0` and the right in
`w1`, in that order. Every binary operation — arithmetic, comparison, whatever the actual
instruction turns out to be — lowers to the same six steps:

1. Evaluate the left operand; its result ends up in `w0`.
2. Spill it: `str w0, [x29, #-T]` saves that result into this node's own temp slot, freeing `w0` for
   the right operand.
3. Evaluate the right operand; its result also ends up in `w0`.
4. Move it aside: `mov w1, w0` puts the right operand's result in `w1`, out of the way.
5. Reload the left operand: `ldr w0, [x29, #-T]` brings it back from the temp slot into `w0`.
6. Apply the instruction — for example `sub w0, w0, w1` — which now reads the left operand from `w0`
   and the right operand from `w1`, in source order.

```
        <evaluate left into w0>     // 1: left operand
        str  w0, [x29, #-T]         // 2: spill left into this node's temp slot
        <evaluate right into w0>    // 3: right operand
        mov  w1, w0                 // 4: right into w1
        ldr  w0, [x29, #-T]         // 5: left back into w0
        sub  w0, w0, w1             // 6: every instruction reads left, right
```

The `mov` earns its place. Without it, the reload would naturally land the left operand in `w1` and
leave the right in `w0`, so every non-commutative instruction would have to cross its operands —
`sub w0, w1, w0` — and every comparison would have to invert its condition code, since `cmp w1, w0`
with `cset w0, lt` means "left less than right" while `cmp w0, w1` with the same `cset` means the
opposite. Both forms are correct if written carefully, but the crossed form is a standing invitation
to transpose `sub`, `sdiv`, `msub`, or a condition code — and the failure is silent, since it
assembles cleanly and is invisible on symmetric operands like `2 - 2` or `a < a`. Spending one
instruction on the `mov` buys the property that every emitted instruction reads in source order.

## The prologue and the frame

Before a function can run its own logic it needs a safe workspace that will not overwrite data
belonging to whoever called it. The prologue is `stp x29, x30, [sp, #-N]!` followed by
`mov x29, sp`, with the frame size rounded up to a multiple of 16 as the ARM64 ABI (AAPCS64)
requires. Every local, spilled parameter, and expression temporary gets a fixed `x29`-relative
offset; the temporary region is sized from the function's maximum expression depth, so nested
expressions cannot collide with each other's slots. Parameters arrive in registers and are
immediately spilled to their slots in the prologue, so the function body treats parameters and
locals identically. Offsets that fall outside the immediate range of `ldr`/`str` are materialized
into a scratch register rather than silently truncated. Each function has exactly one epilogue, and
every `return` branches to it.

## Specific lowerings worth naming

Each of these is an edge case or a hardware quirk that needs care:

- `int` moves with `ldr`/`str`; `char` moves with `ldrsb`/`strb`, which sign-extends on load. This
  is what makes C's `char`-promotes-to-`int` rule fall out for free, with no extra instruction
  needed.
- Comparisons are `cmp w0, w1` followed by `cset` with the condition code that reads the same way as
  the source operator, yielding `0` or `1` — a direct consequence of the `w0`/`w1` convention above.
- `&&` and `||` lower to real branches, so short-circuiting genuinely skips evaluation of the right
  operand rather than evaluating both sides and combining them. A test in the corpus proves this by
  giving the right operand an observable side effect that must not happen when the left side already
  decides the result.
- Remainder has no direct ARM64 instruction. It is `sdiv w2, w0, w1` followed by
  `msub w0, w2, w1, w0`. `msub wd, wn, wm, wa` computes `wa - wn*wm`, so the accumulator operand
  (`wa`) is the one holding the left-hand side of the original `%` — a second operand ordering to
  get right, layered on top of the `w0`/`w1` convention, inside this one lowering.
- Array indexing computes a base address plus the index scaled by element size. The base address
  comes from three different places depending on where the array lives: a frame address for a local,
  an `adrp`+`add` pair for a global, or a loaded pointer for a decayed parameter.
- String literals are emitted as labeled null-terminated bytes in `__TEXT,__cstring` and referenced
  by address with `adrp`+`add`. That address is exactly the `char *` the
  [runtime shim](#the-runtime-shim) expects.

## Calls follow AAPCS64

The first eight integer or pointer arguments go in `x0`–`x7`, the rest on the stack with the
required alignment; the return value comes back in `w0`/`x0`; the stack pointer is 16-byte aligned
at every call site. Argument expressions are fully evaluated into temp slots before any argument
register is loaded, which is what keeps a nested call like `f(g(1), h(2))` from clobbering an
argument that has already been placed. Nine-argument functions appear in the test corpus
specifically because nine is the first arity that crosses the register-to-stack boundary.
