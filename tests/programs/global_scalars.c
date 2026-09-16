// Global variables: initialized and not, read and written from several functions, and shadowed by a
// local of the same name.
// expect-exit: 0
// expect-stdout: 070103 3 5051 10151 357h\n

void print_int(int n);
void print_char(char c);

int counter = 0;
int seeded = 7;
int never_set;
char letter = 'g';

void bump(void) {
    counter = counter + 1;
}

int read_counter(void) {
    return counter;
}

void set_counter(int n) {
    counter = n;
}

// A local of the same name hides the global for the length of the function.
int shadowed(void) {
    int counter = 100;
    counter = counter + 1;
    return counter;
}

int main(void) {
    print_int(counter);
    print_int(seeded);
    print_int(never_set);
    print_int(letter);
    print_char(' ');

    bump();
    bump();
    bump();
    print_int(read_counter());
    print_char(' ');

    set_counter(50);
    print_int(counter);
    bump();
    print_int(counter);
    print_char(' ');

    // The local does not disturb the global it hides.
    print_int(shadowed());
    print_int(counter);
    print_char(' ');

    // A global in an expression, and assigned from one.
    seeded = seeded * counter;
    print_int(seeded);
    letter = letter + 1;
    print_char(letter);
    print_char('\n');

    return 0;
}
