// What `main` returns and what the operating system reports. A process exit status is the low eight
// bits of that value, so 300 and 44 are the same status by the time anyone can observe it — which
// is why test expectations are written against the masked number.
// expect-exit: 44
// expect-stdout: 300\n

void print_int(int n);
void print_char(char c);

int computed(void) {
    return 100 * 3;
}

int main(void) {
    print_int(computed());
    print_char('\n');

    return computed();
}
