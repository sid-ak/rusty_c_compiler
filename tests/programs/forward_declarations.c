// Declaring a function before defining it, which is what lets two functions call each other and
// what lets a caller appear above its callee.
// expect-exit: 0
// expect-stdout: 6103 109 20\n

void print_int(int n);
void print_char(char c);

int later(int n);
int also_later(int n);

int caller(int n) {
    return later(n) + also_later(n);
}

int main(void) {
    print_int(later(3));
    print_int(also_later(3));
    print_char(' ');

    print_int(caller(3));
    print_char(' ');

    // Declared, defined, and then called again, to show the definition did not replace anything.
    print_int(later(10));
    print_char('\n');

    return 0;
}

int later(int n) {
    return n * 2;
}

int also_later(int n) {
    return n + 100;
}
