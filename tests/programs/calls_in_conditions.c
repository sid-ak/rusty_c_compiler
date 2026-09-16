// Calls where a condition goes: in an `if`, in a loop's test, in a `for` clause, and on both sides
// of a short-circuit, where whether the call happens at all is the thing being tested.
// expect-exit: 0
// expect-stdout: tf2 4 34 1t2\n

void print_int(int n);
void print_char(char c);

int probes = 0;

int probe(int n) {
    probes = probes + 1;
    return n;
}

int below(int n, int limit) {
    return n < limit;
}

int main(void) {
    if (probe(1)) {
        print_char('t');
    }
    if (probe(0)) {
        print_char('?');
    } else {
        print_char('f');
    }
    print_int(probes);
    print_char(' ');

    // The condition of a `while`, so the call runs once per test, including the test that fails.
    int i = 0;
    probes = 0;
    while (below(i, 4)) {
        i = i + 1;
    }
    print_int(i);
    print_char(' ');

    // Every clause of a `for`.
    probes = 0;
    int total = 0;
    for (int k = probe(0); below(k, 3); k = k + probe(1)) {
        total = total + k;
    }
    print_int(total);
    print_int(probes);
    print_char(' ');

    // Short-circuit, where the right-hand call runs only if the left-hand one decided nothing.
    probes = 0;
    if (probe(0) && probe(1)) {
        print_char('?');
    }
    print_int(probes);
    if (probe(1) || probe(1)) {
        print_char('t');
    }
    print_int(probes);
    print_char('\n');

    return 0;
}
