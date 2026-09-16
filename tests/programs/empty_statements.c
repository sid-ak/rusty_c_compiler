// The statement forms that do nothing: an empty statement, an empty block, and a loop whose entire
// body is one of those.
// clang-warns: -Wunused-value
// expect-exit: 0
// expect-stdout: 0 0 5 0 0\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int n = 0;

    ;
    ;;
    {
    }

    print_int(n);
    print_char(' ');

    // A loop that does all its work in its own clauses.
    int total = 0;
    for (int i = 0; i < 5; i = i + 1)
        ;
    for (int i = 0; i < 5; i = i + 1) {
    }
    print_int(total);
    print_char(' ');

    // A `while` whose body is empty, with the condition doing the advancing.
    int k = 0;
    while (k++ < 4)
        ;
    print_int(k);
    print_char(' ');

    // An empty statement as the only arm of an `if`, which is legal and means "do nothing here".
    if (n == 0)
        ;
    else
        n = 1;
    print_int(n);
    print_char(' ');

    // An expression statement whose value is thrown away.
    n + 1;
    print_int(n);
    print_char('\n');

    return 0;
}
