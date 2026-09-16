// Initializer lists: exactly as many values as the array holds, fewer than it holds — where the
// rest are zero — and none at all for a global, which is zero throughout.
// expect-exit: 0
// expect-stdout: 10 20 30 40 1200 0000 abc 789 700 xy00 5 10 6 6\n

void print_int(int n);
void print_char(char c);

int global_full[4] = {10, 20, 30, 40};
int global_short[4] = {1, 2};
int global_empty[4];
char global_chars[3] = {'a', 'b', 'c'};

int main(void) {
    for (int i = 0; i < 4; i = i + 1) {
        print_int(global_full[i]);
        print_char(' ');
    }

    for (int i = 0; i < 4; i = i + 1) {
        print_int(global_short[i]);
    }
    print_char(' ');

    for (int i = 0; i < 4; i = i + 1) {
        print_int(global_empty[i]);
    }
    print_char(' ');

    for (int i = 0; i < 3; i = i + 1) {
        print_char(global_chars[i]);
    }
    print_char(' ');

    // The same three shapes as locals, which are laid out on the stack rather than in a data
    // section, so a partial initializer has to zero the rest itself.
    int full[3] = {7, 8, 9};
    int short_list[3] = {7};
    char letters[4] = {'x', 'y'};

    for (int i = 0; i < 3; i = i + 1) {
        print_int(full[i]);
    }
    print_char(' ');
    for (int i = 0; i < 3; i = i + 1) {
        print_int(short_list[i]);
    }
    print_char(' ');
    print_char(letters[0]);
    print_char(letters[1]);
    print_int(letters[2]);
    print_int(letters[3]);
    print_char(' ');

    // Initializers that are expressions rather than literals.
    int n = 5;
    int computed[3] = {n, n * 2, n + 1};
    for (int i = 0; i < 3; i = i + 1) {
        print_int(computed[i]);
        print_char(' ');
    }

    // A one-element array, which is the smallest an array is allowed to be.
    int single[1] = {6};
    print_int(single[0]);
    print_char('\n');

    return 0;
}
