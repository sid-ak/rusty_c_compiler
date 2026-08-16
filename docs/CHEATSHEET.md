# Cheatsheet

Commands for building the compiler, running its tests, and driving it by hand. Run everything from
the repo root.

Every expected output below was copied from a real run, not written from memory. The page is
refreshed at the end of each phase, so it covers what the compiler can actually do now rather than
what it will eventually do — the same discipline the README's status section follows.

## Setup

1. `source ~/.cargo/env`: only needed in a shell opened before rustup was installed; new shells get
   it from `~/.zshrc`.
2. `cargo build`: builds `mycc` into `target/debug/`. Also compiles `runtime/shim.c` with clang, so
   this is the first thing that fails if the Xcode Command Line Tools are missing.

## The gate

The four commands CI runs, in the order that fails fastest.

1. `cargo fmt --check`: prints a diff and exits non-zero on drift. `cargo fmt` fixes it in place.
2. `cargo clippy --all-targets -- -D warnings`: includes the no-unwrap/expect/panic/indexing denials
   for `src/`. Test code is exempt via `clippy.toml`.
3. `cargo test`: 89 tests across six binaries.
4. `uv run mkdocs build --strict`: fails on a broken link or a page missing from `nav`. The red
   MkDocs 2.0 block is an advisory banner from mkdocs-material, not an error — read the last line.

## Narrower test runs

| Command | Scope | Tests |
| --- | --- | --- |
| `cargo test --lib` | in-crate units; no C compiled, no processes spawned | 79 |
| `cargo test --test cli` | usage, exit codes, library callable in process | 3 |
| `cargo test --test lexer_snapshots` | full token stream for a representative program | 1 |
| `cargo test --test runtime_shim` | compiles, links, runs C against the real `shim.o` | 6 |
| `cargo test --lib maximal` | any substring filters by test name | 1 |
| `cargo test -- --nocapture` | show `println!` from passing tests | — |

## Manual: things that should work

### 1. Dump the tokens of a valid program

```bash
cat > /tmp/hello.c <<'EOF'
int main(void) {
    int total = 0xff;
    print_string("hi\n");
    return total % 7;
}
EOF
./target/debug/mycc /tmp/hello.c --dump-tokens
```

Exit 0, and:

```
1:1-1:4         Keyword(Int)
1:5-1:9         Ident("main")
1:9-1:10        LParen
1:10-1:14       Keyword(Void)
1:14-1:15       RParen
1:16-1:17       LBrace
2:5-2:8         Keyword(Int)
2:9-2:14        Ident("total")
2:15-2:16       Assign
2:17-2:21       IntLit(255)
2:21-2:22       Semi
3:5-3:17        Ident("print_string")
3:17-3:18       LParen
3:18-3:24       StrLit("hi\n")
3:24-3:25       RParen
3:25-3:26       Semi
4:5-4:11        Keyword(Return)
4:12-4:17       Ident("total")
4:18-4:19       Percent
4:20-4:21       IntLit(7)
4:21-4:22       Semi
5:1-5:2         RBrace
6:1-6:1         Eof
```

Three things to confirm by eye: `0xff` arrived as `IntLit(255)` so the base was decoded;
`StrLit("hi\n")` holds a real newline byte rather than a backslash and an `n`; every token carries
a start and end position.

### 2. Maximal munch

```bash
printf 'int f(void){int a=1;int b=2;return a+++b;}\n' > /tmp/munch.c
./target/debug/mycc /tmp/munch.c --dump-tokens | grep Plus
```

```
1:37-1:39       PlusPlus
1:39-1:40       Plus
```

`a+++b` must split as `a`, `++`, `+`, `b` — longest operator wins at each position. The wrong split
also produces three tokens, so this needs looking at rather than counting.

### 3. Tabs do not shift the caret

```bash
printf 'int main(void){\n\t\tint x = @;\n}\n' > /tmp/tabs.c
./target/debug/mycc /tmp/tabs.c --dump-tokens
```

```
/tmp/tabs.c:2:11: error: stray '@' in program
		int x = @;
		        ^
```

Column 11 counts bytes, matching what clang reports, so a tab is one column. The caret line
reproduces the two tabs instead of padding with spaces — widen and narrow the terminal and the
caret keeps tracking the `@`.

## Manual: things that should fail

### 4. Several errors in one file

```bash
cat > /tmp/errors.c <<'EOF'
int a = 0x;
char c = '';
int b = @;
int d = "open;
EOF
./target/debug/mycc /tmp/errors.c --dump-tokens
```

Exit 1, and:

```
/tmp/errors.c:1:9: error: expected digits after '0x'
int a = 0x;
        ^~
/tmp/errors.c:2:10: error: empty character literal
char c = '';
         ^~
/tmp/errors.c:3:9: error: stray '@' in program
int b = @;
        ^
/tmp/errors.c:4:9: error: unterminated string literal
int d = "open;
        ^~~~~~
mycc: 4 errors generated
```

