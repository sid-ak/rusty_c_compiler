# Cheatsheet

Commands for building the compiler, running its tests, and driving it by hand. Run everything from
the repo root.

Every expected output below was copied from a real run, not written from memory. The page is
refreshed at the end of each phase, so it covers what the compiler can actually do now rather than
what it will eventually do.

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
3. `cargo test`: the whole suite, about a minute on an M1 — most of it spent in `clang`, since the
   program tiers compile and run every corpus program two and three times over.
4. `uv run mkdocs build --strict`: fails on a broken link or a page missing from `nav`. The red
   MkDocs 2.0 block is an advisory banner from mkdocs-material, not an error — read the last line.

## Narrower test runs

| Command | Scope |
| --- | --- |
| `cargo test --lib` | in-crate units; no C compiled, no processes spawned |
| `cargo test --test cli` | usage, exit codes, `--check`, and the driver's flags and temp files |
| `cargo test --test differential` | every corpus program under both compilers, run and compared |
| `cargo test --test codegen_exec` | every corpus program against the answer recorded in its header |
| `cargo test --test codegen_programs` | ~200 small C programs through the whole pipeline |
| `cargo test --test codegen_snapshots` | the emitter's output, and that an assembler accepts it |
| `cargo test --test generated` | randomly generated programs through the same comparison |
| `cargo test --test harness_self_tests` | the differential harness, broken on purpose one axis at a time |
| `cargo test --test lexer_snapshots` | full token stream for a representative program |
| `cargo test --test parser_snapshots` | AST for every corpus program, and the coverage matrix |
| `cargo test --test frontend_no_panic` | truncations, `tests/adversarial/`, and past fuzz crashes |
| `cargo test --test runtime_shim` | compiles, links, runs C against the real `shim.o` |
| `cargo test --test sema_snapshots` | annotations for every corpus program, and that each analyzes |
| `cargo test --test invalid_programs` | every rejection rule, and what clang makes of it |
| `cargo test --test differential -- arrays` | one corpus program, since each is its own test |
| `cargo test --lib precedence` | any substring filters by test name |
| `cargo test -- --nocapture` | show `println!` from passing tests |

Three environment variables change what the program tiers do:

| Variable | Effect |
| --- | --- |
| `RUSTYCC_DIFF_TIMEOUT_SECS` | the wall-clock limit a compiled program is given; default 10 |
| `RUSTYCC_GENERATED_PROGRAMS` | how many programs the generator writes; default 40 |
| `RUSTYCC_GENERATED_SEED` | the seed to start from, which reproduces a reported failure exactly |

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

A `#` that starts a line begins a preprocessor directive, which is reported once and skipped whole:

```bash
printf '#define SUM(a, b) \\\n    ((a) + (b))\nint x = a # b;\n' > /tmp/directive.c
./target/debug/rustycc /tmp/directive.c --dump-tokens
```

```
/tmp/directive.c:1:1: error: unsupported in this C subset: preprocessor directives
#define SUM(a, b) \
^~~~~~~~~~~~~~~~~~~
note: this subset has no preprocessor, so no directive has any meaning here
/tmp/directive.c:3:11: error: unsupported in this C subset: '#'
int x = a # b;
          ^
note: this subset has no preprocessor, so no directive has any meaning here
```

- The backslash at the end of line 1 continues the directive onto line 2, so the macro body produces
  no diagnostics of its own.
