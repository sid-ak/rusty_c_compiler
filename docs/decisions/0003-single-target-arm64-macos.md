# ADR 0003 — One target: ARM64 macOS, with no portability layer

- Status: Accepted
- Date: 2026-08-10

## Context

A code generator has to commit to an instruction set, a calling convention, and an object file
format. A compiler that intends to support several targets abstracts over all three, usually by
introducing a target-independent intermediate representation and a per-target backend behind an
interface.

That abstraction has to be designed before the first backend exists, which means designing it
against a single concrete backend and guessing what the second one will need. The guess is usually
wrong, and the cost is paid immediately in indirection that makes the one backend that does exist
harder to read.

## Decision

Code generation targets ARM64 macOS only: AArch64 instructions, the AAPCS64 calling convention, and
Mach-O sections and symbol naming. There is no target abstraction, no target-independent
intermediate representation, and no cross-compilation, emulation, or containerization. The compiler
runs on the same machine and architecture it emits code for.

## Consequences

The backend walks the annotated AST and writes assembly text directly. It is short, it is readable
end to end, and target-specific facts — the leading underscore on Mach-O symbols, the 16-byte stack
alignment at call sites, `__TEXT,__cstring` — appear plainly where they are used rather than behind
an interface designed to hide them.

Testing gets simpler in a way that matters: the test suite compiles a program and runs it, natively,
in-process with the rest of `cargo test`. Nothing needs an emulator, a remote runner, or a
cross-linker, and `clang` on the same machine is the oracle for the same architecture.

The trade is that this compiler cannot produce code for anything else, and the test suite cannot run
anywhere else. Both are accepted: this is a learning project on a specific machine, not a product.

The parts most likely to be target-specific are concentrated in `src/codegen/` rather than spread
through the pipeline, so a second target later means introducing the abstraction against two real
backends instead of one real and one imagined. See
[future scope](../architecture.md#future-scope).

## Alternatives considered

Target x86-64 as well. Rejected: doubles the backend work and the ABI reading for no new concepts,
since the second target teaches instruction selection over again rather than anything new.

Emit LLVM IR and let LLVM do code generation. This is what a serious compiler would do, and it would
give optimization and every target for free. Rejected precisely because it removes the part of the
project that is the point — register and stack layout, calling conventions, instruction selection.

Emit C and compile that with `clang`. Even more of the same objection: it makes the "compiler" a
translator and eliminates code generation entirely.

Target ARM64 Linux in a container for portability. Rejected: adds a container toolchain and a
runtime indirection to the test loop while removing the ability to run and debug the produced binary
natively.
