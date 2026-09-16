// Associativity on its own: the operators that group left to right, and assignment, which groups
// right to left.
// expect-exit: 0
// expect-stdout: 50 90 8 32 0 1 999 44\n

void print_int(int n);
void print_char(char c);

int main(void) {
    // Left-associative, checked with operands that make the two groupings disagree.
    print_int(100 - 30 - 20);
    print_char(' ');
    print_int(100 - (30 - 20));
    print_char(' ');
    print_int(64 / 4 / 2);
    print_char(' ');
    print_int(64 / (4 / 2));
    print_char(' ');
    print_int(29 % 12 % 5);
    print_char(' ');
    print_int(29 % (12 % 5));
    print_char(' ');

    // Right-associative: c is assigned first, then b, then a, so all three end up 9.
    int a = 0;
    int b = 0;
    int c = 0;
    a = b = c = 9;
    print_int(a);
    print_int(b);
    print_int(c);
    print_char(' ');

    // The value of an assignment is the value assigned, so it can be used in place rather than
    // read back afterwards. Read back here as well, to show the two agree.
    int d = 0;
    print_int(d = 4);
    print_int(d);
    print_char('\n');

    return 0;
}
