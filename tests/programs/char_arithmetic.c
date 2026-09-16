// `char` in arithmetic. Storage is one byte and computation is 32-bit, so a character is promoted
// to an int the moment it takes part in an expression and the result is an int, not a char.
// clang-warns: -Wconstant-conversion
// expect-exit: 0
// expect-stdout: 9712248 259 194197 -59-56-1 abcde5 100-94\n

void print_int(int n);
void print_char(char c);

int main(void) {
    char a = 'a';
    char z = 'z';
    char zero = '0';

    print_int(a);
    print_int(z);
    print_int(zero);
    print_char(' ');

    // The difference of two characters is an ordinary int.
    print_int(z - a);
    print_int('9' - zero);
    print_char(' ');

    // Arithmetic that would not fit in a byte, which is why the result is not one.
    print_int(a * 2);
    print_int(a + 100);
    print_char(' ');

    // Assigning the result back to a char truncates to the low byte, and a char is signed here.
    char wrapped = a + 100;
    print_int(wrapped);
    char high = 200;
    print_int(high);
    char negative = -1;
    print_int(negative);
    print_char(' ');

    // A char as a loop counter, walking a contiguous range.
    int letters = 0;
    for (char c = 'a'; c <= 'e'; c = c + 1) {
        letters = letters + 1;
        print_char(c);
    }
    print_int(letters);
    print_char(' ');

    // Mixed with an int, where the char is the one that gets promoted.
    int n = 3;
    print_int(a + n);
    print_int(n - a);
    print_char('\n');

    return 0;
}
