// Multiplication, division, and remainder, including the signs C gives a quotient and a remainder
// when one operand is negative: division truncates toward zero, so the remainder takes the sign of
// the left operand.
// expect-exit: 0
// expect-stdout: 42 21 4 -3 -3 3 -1 1 -1 -4 0 10 50\n

void print_int(int n);
void print_char(char c);

int main(void) {
    print_int(6 * 7);
    print_char(' ');
    print_int(84 / 4);
    print_char(' ');
    print_int(84 % 5);
    print_char(' ');

    print_int(-7 / 2);
    print_char(' ');
    print_int(7 / -2);
    print_char(' ');
    print_int(-7 / -2);
    print_char(' ');

    print_int(-7 % 2);
    print_char(' ');
    print_int(7 % -2);
    print_char(' ');
    print_int(-7 % -2);
    print_char(' ');

    // Exact division, so no truncation is involved either way.
    print_int(-8 / 2);
    print_char(' ');
    print_int(-8 % 2);
    print_char(' ');

    // Left-associative: (100 / 5) / 2 is 10, while 100 / (5 / 2) is 50.
    print_int(100 / 5 / 2);
    print_char(' ');
    print_int(100 / (5 / 2));
    print_char('\n');

    return 0;
}
