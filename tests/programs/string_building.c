// Building strings rather than only reading them: copying one array into another, appending, and
// writing the null terminator by hand, which is the part a reader never sees and a bug always
// finds.
// expect-exit: 0
// expect-stdout: 5hello 12hello, world 2hi2 00 *****5 fedcba\n

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

int copy_into(char destination[], char source[]) {
    int i = 0;
    while (source[i] != 0) {
        destination[i] = source[i];
        i = i + 1;
    }
    destination[i] = 0;
    return i;
}

int append(char destination[], char source[]) {
    int end = length(destination);
    int i = 0;
    while (source[i] != 0) {
        destination[end + i] = source[i];
        i = i + 1;
    }
    destination[end + i] = 0;
    return end + i;
}

void fill(char buffer[], int count, char c) {
    for (int i = 0; i < count; i = i + 1) {
        buffer[i] = c;
    }
    buffer[count] = 0;
}

int main(void) {
    char buffer[32];

    print_int(copy_into(buffer, "hello"));
    print_string(buffer);
    print_char(' ');

    print_int(append(buffer, ", world"));
    print_string(buffer);
    print_char(' ');

    // Copying over a longer string, where the terminator has to move back rather than stay put.
    print_int(copy_into(buffer, "hi"));
    print_string(buffer);
    print_int(length(buffer));
    print_char(' ');

    // Copying the empty string, which writes nothing but the terminator.
    print_int(copy_into(buffer, ""));
    print_int(length(buffer));
    print_char(' ');

    fill(buffer, 5, '*');
    print_string(buffer);
    print_int(length(buffer));
    print_char(' ');

    // Reversing in place through a temporary, which needs both ends of the string at once.
    copy_into(buffer, "abcdef");
    int last = length(buffer) - 1;
    for (int i = 0; i < last - i; i = i + 1) {
        char held = buffer[i];
        buffer[i] = buffer[last - i];
        buffer[last - i] = held;
    }
    print_string(buffer);
    print_char('\n');

    return 0;
}
