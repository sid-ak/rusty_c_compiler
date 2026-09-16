// Parameter counts across the register boundary. On this ABI the first eight arguments arrive in
// registers and the ninth onward arrive on the stack, so eight and nine are two different pieces of
// code rather than one piece with a bigger number.
// expect-exit: 0
// expect-stdout: 28 36 45 285 78 111\n

void print_int(int n);
void print_char(char c);

int seven(int a, int b, int c, int d, int e, int f, int g) {
    return a + b * 2 + c * 3 + d * 4 + e * 5 + f * 6 + g * 7;
}

int eight(int a, int b, int c, int d, int e, int f, int g, int h) {
    return a + b * 2 + c * 3 + d * 4 + e * 5 + f * 6 + g * 7 + h * 8;
}

int nine(int a, int b, int c, int d, int e, int f, int g, int h, int i) {
    return a + b * 2 + c * 3 + d * 4 + e * 5 + f * 6 + g * 7 + h * 8 + i * 9;
}

int twelve(int a, int b, int c, int d, int e, int f, int g, int h, int i, int j, int k, int m) {
    return a + b + c + d + e + f + g + h + i + j + k + m;
}

// A `char` after the boundary, which is packed at its own size rather than given a full slot.
int mixed(int a, int b, int c, int d, int e, int f, int g, int h, char i, int j) {
    return a + b + c + d + e + f + g + h + i + j;
}

int main(void) {
    print_int(seven(1, 1, 1, 1, 1, 1, 1));
    print_char(' ');
    print_int(eight(1, 1, 1, 1, 1, 1, 1, 1));
    print_char(' ');
    print_int(nine(1, 1, 1, 1, 1, 1, 1, 1, 1));
    print_char(' ');

    // Weighted so an argument delivered to the wrong position changes the total.
    print_int(nine(1, 2, 3, 4, 5, 6, 7, 8, 9));
    print_char(' ');
    print_int(twelve(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12));
    print_char(' ');
    print_int(mixed(1, 2, 3, 4, 5, 6, 7, 8, 'A', 10));
    print_char('\n');

    return 0;
}
