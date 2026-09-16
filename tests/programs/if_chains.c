// Branching: a plain `if`, an `if`/`else`, and an else-if chain, each taken through every arm so no
// arm is only present rather than reached.
// expect-exit: 0
// expect-stdout: 0123 1 tf a\n

void print_int(int n);
void print_char(char c);

int classify(int n) {
    if (n < 0) {
        return 0;
    } else if (n == 0) {
        return 1;
    } else if (n < 10) {
        return 2;
    } else {
        return 3;
    }
}

int main(void) {
    print_int(classify(-4));
    print_int(classify(0));
    print_int(classify(5));
    print_int(classify(99));
    print_char(' ');

    // An `if` with no `else`, taken and not taken.
    int taken = 0;
    if (1) {
        taken = taken + 1;
    }
    if (0) {
        taken = taken + 10;
    }
    print_int(taken);
    print_char(' ');

    // The condition is any expression, not only a comparison.
    int n = 3;
    if (n) {
        print_char('t');
    }
    if (n - 3) {
        print_char('?');
    } else {
        print_char('f');
    }
    print_char(' ');

    // Nested, where the inner branch only runs because the outer one did.
    if (n > 0) {
        if (n > 2) {
            print_char('a');
        } else {
            print_char('b');
        }
    } else {
        print_char('c');
    }
    print_char('\n');

    return 0;
}
