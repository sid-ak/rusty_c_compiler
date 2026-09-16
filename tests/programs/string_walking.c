// Walking a string to its null terminator, which is the only way to know how long one is. The
// terminator is a real byte in the array, so a loop that stops at it and a loop that counts past it
// disagree by one.
// expect-exit: 0
// expect-stdout: 501 50 320 20-1 desserts dlrow\n

void print_int(int n);
void print_char(char c);
void print_string(char s[]);

int length(char s[]) {
    int n = 0;
    while (s[n] != 0) {
        n = n + 1;
    }
    return n;
}

int count_of(char s[], char target) {
    int seen = 0;
    for (int i = 0; s[i] != 0; i = i + 1) {
        if (s[i] == target) {
            seen = seen + 1;
        }
    }
    return seen;
}

int index_of(char s[], char target) {
    for (int i = 0; s[i] != 0; i = i + 1) {
        if (s[i] == target) {
            return i;
        }
    }
    return -1;
}

void print_reversed(char s[]) {
    for (int i = length(s) - 1; i >= 0; i = i - 1) {
        print_char(s[i]);
    }
}

int main(void) {
    print_int(length("hello"));
    print_int(length(""));
    print_int(length("a"));
    print_char(' ');

    char stored[6] = "world";
    print_int(length(stored));
    print_int(stored[5]);
    print_char(' ');

    print_int(count_of("banana", 'a'));
    print_int(count_of("banana", 'n'));
    print_int(count_of("banana", 'z'));
    print_char(' ');

    print_int(index_of("banana", 'n'));
    print_int(index_of("banana", 'b'));
    print_int(index_of("banana", 'z'));
    print_char(' ');

    print_reversed("stressed");
    print_char(' ');
    print_reversed(stored);
    print_char('\n');

    return 0;
}
