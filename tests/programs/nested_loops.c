// Loops inside loops, and what `break` and `continue` do inside them: each applies to the innermost
// loop containing it, never to the one outside.
// expect-exit: 0
// expect-stdout: 12 36 6 6 3\n

void print_int(int n);
void print_char(char c);

int main(void) {
    // A full pass: three rows of four.
    int cells = 0;
    for (int row = 0; row < 3; row = row + 1) {
        for (int column = 0; column < 4; column = column + 1) {
            cells = cells + 1;
        }
    }
    print_int(cells);
    print_char(' ');

    // `break` leaves the inner loop only, so the outer one still runs every time.
    int inner = 0;
    int outer = 0;
    for (int row = 0; row < 3; row = row + 1) {
        outer = outer + 1;
        for (int column = 0; column < 4; column = column + 1) {
            if (column == 2) {
                break;
            }
            inner = inner + 1;
        }
    }
    print_int(outer);
    print_int(inner);
    print_char(' ');

    // `continue` in the inner loop skips the rest of the inner body only.
    int counted = 0;
    for (int row = 0; row < 3; row = row + 1) {
        for (int column = 0; column < 4; column = column + 1) {
            if (column % 2 == 0) {
                continue;
            }
            counted = counted + 1;
        }
    }
    print_int(counted);
    print_char(' ');

    // `continue` in the outer loop, with an inner loop after it that is therefore skipped.
    int skipped = 0;
    for (int row = 0; row < 4; row = row + 1) {
        if (row == 1) {
            continue;
        }
        for (int column = 0; column < 2; column = column + 1) {
            skipped = skipped + 1;
        }
    }
    print_int(skipped);
    print_char(' ');

    // A `while` inside a `for`, so the two loop forms are nested in each other rather than only in
    // themselves.
    int mixed = 0;
    for (int row = 0; row < 3; row = row + 1) {
        int column = 0;
        while (column < row) {
            mixed = mixed + 1;
            column = column + 1;
        }
    }
    print_int(mixed);
    print_char('\n');

    return 0;
}
