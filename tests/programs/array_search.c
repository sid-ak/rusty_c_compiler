// Searching an array: finding something, finding nothing, and the boundary cases at each end, which
// is where a loop bound that is off by one stops giving the right answer.
// expect-exit: 0
// expect-stdout: 062 -1-1 12 10 -12\n

void print_int(int n);
void print_char(char c);

int index_of(int values[], int count, int target) {
    for (int i = 0; i < count; i = i + 1) {
        if (values[i] == target) {
            return i;
        }
    }
    return -1;
}

int count_of(int values[], int count, int target) {
    int seen = 0;
    for (int i = 0; i < count; i = i + 1) {
        if (values[i] == target) {
            seen = seen + 1;
        }
    }
    return seen;
}

int contains(int values[], int count, int target) {
    return index_of(values, count, target) >= 0;
}

int main(void) {
    int values[7] = {4, 8, 15, 8, 16, 23, 42};

    print_int(index_of(values, 7, 4));
    print_int(index_of(values, 7, 42));
    print_int(index_of(values, 7, 15));
    print_char(' ');

    // Absent, and absent-but-adjacent to something present.
    print_int(index_of(values, 7, 99));
    print_int(index_of(values, 7, 41));
    print_char(' ');

    // A duplicate: found once, at the first position.
    print_int(index_of(values, 7, 8));
    print_int(count_of(values, 7, 8));
    print_char(' ');

    print_int(contains(values, 7, 23));
    print_int(contains(values, 7, 24));
    print_char(' ');

    // A shortened view of the same array, so the bound rather than the contents decides the answer.
    print_int(index_of(values, 2, 15));
    print_int(index_of(values, 3, 15));
    print_char('\n');

    return 0;
}
