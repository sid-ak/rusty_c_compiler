// The six comparison operators, each asked a question it answers yes to and one it answers no to.
// A comparison in C is an int: exactly 0 or exactly 1, which is why the results are printed rather
// than only branched on.
// expect-exit: 0
// expect-stdout: 100 100 110 110 1010 111\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int small = 3;
    int large = 8;
    int same = 3;

    print_int(small < large);
    print_int(large < small);
    print_int(small < same);
    print_char(' ');

    print_int(large > small);
    print_int(small > large);
    print_int(small > same);
    print_char(' ');

    print_int(small <= large);
    print_int(small <= same);
    print_int(large <= small);
    print_char(' ');

    print_int(large >= small);
    print_int(small >= same);
    print_int(small >= large);
    print_char(' ');

    print_int(small == same);
    print_int(small == large);
    print_int(small != large);
    print_int(small != same);
    print_char(' ');

    // Negative operands, where an unsigned comparison would give the opposite answer.
    print_int(-5 < 1);
    print_int(-5 > -9);
    print_int(-5 <= -5);
    print_char('\n');

    return 0;
}
