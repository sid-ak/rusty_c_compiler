// Recursion and arrays together: an array walked by recursion rather than by a loop, and an array
// filled by a function that calls itself, so the same storage is reached from every frame.
// expect-exit: 0
// expect-stdout: 269 0 1 3 6 10 15 21 0 10 20 30 40 1412\n

void print_int(int n);
void print_char(char c);

int sum_from(int values[], int count, int index) {
    if (index >= count) {
        return 0;
    }
    return values[index] + sum_from(values, count, index + 1);
}

int max_from(int values[], int count, int index) {
    if (index == count - 1) {
        return values[index];
    }
    int rest = max_from(values, count, index + 1);
    if (values[index] > rest) {
        return values[index];
    }
    return rest;
}

void fill_triangular(int values[], int count, int index) {
    if (index >= count) {
        return;
    }
    if (index == 0) {
        values[0] = 0;
    } else {
        values[index] = values[index - 1] + index;
    }
    fill_triangular(values, count, index + 1);
}

// Recursion that writes as it unwinds rather than as it descends.
void fill_backwards(int values[], int count, int index) {
    if (index >= count) {
        return;
    }
    fill_backwards(values, count, index + 1);
    values[index] = index * 10;
}

int main(void) {
    int values[6] = {3, 9, 2, 7, 4, 1};

    print_int(sum_from(values, 6, 0));
    print_int(max_from(values, 6, 0));
    print_char(' ');

    int triangular[7];
    fill_triangular(triangular, 7, 0);
    for (int i = 0; i < 7; i = i + 1) {
        print_int(triangular[i]);
        print_char(' ');
    }

    int backwards[5];
    fill_backwards(backwards, 5, 0);
    for (int i = 0; i < 5; i = i + 1) {
        print_int(backwards[i]);
        print_char(' ');
    }

    // Recursion over a slice of the same array.
    print_int(sum_from(values, 3, 0));
    print_int(sum_from(values, 6, 3));
    print_char('\n');

    return 0;
}