The check that matters most: four mistakes, four diagnostics, source order, and the lexer kept going
after each one. Every caret spans the whole offending construct — `^~` covers all of `0x`, not just
the `0`.

### 5. Integer literal overflow

```bash
printf 'int a = 4294967296;\n' > /tmp/big.c
./target/debug/mycc /tmp/big.c --dump-tokens
```

```
/tmp/big.c:1:9: error: integer literal is too large for 'int': 4294967296
int a = 4294967296;
        ^~~~~~~~~~
note: the maximum is 2147483647; write INT_MIN as -2147483647 - 1
```

`2147483648` is deliberately rejected too, which is why `INT_MIN` has to be written the way
`limits.h` writes it.

### 6. An operator the subset omits

```bash
printf 'int a = 1 & 2;\n' > /tmp/amp.c
./target/debug/mycc /tmp/amp.c --dump-tokens
```

```
/tmp/amp.c:1:11: error: '&' is not supported in this C subset
int a = 1 & 2;
          ^
note: did you mean '&&'?
```

A lone `&` is real C, so it is reported as unsupported rather than as a stray byte.

### 7. Bad invocations

1. `./target/debug/mycc`: usage on stderr, exit 2.
2. `./target/debug/mycc nope.c`: `mycc: cannot read 'nope.c': No such file or directory (os error 2)`,
   exit 1.
3. `./target/debug/mycc /tmp/hello.c --dump-tokens --check`: clap reports the conflict, exit 2. The
   stage flags are one conflict group, so a run stops in exactly one place.

### 8. Arbitrary bytes

```bash
head -c 4096 /dev/urandom > /tmp/garbage.c
./target/debug/mycc /tmp/garbage.c --dump-tokens
```

Must produce diagnostics and terminate — never a panic, never a hang. This is the invariant phase 5
will fuzz against.

## The runtime shim by hand

```bash
cat > /tmp/shim_main.c <<'EOF'
void print_int(int n);
void print_char(char c);
void print_string(char *s);

int main(void) {
    print_string("int min = ");
    print_int(-2147483647 - 1);
    print_char('\n');
    return 0;
}
EOF
clang /tmp/shim_main.c "$(ls -t target/debug/build/*/out/shim.o | head -1)" -o /tmp/shim_main
/tmp/shim_main
```

```
int min = -2147483648
```

The build script writes `shim.o` under Cargo's output directory; stale copies accumulate across
rebuilds, hence `ls -t | head -1`. `cargo clean && cargo build` leaves exactly one.

`INT_MIN` is the interesting case: negating it as an `int` overflows, so digits are extracted in
unsigned arithmetic. A naive implementation prints something wrong here rather than crashing.

## Snapshots

1. `cargo install cargo-insta`: not installed yet. Without it a changed snapshot still fails the
   test and writes a `.snap.new` beside the old one to diff by hand.
2. `cargo insta review`: step through diffs interactively. Read the whole snapshot before accepting
   — the token stream is 218 lines and a wrong span mid-file looks like a right one at a glance.
3. `ls tests/snapshots/`: one file, for the representative program. The diagnostic format is pinned
   by an inline snapshot inside `src/diagnostics.rs` instead, next to the code that produces it.

## Docs and branch review

1. `uv run mkdocs serve`: site at `127.0.0.1:8000` with live reload. Run
   `uv venv && uv pip install -r requirements-docs.txt` first if the environment is fresh.
2. `git log --oneline main..HEAD`: the eight commits, one per task in the epic's checklist order.
3. `git show --stat 4fa2205`: any single task on its own.
4. `git diff main..HEAD -- src/`: just the compiler, without fixtures, workflow, and docs.

## Reference

### Exit codes

| Code | Meaning |
| --- | --- |
| 0 | the requested stage completed |
| 1 | the program was rejected, or the input could not be read; diagnostics already on stderr |
| 2 | the command line itself was wrong — missing file argument, or conflicting stage flags |

### Flags

| Flag | Stops after | Status |
| --- | --- | --- |
| `--dump-tokens` | the lexer | prints the token stream |
| `--dump-ast` | the parser | accepted, prints nothing — parser is phase 2 |
| `--check` | semantic analysis | accepted, prints nothing — analyzer is phase 3 |
| `-S` | code generation | accepted, writes nothing — backend is phase 4 |
| `-o <FILE>` | — | parsed and carried; nothing links yet |
| `--keep-temps` | — | parsed; the driver that makes temp files is phase 4 |

`mycc program.c -o program` parses its arguments and exits 0 without producing an executable. The
shape of the CLI is fixed; later phases fill it in.
