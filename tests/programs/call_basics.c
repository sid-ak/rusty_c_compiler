// Calling a function: arguments in, a value back, and the value used where the call was written.
// expect-exit: 0
// expect-stdout: 042 -99 51 123321 301-9 160\n

void print_int(int n);
void print_char(char c);

int zero(void) {
    return 0;
}

int constant(void) {
    return 42;
}

int negate(int n) {
    return -n;
}

int add(int a, int b) {
    return a + b;
}

int weigh(int a, int b, int c) {
    return a * 100 + b * 10 + c;
}

int main(void) {
    print_int(zero());
    print_int(constant());
    print_char(' ');

    print_int(negate(9));
    print_int(negate(-9));
    print_char(' ');

    print_int(add(2, 3));
    print_int(add(-2, 3));
    print_char(' ');

    // Arguments are positional, so a function whose result depends on the order catches a harness
    // that passes them the other way round.
    print_int(weigh(1, 2, 3));
    print_int(weigh(3, 2, 1));
    print_char(' ');

    // A call is an expression: usable in arithmetic, in a comparison, and as an argument.
    print_int(add(1, 2) * 10);
    print_int(add(1, 2) > 2);
    print_int(negate(add(4, 5)));
    print_char(' ');

    // Arguments that are themselves expressions.
    int n = 5;
    print_int(add(n + 1, n * 2));
    print_int(add(negate(n), n));
    print_char('\n');

    return 0;
}
