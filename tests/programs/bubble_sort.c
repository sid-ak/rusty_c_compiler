// Bubble sort: nested loops whose inner bound depends on the outer index, a swap through a
// temporary, and a pass counter that stops early when nothing moved.
// expect-exit: 0
// expect-stdout: 1 2 3 4 5 7 8 9 1 1 2 3 4 5 1 2 3 4 5 -7 -1 0 3 3 3 \n

void print_int(int n);
void print_char(char c);

void sort(int values[], int count) {
    for (int pass = 0; pass < count - 1; pass = pass + 1) {
        int swapped = 0;
        for (int i = 0; i < count - 1 - pass; i = i + 1) {
            if (values[i] > values[i + 1]) {
                int held = values[i];
                values[i] = values[i + 1];
                values[i + 1] = held;
                swapped = 1;
            }
        }
        if (swapped == 0) {
            return;
        }
    }
}

void show(int values[], int count) {
    for (int i = 0; i < count; i = i + 1) {
        print_int(values[i]);
        print_char(' ');
    }
}

int sorted(int values[], int count) {
    for (int i = 1; i < count; i = i + 1) {
        if (values[i - 1] > values[i]) {
            return 0;
        }
    }
    return 1;
}

int main(void) {
    int scrambled[8] = {5, 2, 9, 1, 7, 3, 8, 4};
    sort(scrambled, 8);
    show(scrambled, 8);
    print_int(sorted(scrambled, 8));
    print_char(' ');

    // Already in order: the early exit fires on the first pass.
    int ordered[5] = {1, 2, 3, 4, 5};
    sort(ordered, 5);
    show(ordered, 5);

    // Exactly backwards, which is the most work the algorithm can do.
    int backwards[5] = {5, 4, 3, 2, 1};
    sort(backwards, 5);
    show(backwards, 5);

    // Duplicates and negatives.
    int mixed[6] = {3, -1, 3, 0, -7, 3};
    sort(mixed, 6);
    show(mixed, 6);
    print_char('\n');

    return 0;
}
