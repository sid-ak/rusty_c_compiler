// `for` with each of its three clauses present and absent, all eight combinations, since each one
// is a separate path through the lowering.
// expect-exit: 0
// expect-stdout: 6 6 6 6 6 6 4 4 9\n

void print_int(int n);
void print_char(char c);

int main(void) {
    // All three present.
    int a = 0;
    for (int i = 0; i < 4; i = i + 1) {
        a = a + i;
    }
    print_int(a);
    print_char(' ');

    // No init.
    int b = 0;
    int j = 0;
    for (; j < 4; j = j + 1) {
        b = b + j;
    }
    print_int(b);
    print_char(' ');

    // No condition: an endless loop that has to be left another way.
    int c = 0;
    for (int i = 0; ; i = i + 1) {
        if (i == 4) {
            break;
        }
        c = c + i;
    }
    print_int(c);
    print_char(' ');

    // No step, so the body advances the counter itself.
    int d = 0;
    for (int i = 0; i < 4; ) {
        d = d + i;
        i = i + 1;
    }
    print_int(d);
    print_char(' ');

    // Init only.
    int e = 0;
    for (int i = 0; ; ) {
        e = e + i;
        i = i + 1;
        if (i == 4) {
            break;
        }
    }
    print_int(e);
    print_char(' ');

    // Condition only.
    int f = 0;
    int g = 0;
    for (; g < 4; ) {
        f = f + g;
        g = g + 1;
    }
    print_int(f);
    print_char(' ');

    // Step only.
    int h = 0;
    for (; ; h = h + 1) {
        if (h == 4) {
            break;
        }
    }
    print_int(h);
    print_char(' ');

    // None at all.
    int k = 0;
    for (; ; ) {
        k = k + 1;
        if (k == 4) {
            break;
        }
    }
    print_int(k);
    print_char(' ');

    // An expression rather than a declaration as the init clause.
    int m = 0;
    int n = 0;
    for (n = 2; n < 5; n = n + 1) {
        m = m + n;
    }
    print_int(m);
    print_char('\n');

    return 0;
}
