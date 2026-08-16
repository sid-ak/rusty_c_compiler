# Runtime Shim

The runtime is three fixed-arity functions, compiled once by `clang` and linked into every program
the project builds — by `rustycc` and by `clang` alike, so both sides of a comparison are exercising the
same runtime:

```c
void print_int(int n);
void print_char(char c);
void print_string(char *s);
```

`print_string` takes a pointer because that is the type a string literal actually decays to; passing
it to `print_char` or `print_int` would be a type error the analyzer catches. It is implemented as a
loop over `print_char` up to the null terminator. All three are built on `write(2)` directly, with no
`stdio` dependency, so there is no buffering layer whose flush timing could differ between a binary
built by `rustycc` and one built by `clang`. The reasoning for fixed-arity functions instead of `printf`
is [ADR 0006](decisions/0006-fixed-arity-runtime-shim.md).
