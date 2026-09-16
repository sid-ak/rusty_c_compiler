// Finding the smallest and largest element, which is a loop carrying state that only sometimes
// updates — the branch inside the loop is taken on some passes and not on others depending on the
// data, rather than on the loop counter.
// expect-exit: 0
// expect-stdout: 1192 1177 -8-1 4242\n

void print_int(int n);
void print_char(char c);

int smallest(int values[], int count) {
    int best = values[0];
    for (int i = 1; i < count; i = i + 1) {
        if (values[i] < best) {
            best = values[i];
        }
    }
    return best;
}

int largest(int values[], int count) {
    int best = values[0];
    for (int i = 1; i < count; i = i + 1) {
        if (values[i] > best) {
            best = values[i];
        }
    }
    return best;
}

int index_of_largest(int values[], int count) {
    int best = 0;
    for (int i = 1; i < count; i = i + 1) {
        if (values[i] > values[best]) {
            best = i;
        }
    }
    return best;
}

int main(void) {
    int values[7] = {12, 4, 19, 4, 7, 19, 1};
    print_int(smallest(values, 7));
    print_int(largest(values, 7));
    print_int(index_of_largest(values, 7));
    print_char(' ');

    // The extreme at the front, where the loop never updates, and at the back, where it updates on
    // the last pass.
    int front[4] = {1, 5, 6, 7};
    int back[4] = {7, 6, 5, 1};
    print_int(smallest(front, 4));
    print_int(smallest(back, 4));
    print_int(largest(front, 4));
    print_int(largest(back, 4));
    print_char(' ');

    // Negatives, where a zero-initialized accumulator would give the wrong answer.
    int negatives[4] = {-3, -8, -1, -5};
    print_int(smallest(negatives, 4));
    print_int(largest(negatives, 4));
    print_char(' ');

    // One element, where the loop body never runs.
    int single[1] = {42};
    print_int(smallest(single, 1));
    print_int(largest(single, 1));
    print_char('\n');

    return 0;
}
