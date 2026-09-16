// Declaring an array, writing to it by index, and reading it back — including with an index that is
// itself a computed expression rather than a literal.
// expect-exit: 0
// expect-stdout: 0 1 4 9 16 016 449 49916 5 32\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int slots[5];

    for (int i = 0; i < 5; i = i + 1) {
        slots[i] = i * i;
    }
    for (int i = 0; i < 5; i = i + 1) {
        print_int(slots[i]);
        print_char(' ');
    }

    // A literal index, the first and last elements, where an off-by-one in the address arithmetic
    // shows up as the wrong element rather than as a crash.
    print_int(slots[0]);
    print_int(slots[4]);
    print_char(' ');

    // A computed index.
    int k = 1;
    print_int(slots[k + 1]);
    print_int(slots[2 * k]);
    print_int(slots[4 - k]);
    print_char(' ');

    // Writing through a computed index, and reading the neighbours to show nothing else moved.
    slots[k + 2] = 99;
    print_int(slots[2]);
    print_int(slots[3]);
    print_int(slots[4]);
    print_char(' ');

    // An element as an operand, on both sides of an assignment.
    slots[0] = slots[1] + slots[2];
    print_int(slots[0]);
    print_char(' ');

    // Increment applied to an element.
    slots[1]++;
    ++slots[1];
    print_int(slots[1]);
    slots[1]--;
    print_int(slots[1]);
    print_char('\n');

    return 0;
}
