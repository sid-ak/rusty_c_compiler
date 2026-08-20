# Cheatsheet

Commands for building the compiler, running its tests, and driving it by hand. Run everything from
the repo root.

Every expected output below was copied from a real run, not written from memory. The page is
refreshed at the end of each phase, so it covers what the compiler can actually do now rather than
what it will eventually do — the same discipline the README's status section follows.

## Setup

1. `source ~/.cargo/env`: only needed in a shell opened before rustup was installed; new shells get
   it from `~/.zshrc`.
2. `cargo build`: builds `rustycc` into `target/debug/`. Also compiles `runtime/shim.c` with clang, so
   this is the first thing that fails if the Xcode Command Line Tools are missing.

## The gate

The four commands CI runs, in the order that fails fastest.

1. `cargo fmt --check`: prints a diff and exits non-zero on drift. `cargo fmt` fixes it in place.
2. `cargo clippy --all-targets -- -D warnings`: includes the no-unwrap/expect/panic/indexing denials
   for `src/`. Test code is exempt via `clippy.toml`, but only inside `#[test]` bodies — a helper in
   a `tests/*.rs` file needs the file-level `#![allow(clippy::expect_used)]` those files carry.
3. `cargo test`: 170 tests across seven binaries.
4. `uv run mkdocs build --strict`: fails on a broken link or a page missing from `nav`. The red
   MkDocs 2.0 block is an advisory banner from mkdocs-material, not an error — read the last line.

## Narrower test runs

| Command | Scope | Tests |
| --- | --- | --- |
| `cargo test --lib` | in-crate units; no C compiled, no processes spawned | 142 |
| `cargo test --test cli` | usage, exit codes, library callable in process | 3 |
| `cargo test --test lexer_snapshots` | full token stream for a representative program | 1 |
| `cargo test --test parser_snapshots` | AST for every corpus program, and the coverage matrix | 10 |
| `cargo test --test parser_no_panic` | every corpus program cut short at every byte, plus `tests/adversarial/` | 8 |
| `cargo test --test runtime_shim` | compiles, links, runs C against the real `shim.o` | 6 |
| `cargo test --lib precedence` | any substring filters by test name | 1 |
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
./target/debug/rustycc /tmp/hello.c --dump-tokens
```

Exit 0, and:

```
1:1-1:4    Keyword(Int)
1:5-1:9    Ident("main")
1:9-1:10   LParen
1:10-1:14  Keyword(Void)
1:14-1:15  RParen
1:16-1:17  LBrace
2:5-2:8    Keyword(Int)
2:9-2:14   Ident("total")
2:15-2:16  Assign
2:17-2:21  IntLit(255)
2:21-2:22  Semi
3:5-3:17   Ident("print_string")
3:17-3:18  LParen
3:18-3:24  StrLit("hi\n")
3:24-3:25  RParen
3:25-3:26  Semi
4:5-4:11   Keyword(Return)
4:12-4:17  Ident("total")
4:18-4:19  Percent
4:20-4:21  IntLit(7)
4:21-4:22  Semi
5:1-5:2    RBrace
6:1-6:1    Eof
```

Three things to confirm by eye: `0xff` arrived as `IntLit(255)` so the base was decoded;
`StrLit("hi\n")` holds a real newline byte rather than a backslash and an `n`; every token carries
a start and end position.

### 2. Maximal munch

```bash
printf 'int f(void){int a=1;int b=2;return a+++b;}\n' > /tmp/munch.c
./target/debug/rustycc /tmp/munch.c --dump-tokens | grep Plus
```

```
1:37-1:39  PlusPlus
1:39-1:40  Plus
```

`a+++b` must split as `a`, `++`, `+`, `b` — longest operator wins at each position. The wrong split
also produces three tokens, so this needs looking at rather than counting.

### 3. Dump the syntax tree

```bash
printf 'int add(int a, int b) {\n    return a + b;\n}\n' > /tmp/add.c
./target/debug/rustycc /tmp/add.c --dump-ast
```

Exit 0, and:

```
(program
  (func-def int add
    (params
      (param int a)
      (param int b))
    (block
      (return
        (binary +
          (ident a)
          (ident b))))))
