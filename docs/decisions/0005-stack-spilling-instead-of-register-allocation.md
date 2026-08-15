# ADR 0005 — Stack spilling instead of register allocation

- Status: Accepted
- Date: 2026-08-10

## Context

The code generator has to decide where values live. ARM64 has 31 general-purpose registers, and a
real compiler assigns values to them by computing live ranges and solving a graph-coloring or
linear-scan allocation problem, spilling to the stack only when registers run out.

The proposal identified this as a gap that needed resolving before implementation rather than being
discovered mid-build, because the choice determines the shape of the entire backend.

Register allocation is a genuinely hard problem, and it is orthogonal to everything else this
project is about. It is also unusually unforgiving: a subtly wrong live range produces a value that
is correct in most programs and wrong in one, and localizing that failure means reading generated
assembly for a program that has no obvious distinguishing feature.

## Decision

Every local variable and every intermediate value is assigned a fixed slot in its function's stack
frame. Values are loaded into registers only for the duration of a single operation and written
straight back. No value is kept in a register across instructions, and no allocation decision is
made.

A binary operation therefore evaluates its left side into `w0`, spills it to that node's temp slot,
evaluates its right side into `w0`, moves that into `w1`, reloads the left side into `w0`, and
applies the instruction. The temporary region of the frame is sized from the maximum expression depth
in the function, so nested expressions cannot collide.

The `mov` is deliberate. Reloading the left operand straight into `w1` would save an instruction and
leave the operands reversed at the point of use, so every non-commutative instruction would need
crossed operands and every comparison an inverted condition code. Spending one instruction to keep
the left operand in `w0` and the right in `w1` means every emitted instruction reads in source order.
The exact forms are in [architecture.md](../architecture.md#code-generation).

## Consequences

The backend is correct by construction with respect to clobbering: no value can be clobbered because
no value stays in a register long enough to be clobbered. Nested calls, nested expressions, and
argument evaluation all become uniform, and the class of bug that is hardest to find in a first
backend does not arise.

Lowering becomes a direct structural walk. Each expression form has one lowering with no context
dependence, which is what makes per-construct assembly snapshot tests readable and stable.

The generated code is slow — every intermediate makes a round trip to memory — and visibly so in the
assembly. This is accepted. Correct and slow is the right first backend, and the differential suite
is indifferent to speed.

Frames are larger than they need to be, which is why the plan includes a specific test for a
function whose frame exceeds the immediate-offset range of `ldr`/`str`: with this strategy, that
path is reached by ordinary programs rather than by pathological ones.

The existing differential corpus becomes the safety net for replacing this later. Swapping in real
allocation is a self-contained change with an unambiguous success metric — the suite stays green
while the code gets faster. See [future scope](../architecture.md#future-scope).

## Alternatives considered

Linear-scan register allocation. The usual first step beyond spilling, and not unreasonable.
Rejected for this stage: it needs live-range computation, which needs a control-flow graph, which
needs an intermediate representation the project does not otherwise have.

Graph-coloring allocation. What a production compiler does. Rejected outright as disproportionate.

A hybrid — keep locals in registers, spill only temporaries. Tempting, and it would remove most of
the memory traffic. Rejected because it reintroduces exactly the clobbering question this decision
exists to avoid, without the analysis machinery that would answer it correctly.
