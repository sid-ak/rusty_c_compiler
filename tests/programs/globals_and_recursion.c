// A global touched by a recursive function, so one piece of storage is reached from every frame at
// once. A local would be a separate copy per level; a global is not, and the order the frames write
// in is visible in what is left behind.
// expect-exit: 0
// expect-stdout: 066 43210 01234 55177\n

void print_int(int n);
void print_char(char c);

int depth = 0;
int deepest = 0;
int visits = 0;
int trail[16];

void descend(int n) {
    depth = depth + 1;
    visits = visits + 1;
    if (depth > deepest) {
        deepest = depth;
    }
    if (n > 0) {
        descend(n - 1);
    }
    depth = depth - 1;
}

// Records the order calls are entered in, which is the descent, and then the order they return in.
int entered = 0;
int returned = 0;

void record(int n) {
    trail[entered] = n;
    entered = entered + 1;
    if (n > 0) {
        record(n - 1);
    }
    trail[8 + returned] = n;
    returned = returned + 1;
}

int fib(int n) {
    visits = visits + 1;
    if (n < 2) {
        return n;
    }
    return fib(n - 1) + fib(n - 2);
}

int main(void) {
    descend(5);
    print_int(depth);
    print_int(deepest);
    print_int(visits);
    print_char(' ');

    record(4);
    for (int i = 0; i < 5; i = i + 1) {
        print_int(trail[i]);
    }
    print_char(' ');
    for (int i = 0; i < 5; i = i + 1) {
        print_int(trail[8 + i]);
    }
    print_char(' ');

    // The call count of a doubly recursive function, which only a shared counter can see.
    visits = 0;
    print_int(fib(10));
    print_int(visits);
    print_char('\n');

    return 0;
}
