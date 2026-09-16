// Calls inside calls. Placing an argument register and then evaluating the next argument loses that
// register the moment the next argument is itself a call, so every shape where that can happen is
// written out here.
// expect-exit: 0
// expect-stdout: 7 123123123 456 1010 285 246\n

void print_int(int n);
void print_char(char c);

int identity(int n) {
    return n;
}

int add(int a, int b) {
    return a + b;
}

int three(int a, int b, int c) {
    return a * 100 + b * 10 + c;
}

int nine(int a, int b, int c, int d, int e, int f, int g, int h, int i) {
    return a + b * 2 + c * 3 + d * 4 + e * 5 + f * 6 + g * 7 + h * 8 + i * 9;
}

int main(void) {
    // A call as the only argument, nested three deep.
    print_int(identity(identity(identity(7))));
    print_char(' ');

    // A call in each argument position, so no position is left as the one that happens to work.
    print_int(three(identity(1), 2, 3));
    print_int(three(1, identity(2), 3));
    print_int(three(1, 2, identity(3)));
    print_char(' ');

    // Every argument a call.
    print_int(three(identity(4), identity(5), identity(6)));
    print_char(' ');

    // A call whose argument is a call with its own arguments.
    print_int(add(add(1, 2), add(3, 4)));
    print_int(add(add(add(1, 2), 3), 4));
    print_char(' ');

    // Nesting that reaches past the register boundary, where the pending arguments live on the
    // stack while the inner call runs.
    print_int(nine(identity(1), 2, 3, 4, 5, 6, 7, 8, identity(9)));
    print_char(' ');

    // A call mixed with arithmetic in the same argument list.
    print_int(three(identity(1) + 1, identity(2) * 2, 9 - identity(3)));
    print_char('\n');

    return 0;
}
