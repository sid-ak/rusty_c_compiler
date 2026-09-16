// The wrong half of the harness's fault-injection test: the same program with a different answer.
// Compared against prints_one.c, the harness has to report a mismatch; a harness that cannot fail
// proves nothing about the runs it passes.

void print_string(char s[]);

int main(void) {
    print_string("two");
    return 0;
}
