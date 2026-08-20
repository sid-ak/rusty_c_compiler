// Strings and characters: string literals as arguments, a `char` array initialized from one, and
// every escape sequence the subset decodes.

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

int main(void) {
    print_string("hello, world");
    print_char('\n');

    // The empty literal is still a valid, null-terminated string.
    print_string("");

    // Escapes are decoded once, by the lexer, so what reaches the program is real bytes.
    print_string("tab:\there");
    print_char('\n');
    print_string("quote:\" backslash:\\ single:'");
    print_char('\n');

    print_char('a');
    print_char('Z');
    print_char('0');
    print_char(' ');
    print_char('\t');
    print_char('\n');

    // Character literals promote to int, so they can be compared and counted like any other.
    print_int('A');
    print_int('a' - 'A');
    print_int('z' > 'a');

    // A char array initialized from a string literal, which is the one place a literal is not an
    // argument.
    char greeting[6] = "hello";
    print_string(greeting);
    print_char('\n');
    print_int(greeting[0]);
    print_int(greeting[4]);

    print_int(length("counted"));
    print_int(length(greeting));
    print_int(length(""));

    // Reading a string a character at a time, which is what makes the null terminator visible.
    char word[4] = "abc";
    for (int i = 0; i < 3; i = i + 1) {
        print_char(word[i]);
    }
    print_int(word[3]);
    print_char('\n');

    return 0;
}
