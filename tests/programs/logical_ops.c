// Logical and, or, and not: what they yield rather than only which way they branch. Each is 0 or 1
// regardless of how large the operands that produced it were.
// expect-exit: 0
// expect-stdout: 1000 1110 1010 1 1 1 0 10\n

void print_int(int n);
void print_char(char c);

int main(void) {
    print_int(1 && 1);
    print_int(1 && 0);
    print_int(0 && 1);
    print_int(0 && 0);
    print_char(' ');

    print_int(1 || 1);
    print_int(1 || 0);
    print_int(0 || 1);
    print_int(0 || 0);
    print_char(' ');

    print_int(!0);
    print_int(!1);
    print_int(!!5);
    print_int(!5);
    print_char(' ');

    // Any non-zero value is true, and the result is still normalized to 1. Held in variables
    // rather than written as literals, so the answer is computed rather than folded.
    int seven = 7;
    int nine = 9;
    int negative = -3;
    print_int(seven && nine);
    print_char(' ');
    print_int(seven || nine);
    print_char(' ');
    print_int(negative && seven);
    print_char(' ');
    print_int(!negative);
    print_char(' ');

    // Comparisons feeding logical operators, which is where they actually get used.
    int n = 15;
    print_int(n > 10 && n < 20);
    print_int(n < 10 || n > 20);
    print_char('\n');

    return 0;
}
