// Binary search, written both as a loop and as a recursion over the same array, so the two forms
// can be checked against each other on every element and on the gaps between them.
// expect-exit: 0
// expect-stdout: 012345678 1 -1-1-1 0-1-1\n

void print_int(int n);
void print_char(char c);

int search_loop(int values[], int count, int target) {
    int low = 0;
    int high = count - 1;
    while (low <= high) {
        int middle = low + (high - low) / 2;
        if (values[middle] == target) {
            return middle;
        }
        if (values[middle] < target) {
            low = middle + 1;
        } else {
            high = middle - 1;
        }
    }
    return -1;
}

int search_between(int values[], int low, int high, int target) {
    if (low > high) {
        return -1;
    }
    int middle = low + (high - low) / 2;
    if (values[middle] == target) {
        return middle;
    }
    if (values[middle] < target) {
        return search_between(values, middle + 1, high, target);
    }
    return search_between(values, low, middle - 1, target);
}

int main(void) {
    int values[9] = {1, 3, 5, 7, 9, 11, 13, 15, 17};

    // Every element is found, at its own index.
    for (int i = 0; i < 9; i = i + 1) {
        print_int(search_loop(values, 9, values[i]));
    }
    print_char(' ');

    // The two implementations agree everywhere, present or absent.
    int agree = 1;
    for (int target = 0; target < 20; target = target + 1) {
        if (search_loop(values, 9, target) != search_between(values, 0, 8, target)) {
            agree = 0;
        }
    }
    print_int(agree);
    print_char(' ');

    // Below the first element, above the last, and in a gap.
    print_int(search_loop(values, 9, 0));
    print_int(search_loop(values, 9, 18));
    print_int(search_loop(values, 9, 8));
    print_char(' ');

    // A single-element array, and the empty search that a zero count implies.
    int single[1] = {4};
    print_int(search_loop(single, 1, 4));
    print_int(search_loop(single, 1, 5));
    print_int(search_loop(single, 0, 4));
    print_char('\n');

    return 0;
}
