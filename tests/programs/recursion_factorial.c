// The simplest recursion there is: one call per level, unwinding on the way back. Kept under the
// range where a factorial would overflow a 32-bit int, since an overflow is undefined and an
// undefined program cannot be compared against anything.
// expect-exit: 0
// expect-stdout: 1 1 2 6 24 120 720 5040 40320 \n1\n

void print_int(int n);
void print_char(char c);

int factorial(int n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

// The same function written as a loop, so the two answers can be checked against each other.
int factorial_loop(int n) {
    int result = 1;
    for (int i = 2; i <= n; i = i + 1) {
        result = result * i;
    }
    return result;
}

int main(void) {
    for (int i = 0; i <= 8; i = i + 1) {
        print_int(factorial(i));
        print_char(' ');
    }
    print_char('\n');

    int matches = 1;
    for (int i = 0; i <= 12; i = i + 1) {
        if (factorial(i) != factorial_loop(i)) {
            matches = 0;
        }
    }
    print_int(matches);
    print_char('\n');

    return 0;
}