```

One node per line, indented by depth. The dump is a function of the tree and nothing else, so two
runs over the same file give the same bytes — which is what makes it usable as a snapshot.

### 4. Precedence, read off the tree

```bash
printf 'int main(void) { return 1 + 2 * 3 - 4 / 2; }\n' > /tmp/prec.c
./target/debug/rustycc /tmp/prec.c --dump-ast
```

```
(program
  (func-def int main
    (params)
    (block
      (return
        (binary -
          (binary +
            (int-lit 1)
            (binary *
              (int-lit 2)
              (int-lit 3)))
          (binary /
            (int-lit 4)
            (int-lit 2)))))))
```

The thing to check by eye is that `*` and `/` sit *below* `+` and `-`, and that the top operator is
the `-`, because subtraction is left-associative and so takes everything before it as its left
operand. Running the program and checking it prints `5` would pass just as happily with precedence
wrong in a way that cancelled out, which is why the tree is what gets asserted.

### 5. Tabs do not shift the caret

```bash
printf 'int main(void){\n\t\tint x = @;\n}\n' > /tmp/tabs.c
./target/debug/rustycc /tmp/tabs.c --dump-tokens
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

### 6. Several lexical errors in one file

```bash
cat > /tmp/errors.c <<'EOF'
int a = 0x;
char c = '';
int b = @;
int d = "open;
EOF
./target/debug/rustycc /tmp/errors.c --dump-tokens
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
rustycc: 4 errors generated
```

The check that matters most: four mistakes, four diagnostics, source order, and the lexer kept going
after each one. Every caret spans the whole offending construct — `^~` covers all of `0x`, not just
the `0`.

### 7. Integer literal overflow

```bash
printf 'int a = 4294967296;\n' > /tmp/big.c
./target/debug/rustycc /tmp/big.c --dump-tokens
```

```
/tmp/big.c:1:9: error: integer literal is too large for 'int': 4294967296
int a = 4294967296;
        ^~~~~~~~~~
note: the maximum is 2147483647; write INT_MIN as -2147483647 - 1
```

`2147483648` is deliberately rejected too, which is why `INT_MIN` has to be written the way
`limits.h` writes it.

### 8. An operator the subset omits

```bash
printf 'int a = 1 & 2;\n' > /tmp/amp.c
./target/debug/rustycc /tmp/amp.c --dump-tokens
```

```
/tmp/amp.c:1:11: error: unsupported in this C subset: '&'
int a = 1 & 2;
          ^
note: did you mean '&&'?
```

A lone `&` is real C, so it is reported as unsupported rather than as a stray byte. `#`, `?`, `:`,
`^`, and `~` are reported the same way and for the same reason — none is a token of this grammar, so
nothing about them ever reaches the parser and the lexer is the only place that can say anything.
A byte that is not C at all, like `@`, is still a stray character.

### 9. Several structural errors in one file

```bash
cat > /tmp/recover.c <<'EOF'
struct point { int x; };

int main(void) {
    int a = ;
    int *p;
    a += 1;
    return a
}
EOF
./target/debug/rustycc /tmp/recover.c --dump-ast
```

Exit 1, and:

```
/tmp/recover.c:1:1: error: unsupported in this C subset: 'struct'
struct point { int x; };
^~~~~~
/tmp/recover.c:4:13: error: expected an expression, found ';'
    int a = ;
            ^
/tmp/recover.c:5:9: error: unsupported in this C subset: pointer declarators
    int *p;
        ^
/tmp/recover.c:6:7: error: unsupported in this C subset: '+='
    a += 1;
      ^~
/tmp/recover.c:8:1: error: expected ';', found '}'
}
^
rustycc: 5 errors generated
```

Five mistakes, five diagnostics, none of them a knock-on effect of the one before. Three things to
confirm by eye. The `struct` definition costs one diagnostic, not two, because recovery skipped its
body and the `};` that closed it. `int *p;` is named as a pointer declarator rather than reported as
a missing identifier. And `+=` is named even though it is two tokens, because the parser noticed the
`+` and the `=` were written touching.

Note that these are all *parser* diagnostics. A file with a lexical mistake in it stops before the
parser runs — a token stream that came out of malformed text makes parse errors guesswork — so
adding a `?` to this file would replace the whole report with one about the conditional operator.

