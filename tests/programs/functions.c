// Functions: forward declarations, recursion, a `void` return, and parameter counts either side
// of the eight-register boundary the ARM64 calling convention draws.

void print_int(int n);
void print_char(char c);

// Declared here, defined below, and called from `main` before its definition is reached.
int factorial(int n);
int fibonacci(int n);
int sum_nine(int a, int b, int c, int d, int e, int f, int g, int h, int i);

int global_counter;

void bump(void) {
    global_counter = global_counter + 1;
}

int zero(void) {
    return 0;
}

int identity(int n) {
    return n;
}

int add(int a, int b) {
    return a + b;
}

int sum_eight(int a, int b, int c, int d, int e, int f, int g, int h) {
    return a + b + c + d + e + f + g + h;
}

// Nine parameters: the ninth argument is passed on the stack rather than in a register, which is
// the boundary worth having a program for.
int sum_nine(int a, int b, int c, int d, int e, int f, int g, int h, int i) {
    return sum_eight(a, b, c, d, e, f, g, h) + i;
}

int factorial(int n) {
    if (n <= 1) {
        return 1;
    }
    return n * factorial(n - 1);
}

// Two recursive calls per level, so a wrong stack frame shows up as a wrong answer rather than as
// a value that happens to survive.
int fibonacci(int n) {
    if (n < 2) {
        return n;
    }
    return fibonacci(n - 1) + fibonacci(n - 2);
}

int main(void) {
    print_int(zero());
    print_int(identity(42));
    print_int(add(2, 3));
    print_int(add(add(1, 2), add(3, 4)));

    print_int(sum_eight(1, 2, 3, 4, 5, 6, 7, 8));
    print_int(sum_nine(1, 2, 3, 4, 5, 6, 7, 8, 9));

    print_int(factorial(5));
    print_int(fibonacci(10));

    global_counter = 0;
    bump();
    bump();
    print_int(global_counter);

    // An argument that is itself an assignment: one argument, not two.
    int n = 0;
    print_int(identity(n = 7));
    print_int(n);

    print_char('\n');

    return 0;
}
