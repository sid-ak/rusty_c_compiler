// Arrays across the function boundary. An array argument becomes the address of its first element,
// so the callee is reading and writing the caller's own storage rather than a copy of it.
// expect-exit: 0
// expect-stdout: 15 302 4 6 8 10 100100 128 110\n

void print_int(int n);
void print_char(char c);

int sum(int values[], int count) {
    int total = 0;
    for (int i = 0; i < count; i = i + 1) {
        total = total + values[i];
    }
    return total;
}

void double_all(int values[], int count) {
    for (int i = 0; i < count; i = i + 1) {
        values[i] = values[i] * 2;
    }
}

void set_first(int values[], int value) {
    values[0] = value;
}

int first(int values[]) {
    return values[0];
}

// Takes an already-decayed parameter and hands it on to another function, which is the shape where
// a pointer that was stored at the wrong width stops being a pointer.
int forward(int values[], int count) {
    return sum(values, count);
}

int main(void) {
    int numbers[5] = {1, 2, 3, 4, 5};

    print_int(sum(numbers, 5));
    print_char(' ');

    double_all(numbers, 5);
    print_int(sum(numbers, 5));
    for (int i = 0; i < 5; i = i + 1) {
        print_int(numbers[i]);
        print_char(' ');
    }

    set_first(numbers, 100);
    print_int(numbers[0]);
    print_int(first(numbers));
    print_char(' ');

    print_int(forward(numbers, 5));
    print_char(' ');

    // A global array through the same boundary.
    print_int(sum(numbers, 3));
    print_char('\n');

    return 0;
}
