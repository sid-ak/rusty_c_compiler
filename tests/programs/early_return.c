// Returning from the middle of a function: out of a branch, out of a loop, and out of a loop nested
// in another, where the return has to unwind past everything at once.
// expect-exit: 0
// expect-stdout: 41-1 023-1 0112 z4\n

void print_int(int n);
void print_char(char c);

int first_over(int limit) {
    for (int i = 0; i < 100; i = i + 1) {
        if (i * i > limit) {
            return i;
        }
    }
    return -1;
}

int found_in_grid(int target) {
    for (int row = 0; row < 5; row = row + 1) {
        for (int column = 0; column < 5; column = column + 1) {
            if (row * 5 + column == target) {
                return row * 10 + column;
            }
        }
    }
    return -1;
}

int guard(int n) {
    if (n < 0) {
        return 0;
    }
    if (n == 0) {
        return 1;
    }
    return n * 2;
}

void announce(int n) {
    if (n == 0) {
        print_char('z');
        return;
    }
    print_int(n);
}

int main(void) {
    print_int(first_over(10));
    print_int(first_over(0));
    print_int(first_over(99999));
    print_char(' ');

    print_int(found_in_grid(0));
    print_int(found_in_grid(13));
    print_int(found_in_grid(99));
    print_char(' ');

    print_int(guard(-1));
    print_int(guard(0));
    print_int(guard(6));
    print_char(' ');

    announce(0);
    announce(4);
    print_char('\n');

    return 0;
}
