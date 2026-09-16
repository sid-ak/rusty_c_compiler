// Functions that return nothing: called as statements, returning early with a bare `return`, and
// falling off the end, which is the one place that is allowed.
// expect-exit: 0
// expect-stdout: 2 12 171717 17 20\n

void print_int(int n);
void print_char(char c);

int side_effects = 0;

void bump(void) {
    side_effects = side_effects + 1;
}

void bump_by(int n) {
    side_effects = side_effects + n;
}

void only_if_positive(int n) {
    if (n <= 0) {
        return;
    }
    side_effects = side_effects + n;
}

void nothing(void) {
}

int main(void) {
    bump();
    bump();
    print_int(side_effects);
    print_char(' ');

    bump_by(10);
    print_int(side_effects);
    print_char(' ');

    only_if_positive(5);
    print_int(side_effects);
    only_if_positive(-5);
    print_int(side_effects);
    only_if_positive(0);
    print_int(side_effects);
    print_char(' ');

    nothing();
    print_int(side_effects);
    print_char(' ');

    // Called from inside a loop, so the sequencing of a void call is exercised more than once.
    for (int i = 0; i < 3; i = i + 1) {
        bump();
    }
    print_int(side_effects);
    print_char('\n');

    return 0;
}
