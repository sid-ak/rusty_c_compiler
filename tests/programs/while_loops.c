// `while`: a loop that runs, a loop whose condition is false before the first pass, and the two
// ways out of one other than the condition going false.
// expect-exit: 0
// expect-stdout: 105 7 4 25 0\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int i = 0;
    int sum = 0;
    while (i < 5) {
        sum = sum + i;
        i = i + 1;
    }
    print_int(sum);
    print_int(i);
    print_char(' ');

    // Zero iterations: the body never runs and the variable it would have changed proves it.
    int untouched = 7;
    while (0) {
        untouched = 0;
    }
    print_int(untouched);
    print_char(' ');

    // `break` leaves immediately, before the rest of the body.
    int n = 0;
    while (1) {
        n = n + 1;
        if (n == 4) {
            break;
        }
        n = n + 10;
    }
    print_int(n);
    print_char(' ');

    // `continue` returns to the condition, skipping the rest of the body.
    int odd = 0;
    int k = 0;
    while (k < 10) {
        k = k + 1;
        if (k % 2 == 0) {
            continue;
        }
        odd = odd + k;
    }
    print_int(odd);
    print_char(' ');

    // An unbraced body, which is one statement rather than a block.
    int countdown = 3;
    while (countdown > 0)
        countdown = countdown - 1;
    print_int(countdown);
    print_char('\n');

    return 0;
}
