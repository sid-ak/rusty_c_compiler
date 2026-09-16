// Arrays worked on in place across a function boundary: reversed, sorted, and searched. Every
// helper receives the array as a decayed pointer and mutates the caller's storage, which is the
// only thing an array is allowed to become in this subset.
// expect-exit: 0
// expect-stdout: 5 3 9 1 8 2 \n2 8 1 9 3 5 \n1 2 3 5 8 9 \n1 2 3 5 8 9 \n4-1\n5 4 3 2 1 \n

void print_int(int n);
void print_char(char c);

void reverse(int values[], int count) {
    int left = 0;
    int right = count - 1;
    while (left < right) {
        int held = values[left];
        values[left] = values[right];
        values[right] = held;
        left = left + 1;
        right = right - 1;
    }
}

// Bubble sort: the simplest sort with a nested loop and a swap, which is what makes it worth having
// here rather than a faster one.
void sort(int values[], int count) {
    for (int pass = 0; pass < count - 1; pass = pass + 1) {
        for (int i = 0; i < count - 1 - pass; i = i + 1) {
            if (values[i] > values[i + 1]) {
                int held = values[i];
                values[i] = values[i + 1];
                values[i + 1] = held;
            }
        }
    }
}

int index_of(int values[], int count, int wanted) {
    for (int i = 0; i < count; i = i + 1) {
        if (values[i] == wanted) {
            return i;
        }
    }
    return -1;
}

void show(int values[], int count) {
    for (int i = 0; i < count; i = i + 1) {
        print_int(values[i]);
        print_char(' ');
    }
    print_char('\n');
}

int main(void) {
    int numbers[6] = {5, 3, 9, 1, 8, 2};
    show(numbers, 6);

    reverse(numbers, 6);
    show(numbers, 6);

    sort(numbers, 6);
    show(numbers, 6);

    // Reversing twice returns the original order, which a one-sided swap would not.
    reverse(numbers, 6);
    reverse(numbers, 6);
    show(numbers, 6);

    print_int(index_of(numbers, 6, 8));
    print_int(index_of(numbers, 6, 4));
    print_char('\n');

    // An odd length, so the middle element stays put rather than being swapped with itself.
    int odd[5] = {1, 2, 3, 4, 5};
    reverse(odd, 5);
    show(odd, 5);

    return 0;
}
