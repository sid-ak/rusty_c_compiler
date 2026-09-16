// Blocks inside blocks, and the shadowing that comes with them: an inner declaration hides the
// outer one for as long as the inner block lasts, and the outer one is unchanged afterwards.
// expect-exit: 0
// expect-stdout: 12321 1510 01299 111\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int n = 1;
    print_int(n);

    {
        int n = 2;
        print_int(n);

        {
            int n = 3;
            print_int(n);
        }

        print_int(n);
    }

    print_int(n);
    print_char(' ');

    // A shadowing declaration initialized from the variable it hides. The outer `value` is read
    // first, into a temporary, so the inner one starts from it rather than from itself.
    int value = 10;
    {
        int outer = value;
        int value = outer + 5;
        print_int(value);
    }
    print_int(value);
    print_char(' ');

    // A block introduced for its own sake, with no statement of its own.
    {
    }
    {
        {
        }
    }

    // The loop variable of a `for` is scoped to the loop, so the outer one survives it.
    int i = 99;
    for (int i = 0; i < 3; i = i + 1) {
        print_int(i);
    }
    print_int(i);
    print_char(' ');

    // A variable declared inside a loop body is fresh on every pass.
    for (int pass = 0; pass < 3; pass = pass + 1) {
        int fresh = 0;
        fresh = fresh + 1;
        print_int(fresh);
    }
    print_char('\n');

    return 0;
}
