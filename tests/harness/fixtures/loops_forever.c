// A program that never finishes. The harness has to kill it and report a timeout; without that it
// would hang the suite, which looks the same as a build that never started.

void print_string(char s[]);

int main(void) {
    int n = 0;
    print_string("starting");
    while (1) {
        n = n + 1;
        if (n > 1000000) {
            n = 0;
        }
    }
    return 0;
}