### 10. Nesting deeper than the parser will follow

```bash
python3 -c "print('int f(void) { return ' + '('*2000 + '1' + ')'*2000 + '; }')" > /tmp/deep.c
./target/debug/rustycc /tmp/deep.c --dump-ast | head -3
```

```
/tmp/deep.c:1:85: error: nesting is too deep: the parser descends at most 128 levels
```

Exit 1. Recursive descent recurses, so without this the input above is a stack overflow, and a crash
is the one thing the front end is not allowed to produce. The limit counts parser descents rather
than brackets — a parenthesized expression costs two — which is why the caret lands on the 64th `(`
rather than the 128th. The number is a stack budget: a level costs roughly 4 KB in an unoptimized
build, and a unit test parses input this deep on a deliberately undersized stack so the margin is
enforced rather than assumed.

### 11. Bad invocations

1. `./target/debug/rustycc`: usage on stderr, exit 2.
2. `./target/debug/rustycc nope.c`: `rustycc: cannot read 'nope.c': No such file or directory (os error 2)`,
   exit 1.
3. `./target/debug/rustycc /tmp/hello.c --dump-tokens --check`: clap reports the conflict, exit 2. The
   stage flags are one conflict group, so a run stops in exactly one place.

### 12. Arbitrary bytes

```bash
head -c 4096 /dev/urandom > /tmp/garbage.c
./target/debug/rustycc /tmp/garbage.c --dump-tokens
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

## The test corpus

`tests/programs/` holds five valid subset-C programs, one per feature area. They are the parser's
snapshots now, the golden programs in Phase 4, and the differential corpus in Phase 5, so a program
added there earns its keep three times.

1. `ls tests/programs/`: `arithmetic.c`, `control_flow.c`, `functions.c`, `arrays.c`, `strings.c`,
   and `COVERAGE.md`.
2. `./target/debug/rustycc tests/programs/arrays.c --dump-ast`: any of them, by hand.
3. Adding one means adding a row to `tests/programs/COVERAGE.md` in the same change. A program with
   no row fails `cargo test --test parser_snapshots`, on the grounds that a corpus nobody wrote
   down the purpose of stops being a record of coverage and becomes a pile of files.

`tests/adversarial/` is the other half: inputs aimed at how the front end breaks rather than at what
it supports — nesting deep enough to exhaust the stack, a file of nothing but operators, a 60 KB
identifier, a file with no tokens at all. They are checked in rather than generated so that a crash
found once can never come back unnoticed.

## Snapshots

1. `cargo install cargo-insta`: not installed yet. Without it a changed snapshot still fails the
   test and writes a `.snap.new` beside the old one to diff by hand;
   `INSTA_UPDATE=always cargo test` rewrites the `.snap` files in place, which is the same thing
   with the reviewing step left to `git diff`.
2. `cargo insta review`: step through diffs interactively. Read the whole snapshot before accepting
   — the AST for `control_flow.c` is 350 lines and a branch attached to the wrong `if` looks like a
   right one at a glance.
3. `ls tests/snapshots/`: six files — the token stream for the lexer's representative program, and
   the AST for each of the five corpus programs. Two smaller formats are pinned by inline snapshots
   next to the code that produces them instead: the diagnostic rendering in `src/diagnostics.rs`,
   and a dump with spans on in `tests/parser_snapshots.rs`.

## Docs and branch review

1. `uv run mkdocs serve`: site at `127.0.0.1:8000` with live reload. Run
   `uv venv && uv pip install -r requirements-docs.txt` first if the environment is fresh.
2. `git log --oneline main..HEAD`: the commits, one per task in the epic's checklist order.
3. `git show --stat <hash>`: any single task on its own.
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
| `--dump-ast` | the parser | prints the syntax tree |
| `--check` | semantic analysis | accepted, prints nothing — analyzer is phase 3 |
| `-S` | code generation | accepted, writes nothing — backend is phase 4 |
| `-o <FILE>` | — | parsed and carried; nothing links yet |
| `--keep-temps` | — | parsed; the driver that makes temp files is phase 4 |

`rustycc program.c -o program` parses its arguments and exits 0 without producing an executable. The
shape of the CLI is fixed; later phases fill it in.
