// Expressions deep enough that every intermediate value needs somewhere to live. This compiler
// gives each one its own stack slot rather than a register, and the question here is whether two
// intermediates ever land in the same place — which shows up as an operand that has quietly become
// the wrong number.
// expect-exit: 0
// expect-stdout: 60 -29 -22 120 1 2 0 11 1 85 -24 405\n

void print_int(int n);
void print_char(char c);

int add(int a, int b) {
    return a + b;
}

int main(void) {
    int a = 2;
    int b = 3;
    int c = 5;
    int d = 7;

    // A balanced tree, where both halves are live at once.
    print_int((a + b) * (c + d));
    print_char(' ');
    print_int((a * b) - (c * d));
    print_char(' ');

    // Deeper, with the same variables appearing at several levels.
    print_int(((a + b) * (c - d)) + ((a - b) * (c + d)));
    print_char(' ');
    print_int((((a + 1) * (b + 1)) + ((c + 1) * (d + 1))) * 2);
    print_char(' ');

    // Non-commutative operators at every level, so a transposed operand changes the answer.
    print_int((d - c) - (b - a));
    print_char(' ');
    print_int((d / a) - (c / b));
    print_char(' ');
    print_int(((d - a) / (b - a)) % c);
    print_char(' ');

    // Comparisons feeding logical operators feeding arithmetic.
    print_int((a < b) + (c < d) * 10);
    print_char(' ');
    print_int(((a < b) && (c < d)) + ((a > b) || (c > d)));
    print_char(' ');

    // Calls at several levels of the same tree, so a temporary has to survive a call.
    print_int(add(a + b, c + d) * add(a, b));
    print_char(' ');
    print_int(add(add(a, b), add(c, d)) - add(a * b, c * d));
    print_char(' ');

    // An assignment nested inside a larger expression, which both stores and produces a value. The
    // other operand is a variable the assignment does not touch, so there is one order to read this
    // in rather than two.
    int n = 0;
    print_int((n = a + b) * (d + 1));
    print_int(n);
    print_char('\n');

    return 0;
}
