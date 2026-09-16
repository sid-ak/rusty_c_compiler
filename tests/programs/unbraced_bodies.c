// Single-statement bodies without braces, on every construct that takes one, since an unbraced body
// is a different parse rather than a formatting choice.
// expect-exit: 0
// expect-stdout: 13 46 6 5 2\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int n = 0;

    if (1)
        n = n + 1;
    print_int(n);

    if (0)
        n = n + 10;
    else
        n = n + 2;
    print_int(n);
    print_char(' ');

    int i = 0;
    while (i < 4)
        i = i + 1;
    print_int(i);

    int sum = 0;
    for (int k = 0; k < 4; k = k + 1)
        sum = sum + k;
    print_int(sum);
    print_char(' ');

    // An unbraced body that is itself a loop, so two constructs nest with no braces anywhere.
    int cells = 0;
    for (int row = 0; row < 3; row = row + 1)
        for (int column = 0; column < 2; column = column + 1)
            cells = cells + 1;
    print_int(cells);
    print_char(' ');

    // An unbraced body that is a block, which is the same statement written the usual way.
    int braced = 0;
    if (1) {
        braced = 5;
    }
    print_int(braced);
    print_char(' ');

    // `break` and `continue` as unbraced bodies.
    int stopped = 0;
    for (int k = 0; k < 10; k = k + 1) {
        if (k == 3)
            break;
        stopped = k;
    }
    print_int(stopped);
    print_char('\n');

    return 0;
}
