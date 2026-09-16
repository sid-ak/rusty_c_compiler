// String literals: printed directly, passed on through another function, repeated so the same text
// appears twice, and every escape the lexer decodes.
// expect-exit: 0
// expect-stdout: plain||through a parameter|repeated|repeated|twice|twice| tab[\t]quote["]backslash[\\]single['] newline[\n] 120\n

void print_int(int n);
void print_char(char c);
void print_string(char s[]);

void announce(char text[]) {
    print_string(text);
    print_char('|');
}

void twice(char text[]) {
    announce(text);
    announce(text);
}

int main(void) {
    print_string("plain");
    print_char('|');

    // The empty literal is still a null-terminated string, so it prints nothing and does not hang.
    print_string("");
    print_char('|');

    announce("through a parameter");

    // The same literal twice, which the compiler is free to store once.
    announce("repeated");
    announce("repeated");
    twice("twice");
    print_char(' ');

    // Every escape in the subset, decoded once by the lexer so what reaches the program is bytes.
    print_string("tab[\t]");
    print_string("quote[\"]");
    print_string("backslash[\\]");
    print_string("single[']");
    print_char(' ');
    print_string("newline[");
    print_char('\n');
    print_string("]");
    print_char(' ');

    // A literal as the argument of a call whose result is used, rather than as a statement.
    print_int('x');
    print_char('\n');

    return 0;
}