- The `#` on line 3 follows a token on its line, so it is not a directive and is reported alone.

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
python3 -c "print('int f(void) { return 1' + '+1'*2000 + '; }')" > /tmp/chain.c
./target/debug/rustycc /tmp/chain.c --dump-ast | head -3
```

```
/tmp/deep.c:1:85: error: nesting is too deep: the syntax tree goes at most 128 levels deep
/tmp/chain.c:1:274: error: nesting is too deep: the syntax tree goes at most 128 levels deep
```

Exit 1 for both. Recursive descent recurses, so without the limit the first input is a stack
overflow, and a crash is the one thing the front end is not allowed to produce. The second input
never makes the parser recurse — `1+1+1` is built by a loop — but it still builds a tree 2,000
levels deep, and the dump walks that tree recursively, so it would overflow there instead. That is
why the limit counts levels of the tree rather than the parser's own calls: a parenthesized
expression costs two, which is why the first caret lands on the 64th `(`, and each operator in a
chain costs one, which is why the second lands just past the 126th `+`. The number is a stack
budget: nested blocks, the costliest shape, need about 550 KB of stack at the limit in an
unoptimized build, and a unit test parses, dumps, and drops every deep shape on a deliberately
undersized 1 MiB stack so the margin is enforced rather than assumed.

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

### 13. Check a program, and see what analysis recorded

```bash
cat > /tmp/ann.c <<'EOF'
int add(int a, char b) {
    return a + b;
}

int main(void) {
    char label[4] = "ok";
    return add(1, label[0]);
}
EOF
./target/debug/rustycc --check /tmp/ann.c
```

Exit 0 and no output at all — which is the point of `--check`: it is meant to be run by something
that reads exit codes. To see what it worked out:

```bash
./target/debug/rustycc --dump-annotations /tmp/ann.c
```

```
types
  #2 int
  #3 char
  #4 int
  #7 char[3]
  #10 int(int, char)
  #11 int
  #12 char[4]
  #13 int
  #14 char
  #15 int
conversions
  #3 char -> int
bindings
  #0 param[0] a
  #1 param[1] b
  #2 param[0] a
  #3 param[1] b
  #8 local label
  #10 function add
  #12 local label
frames
  add
    0 param[0] a size 4 align 4
    1 param[1] b size 1 align 1
  main
    0 local label size 4 align 1
strings
  l_.str.0 "ok"
```

Four things worth reading off it:

1. `#3 char -> int` is the `b` in `a + b`, and it is the only conversion in the file. `label[0]` is
   already a `char` and the parameter it is passed to is a `char`, so nothing happens there.
2. `#7 char[3]` is the literal `"ok"` — two characters and the terminator. The variable it
   initializes is `char[4]`, which is why it fits.
3. The frame for `add` stores its `char` parameter in one byte while computing on it as an `int`.
   Storage and computation are different questions, and this is where that shows.
4. Every number is a node id from the syntax tree. Nothing was written onto the tree to produce
   this; the tree is exactly what `--dump-ast` printed.

### 14. Several semantic errors in one file

```bash
cat > /tmp/bad.c <<'EOF'
int total;
int total;

int describe(int n) {
    int seen;
    seen = n;
}

int main(void) {
    return describe(1, 2) + missing;
}
EOF
./target/debug/rustycc --check /tmp/bad.c
```

Exit 1, and:

```
/tmp/bad.c:2:5: error: redeclaration of 'total' in this scope
int total;
    ^~~~~
/tmp/bad.c:1:5: note: previous declaration of 'total' is here
int total;
    ^~~~~
/tmp/bad.c:7:1: error: control reaches the end of non-void function 'describe'
}
^
/tmp/bad.c:10:12: error: 'describe' takes 1 argument, but 2 were passed
    return describe(1, 2) + missing;
           ^~~~~~~~~~~~~~
/tmp/bad.c:10:29: error: undeclared identifier 'missing'
    return describe(1, 2) + missing;
                            ^~~~~~~
rustycc: 4 errors generated
```

The redeclaration is the one to look at: the note has its own line, its own source line, and its own
caret, pointing at the first `total` rather than merely claiming it exists somewhere. The
fall-off-the-end error points at the closing brace, because that is the place control reaches.

Note also what is *not* reported. `missing` is undeclared, and the `+` it is an operand of says
nothing — one mistake, one message.

### 15. Where the subset is stricter than C

```bash
clang -O0 -std=c99 -fsyntax-only tests/programs/invalid/zero_length_array.c; echo "clang: $?"
./target/debug/rustycc --check tests/programs/invalid/zero_length_array.c; echo "rustycc: $?"
```

clang exits 0. `rustycc` exits 1 with `array size must be greater than zero`. That is deliberate, and
the file says so in its own header. Four programs in `tests/programs/invalid/` are like this; the
full list, and the reasoning, is in
[`architecture.md`](architecture.md#where-this-subset-is-stricter-than-c), and
`cargo test --test invalid_programs` fails if the two lists disagree.

To see every rejection rule and what clang makes of each:

```bash
cat tests/programs/invalid/COVERAGE.md
```

### 16. Compile and run a program

```bash
cat > /tmp/demo.c <<'EOF'
void print_int(int n);
void print_string(char s[]);

int square(int n) { return n * n; }

int main(void) {
    print_string("squares: ");
    for (int i = 1; i <= 5; i = i + 1) {
        print_int(square(i));
        print_string(" ");
    }
    return 0;
}
EOF
./target/debug/rustycc /tmp/demo.c -o /tmp/demo
/tmp/demo
```

```
squares: 1 4 9 16 25
```

There is no preprocessor, so the program declares the three shim functions itself rather than
including a header. The driver links the shim in without being asked.

### 17. Read the assembly

```bash
./target/debug/rustycc /tmp/demo.c -S -o /tmp/demo.s
sed -n '/_square:/,/ret/p' /tmp/demo.s
```

```
_square:
	stp x29, x30, [sp, #-48]!
	mov x29, sp
	str w0, [x29, #16]
	add x0, x29, #16
	ldr w0, [x0]
	str w0, [x29, #24]
	add x0, x29, #16
	ldr w0, [x0]
	mov w1, w0
	ldr w0, [x29, #24]
	mul w0, w0, w1
	b Lsquare_return_0
```

The six-step shape of a binary operation is the thing to read here, and `n * n` shows it even though
both operands are the same:

1. `str w0, [x29, #16]` is the prologue spilling the parameter into its slot.
2. The left operand is loaded, then spilled to the multiplication's own temporary at `#24`.
3. The right operand is loaded and moved to `w1`.
4. The left operand comes back into `w0`.
5. `mul w0, w0, w1` reads left then right, in source order.

Every offset is positive because `x29` sits at the bottom of the frame, and the `b` at the end goes
to the function's single epilogue rather than returning from where it stands.

### 18. Compare against clang, by hand

```bash
SHIM=$(find target/debug/build -name shim.o | head -1)
clang -O0 -std=c99 tests/programs/sorting.c "$SHIM" -o /tmp/oracle
./target/debug/rustycc tests/programs/sorting.c -o /tmp/ours
diff <(/tmp/oracle) <(/tmp/ours) && echo "identical"
```

`identical`. This is what `cargo test --test differential` does for every program in the corpus, one
test each; `cargo test --test codegen_exec` does the recorded-expectation version of the same thing.
When the automated one fails, its message names a directory holding both binaries, both captures of
their output, and the assembly `rustycc` produced.

### 19. Keep the intermediates

```bash
./target/debug/rustycc /tmp/demo.c -o /tmp/demo --keep-temps
```

Prints the directory it kept, which holds the `.s` and the `.o`. Without the flag that directory is
removed however the run ends, including when the link fails — which is exactly when looking at the
assembly is worth doing.

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

`tests/programs/` holds sixty-four valid subset-C programs. Each one is the parser's snapshot, the
analyzer's snapshot, a golden program, and a differential comparison, so a program added there earns
its keep four times.

1. `ls tests/programs/ | wc -l`: the corpus, plus `COVERAGE.md` and `invalid/`.
2. `./target/debug/rustycc tests/programs/bubble_sort.c --dump-ast`: any of them, by hand.
3. Adding one means adding a row to `tests/programs/COVERAGE.md` in the same change. A program with
   no row fails `cargo test --test parser_snapshots`, on the grounds that a corpus nobody wrote
   down the purpose of stops being a record of coverage and becomes a pile of files.
4. Nothing else has to be edited. `build.rs` reads the directory and generates the list of tests, so
   a program cannot be added and silently never run.
5. A program that provokes a `clang` warning under `-Wall -Wextra` declares it in a
   `// clang-warns:` header, and `cargo test --test differential` fails if the declared set and the
   reported set ever stop matching — in either direction.

`tests/adversarial/` is the other half: inputs aimed at how the front end breaks rather than at what
it supports — nesting deep enough to exhaust the stack, a file of nothing but operators, a 60 KB
identifier, a file with no tokens at all. They are checked in rather than generated so that a crash
found once can never come back unnoticed.

## Fuzzing

Needs a toolchain the compiler itself does not: `rustup toolchain install nightly` and
`cargo install cargo-fuzz`, once.

1. `./scripts/fuzz.sh lex`: fifteen minutes on the lexer — the documented minimum before a front-end
   change is called done. Also `parse` and `frontend`.
2. `./scripts/fuzz.sh parse 60`: a shorter run, to check the target still builds.
3. The seed corpus is assembled by the script from `tests/programs/`, `tests/programs/invalid/`,
   `tests/adversarial/`, and `fuzz/regressions/`, so nothing has to be copied under `fuzz/` by hand.
4. `cargo +nightly fuzz run lex fuzz/artifacts/lex/<file>`: replay a crash it reported.
5. `cargo +nightly fuzz tmin lex fuzz/artifacts/lex/<file>`: minimize it. The result belongs in
   `fuzz/regressions/`, where `cargo test --test frontend_no_panic` runs it from then on.

## Reports

1. `./scripts/test-evidence.sh`: regenerates the toolchain capture and the full-run capture the unit
   test reports cite, into `docs/reports/unit_tests/evidence/`.
2. `python3 scripts/test_inventory.py`: refreshes the table of tests in each unit test report from
   the tests' own doc comments.
3. `python3 scripts/test_inventory.py --check`: what the docs build runs. Fails if a table is stale,
   if two reports claim the same file of tests, or if a file of tests has no report at all.

## Snapshots

1. `cargo install cargo-insta`: optional. Without it a changed snapshot still fails the
   test and writes a `.snap.new` beside the old one to diff by hand;
   `INSTA_UPDATE=always cargo test` rewrites the `.snap` files in place, which is the same thing
   with the reviewing step left to `git diff`.
2. `cargo insta review`: step through diffs interactively. Read the whole snapshot before accepting
   — the AST for `control_flow.c` is 350 lines and a branch attached to the wrong `if` looks like a
   right one at a glance.
3. `ls tests/snapshots/`: the token stream for the lexer's representative program, the AST and the
   annotations for the original feature-area programs, and the emitted assembly for each construct.
   Two smaller formats are pinned by inline snapshots next to the code that produces them instead:
   the diagnostic rendering in `src/diagnostics.rs`, and a dump with spans on in
   `tests/parser_snapshots.rs`.

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
| `--check` | semantic analysis | runs the whole front end; prints nothing when accepted |
| `--dump-annotations` | semantic analysis | prints the types, conversions, bindings, frames, and interned literals |
| `-S` | code generation | writes the ARM64 assembly |
| `-c` | assembling | writes an object file |
| `--emit-asm-to <FILE>` | — | also writes the assembly, whatever else is produced |
| `-o <FILE>` | — | where the output goes; defaults to the input's stem |
| `--keep-temps` | — | leaves the intermediates and prints where they are |

`rustycc program.c -o program && ./program` works.
