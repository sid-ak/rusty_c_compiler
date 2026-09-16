// Assignment as an expression: it produces the value it stored, which is what makes a chain work
// and what lets one sit inside a condition.
// expect-exit: 0
// expect-stdout: 777 444 2015 30 18\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int a = 1;
    int b = 2;
    int c = 3;

    // The rightmost assignment happens first, and its value flows left.
    a = b = c = 7;
    print_int(a);
    print_int(b);
    print_int(c);
    print_char(' ');

    // The value of the whole chain is usable in place.
    print_int(a = b = 4);
    print_int(a);
    print_int(b);
    print_char(' ');

    // An assignment whose right-hand side reads the variable being assigned.
    int n = 10;
    n = n + n;
    print_int(n);
    n = n - 5;
    print_int(n);
    print_char(' ');

    // Assignment inside a condition, which is legal and means "store, then test".
    int value = 0;
    if ((value = 3)) {
        print_int(value);
    }
    if ((value = 0)) {
        print_char('?');
    }
    print_int(value);
    print_char(' ');

    // Into array elements, where the chain has to compute two addresses.
    int slots[3] = {0, 0, 0};
    slots[0] = slots[1] = slots[2] = 6;
    print_int(slots[0] + slots[1] + slots[2]);
    print_char('\n');

    return 0;
}
