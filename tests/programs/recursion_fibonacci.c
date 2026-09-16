// Double recursion: two calls per level rather than one, so the call stack branches instead of
// forming a line. The second call cannot reuse whatever the first left behind.
// expect-exit: 0
// expect-stdout: 0 1 1 2 3 5 8 13 21 34 55 89 \n1\n

void print_int(int n);
void print_char(char c);

int fib(int n) {
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

int fib_loop(int n) {
    int previous = 0;
    int current = 1;
    if (n == 0) {
        return 0;
    }
    for (int i = 1; i < n; i = i + 1) {
        int next = previous + current;
        previous = current;
        current = next;
    }
    return current;
}

int main(void) {
    for (int i = 0; i < 12; i = i + 1) {
        print_int(fib(i));
        print_char(' ');
    }
    print_char('\n');

    int matches = 1;
    for (int i = 0; i < 20; i = i + 1) {
        if (fib(i) != fib_loop(i)) {
            matches = 0;
        }
    }
    print_int(matches);
    print_char('\n');

    return 0;
}
