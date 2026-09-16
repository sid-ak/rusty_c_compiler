// Reversing an array in place, which needs two indices, a swap through a temporary, and a loop that
// stops in the middle rather than at the end.
// expect-exit: 0
// expect-stdout: 654321 54321 12345 9 21 21 \n

void print_int(int n);
void print_char(char c);

void swap(int values[], int a, int b) {
    int held = values[a];
    values[a] = values[b];
    values[b] = held;
}

void reverse(int values[], int count) {
    int low = 0;
    int high = count - 1;
    while (low < high) {
        swap(values, low, high);
        low = low + 1;
        high = high - 1;
    }
}

void show(int values[], int count) {
    for (int i = 0; i < count; i = i + 1) {
        print_int(values[i]);
    }
    print_char(' ');
}

int main(void) {
    int even[6] = {1, 2, 3, 4, 5, 6};
    reverse(even, 6);
    show(even, 6);

    // An odd length, where the middle element stays where it is.
    int odd[5] = {1, 2, 3, 4, 5};
    reverse(odd, 5);
    show(odd, 5);

    // Reversing twice gives back what was there to begin with.
    reverse(odd, 5);
    show(odd, 5);

    // The degenerate lengths, where the loop body must not run at all.
    int single[1] = {9};
    reverse(single, 1);
    show(single, 1);

    int pair[2] = {1, 2};
    reverse(pair, 2);
    show(pair, 2);

    // Swapping an element with itself, which has to leave it alone rather than zero it.
    swap(pair, 0, 0);
    show(pair, 2);
    print_char('\n');

    return 0;
}
