// Global arrays: laid out in a data section rather than on a stack, initialized fully, partly, and
// not at all, and mutated from more than one function.
// expect-exit: 0
// expect-stdout: 15030 00000 12000 45030150 9 global0wxy0\n

void print_int(int n);
void print_char(char c);
void print_string(char s[]);

int full[5] = {10, 20, 30, 40, 50};
int partial[5] = {1, 2};
int blank[5];
char name[8] = "global";
char letters[4] = {'w', 'x', 'y'};

void scale(int factor) {
    for (int i = 0; i < 5; i = i + 1) {
        full[i] = full[i] * factor;
    }
}

int total(int values[], int count) {
    int sum = 0;
    for (int i = 0; i < count; i = i + 1) {
        sum = sum + values[i];
    }
    return sum;
}

int main(void) {
    print_int(total(full, 5));
    print_int(total(partial, 5));
    print_int(total(blank, 5));
    print_char(' ');

    // An uninitialized global array is zero throughout, not garbage.
    for (int i = 0; i < 5; i = i + 1) {
        print_int(blank[i]);
    }
    print_char(' ');

    // Partly initialized: the rest is zero.
    for (int i = 0; i < 5; i = i + 1) {
        print_int(partial[i]);
    }
    print_char(' ');

    scale(3);
    print_int(total(full, 5));
    print_int(full[0]);
    print_int(full[4]);
    print_char(' ');

    // Written directly as well as through a function.
    blank[2] = 9;
    print_int(total(blank, 5));
    print_char(' ');

    print_string(name);
    print_int(name[6]);
    print_char(letters[0]);
    print_char(letters[1]);
    print_char(letters[2]);
    print_int(letters[3]);
    print_char('\n');

    return 0;
}
