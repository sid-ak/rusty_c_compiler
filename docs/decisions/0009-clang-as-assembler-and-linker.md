# ADR 0009 — Drive the toolchain through clang, not as and ld

- Status: Accepted
- Date: 2026-08-10

## Context

The code generator emits assembly text. Turning that into a runnable executable takes two more
steps — assembling to a Mach-O object, then linking that object with the C runtime startup files and
the system libraries — and the compiler's driver has to invoke something to do them.

Both `as` and `ld` ship with the Xcode Command Line Tools and can be invoked directly. Doing so
means the driver must know which SDK is installed and where, where `crt1.o` and friends live, which
system library paths to pass, and which minimum-version and platform flags `ld` currently expects.
That set changes between Xcode releases and between macOS versions.

## Decision

The driver shells out to `clang` for both steps: `clang -c` to assemble the `.s` into an object, and
`clang` again to link that object together with the runtime shim's object into an executable.

## Consequences

The driver stays short and stays working across toolchain updates. `clang` resolves the SDK path,
the startup files, and the library search paths itself, and it is the same `clang` already required
as the differential oracle, so the project gains no new dependency.

The two sides of the differential comparison are linked by the same program with the same defaults,
which removes link-time configuration as a possible source of a behavioral difference.

The compiler is not self-contained: it cannot produce a binary without the Xcode Command Line Tools
present. This is already true for the test suite, which needs `clang` as the oracle regardless, so
it costs nothing beyond a preflight check and a clear error pointing at `xcode-select --install`.

Toolchain failures arrive as a child process's exit code and stderr rather than as structured
errors, so the driver surfaces that stderr verbatim. A linker error the user cannot read is worse
than no error at all.

Because the boundary is a subprocess invocation, replacing it later — with direct `as` and `ld`
calls, or with a real Mach-O object writer — is a change confined to `src/driver.rs`.

## Alternatives considered

Invoke `as` and `ld` directly. More explicit, and it would make the toolchain dependency narrower.
Rejected: reproducing SDK and startup-file discovery by hand is fragile busywork that teaches
nothing about compilers and breaks on Xcode updates.

Emit a Mach-O object file directly, skipping assembly text. This is what a production compiler does,
and it would remove the assembler dependency. Rejected for this scope, and rejected for a second
reason that matters more: readable assembly text is what makes per-construct snapshot tests possible
and makes a failing program debuggable by eye.

Link statically with no libc. Would remove the linker's SDK knowledge requirement. Rejected: the
runtime shim uses `write(2)`, and the differential oracle links normally, so both sides need the
same environment.
