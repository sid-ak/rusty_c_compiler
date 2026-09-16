// Addition and subtraction: how a chain of them groups, and what unary minus and plus do to the
// operands inside one.
// expect-exit: 0
// expect-stdout: 47 33 30 36 36 44 -33 33 -33 47\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int a = 40;
    int b = 7;
    int c = 3;

    print_int(a + b);
    print_char(' ');
    print_int(a - b);
    print_char(' ');

    // Subtraction is left-associative, so this is (a - b) - c and not a - (b - c). The two differ
    // by 2 * c, which is why c is not zero.
    print_int(a - b - c);
    print_char(' ');
    print_int(a - (b - c));
    print_char(' ');

    // Mixed, still left to right.
    print_int(a - b + c);
    print_char(' ');
    print_int(a + b - c);
    print_char(' ');

    print_int(-a + b);
    print_char(' ');
    print_int(+a - b);
    print_char(' ');
    print_int(-(a - b));
    print_char(' ');
    print_int(a - -b);
    print_char('\n');

    return 0;
}
