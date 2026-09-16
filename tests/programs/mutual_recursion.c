// Two functions that call each other, which needs the forward declaration to exist before either
// body does.
// expect-exit: 0
// expect-stdout: 100110011001 1 10 26 24 42 74 \n

void print_int(int n);
void print_char(char c);

int is_odd(int n);

int is_even(int n) {
    if (n == 0) {
        return 1;
    }
    return is_odd(n - 1);
}

int is_odd(int n) {
    if (n == 0) {
        return 0;
    }
    return is_even(n - 1);
}

// A three-way cycle, where the return trip passes through a function that is neither the caller nor
// the callee of the previous step.
int step_c(int n);

int step_a(int n) {
    if (n <= 0) {
        return 1;
    }
    return step_c(n - 1) * 2;
}

int step_b(int n) {
    if (n <= 0) {
        return 3;
    }
    return step_a(n - 1) + 1;
}

int step_c(int n) {
    if (n <= 0) {
        return 5;
    }
    return step_b(n - 1) + 10;
}

int main(void) {
    for (int i = 0; i < 6; i = i + 1) {
        print_int(is_even(i));
        print_int(is_odd(i));
    }
    print_char(' ');

    for (int i = 0; i < 6; i = i + 1) {
        print_int(step_a(i));
        print_char(' ');
    }
    print_char('\n');

    return 0;
}
