// Precedence across the whole operator table, with the operands chosen so the wrong grouping gives
// a different answer rather than the same one by luck.
// clang-warns: -Wlogical-op-parentheses
// expect-exit: 0
// expect-stdout: 7 9 26 17 12 1 2 1 1 1 0 -6 2 0\n

void print_int(int n);
void print_char(char c);

int main(void) {
    // Multiplicative before additive.
    print_int(1 + 2 * 3);
    print_char(' ');
    print_int((1 + 2) * 3);
    print_char(' ');
    print_int(2 * 3 + 4 * 5);
    print_char(' ');
    print_int(20 - 12 / 4);
    print_char(' ');
    print_int(20 % 7 * 2);
    print_char(' ');

    // Additive before relational.
    print_int(1 + 2 < 4);
    print_char(' ');
    print_int(1 + (2 < 4));
    print_char(' ');

    // Relational before equality: (3 < 4) == (1 < 2) is 1 == 1.
    print_int(3 < 4 == 1 < 2);
    print_char(' ');

    // Equality before logical and, logical and before logical or.
    print_int(0 == 1 || 2 == 2);
    print_char(' ');
    print_int(1 || 0 && 0);
    print_char(' ');
    print_int((1 || 0) && 0);
    print_char(' ');

    // Unary binds tighter than anything binary.
    print_int(-2 * 3);
    print_char(' ');
    print_int(!0 + 1);
    print_char(' ');
    print_int(!(0 + 1));
    print_char('\n');

    return 0;
}
