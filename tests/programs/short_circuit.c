// Short-circuiting, made visible by side effects: the right-hand operand of `&&` runs only when the
// left is true, and the right-hand operand of `||` only when the left is false. A compiler that
// evaluated both would print the same answers and a different trace.
// expect-exit: 0
// expect-stdout: 00 *01 11 *12 012 01\n

void print_int(int n);
void print_char(char c);

int calls = 0;

int mark(int value) {
    calls = calls + 1;
    print_char('*');
    return value;
}

int main(void) {
    // False and anything: the right side must not run.
    print_int(0 && mark(1));
    print_int(calls);
    print_char(' ');

    // True and anything: it must.
    print_int(1 && mark(0));
    print_int(calls);
    print_char(' ');

    // True or anything: the right side must not run.
    print_int(1 || mark(0));
    print_int(calls);
    print_char(' ');

    // False or anything: it must.
    print_int(0 || mark(1));
    print_int(calls);
    print_char(' ');

    // A chain stops at the first operand that decides the answer.
    print_int(0 && mark(1) && mark(1));
    print_int(1 || mark(1) || mark(1));
    print_int(calls);
    print_char(' ');

    // Increment as the side effect, which is the form this actually takes in real code.
    int i = 0;
    int guard = 0;
    if (guard && i++) {
        print_char('?');
    }
    print_int(i);
    if (guard || i++) {
        print_char('?');
    }
    print_int(i);
    print_char('\n');

    return 0;
}
