// The unary operators, stacked and mixed with binary ones, where the question is what each one
// binds to.
// expect-exit: 0
// expect-stdout: -6 6 6 6 010 -12 -12 -4 1 -6 1 4\n

void print_int(int n);
void print_char(char c);

int identity(int n) {
    return n;
}

int main(void) {
    int n = 6;

    print_int(-n);
    print_char(' ');
    print_int(+n);
    print_char(' ');
    print_int(- -n);
    print_char(' ');
    print_int(-+-n);
    print_char(' ');

    print_int(!n);
    print_int(!!n);
    print_int(!-n);
    print_char(' ');

    // Unary binds tighter than the binary operators around it.
    print_int(-n * 2);
    print_char(' ');
    print_int(-(n * 2));
    print_char(' ');
    print_int(-n + 2);
    print_char(' ');

    // And tighter than a comparison, so this is (-n) < 0.
    print_int(-n < 0);
    print_char(' ');

    // Unary applied to a call, and to a parenthesized expression.
    print_int(-identity(n));
    print_char(' ');
    print_int(!identity(0));
    print_char(' ');
    print_int(-(n - 10));
    print_char('\n');

    return 0;
}
