// Where `continue` goes in a `for` loop: to the step clause, not to the condition. Sending it to
// the condition compiles and assembles perfectly well, and produces a loop that never advances — so
// this program only terminates if the step runs.
// expect-exit: 0
// expect-stdout: 9 50 24 9\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int visited = 0;
    for (int i = 0; i < 6; i = i + 1) {
        if (i % 2 == 0) {
            continue;
        }
        visited = visited + i;
    }
    print_int(visited);
    print_char(' ');

    // Every iteration continues, so the body after it never runs and only the step advances.
    int never = 0;
    int passes = 0;
    for (int i = 0; i < 5; i = i + 1) {
        passes = passes + 1;
        continue;
    }
    print_int(passes);
    print_int(never);
    print_char(' ');

    // A step with a side effect of its own, so what the step did is visible after the loop.
    int counter = 0;
    for (int i = 0; i < 4; i = i + 1) {
        counter = counter + 1;
        if (i < 2) {
            continue;
        }
        counter = counter + 10;
    }
    print_int(counter);
    print_char(' ');

    // In a `while`, `continue` goes to the condition, which is why the advance has to come before
    // it rather than after.
    int k = 0;
    int seen = 0;
    while (k < 6) {
        k = k + 1;
        if (k % 3 != 0) {
            continue;
        }
        seen = seen + k;
    }
    print_int(seen);
    print_char('\n');

    return 0;
}
