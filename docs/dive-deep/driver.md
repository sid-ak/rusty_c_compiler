# Driver

The driver writes the generated `.s` to a temporary directory, invokes `clang -c` to assemble it,
then invokes `clang` again to link the resulting object together with the runtime shim's object, and
cleans up the intermediates. The reasoning for shelling out to `clang` rather than calling `as` and
`ld` directly is [ADR 0009](decisions/0009-clang-as-assembler-and-linker.md).

When a toolchain invocation fails, the child process's stderr is surfaced rather than a bare exit
code — a linker error the user cannot read is worse than no error at all. Temp files are removed on
both the success and the failure path unless `--keep-temps` is passed, and temp paths are unique per
invocation so two concurrent compilations cannot collide with each other.

Debug flags mirror the pipeline stages, so any stage's output can be inspected in isolation:

| Flag | Stops after |
| --- | --- |
| `--dump-tokens` | the lexer |
| `--dump-ast` | the parser |
| `--check` | semantic analysis (front end only, no code emitted) |
| `-S` | code generation (emits `.s`, does not assemble or link) |
