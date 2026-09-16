// Comparing characters, which is comparing their promoted values. A char is signed here, so a byte
// above 127 is negative and compares below every ordinary letter.
// clang-warns: -Wconstant-conversion
// expect-exit: 0
// expect-stdout: 1011 111000 110110 111 111\n

void print_int(int n);
void print_char(char c);

int is_digit(char c) {
    return c >= '0' && c <= '9';
}

int is_lower(char c) {
    return c >= 'a' && c <= 'z';
}

int is_upper(char c) {
    return c >= 'A' && c <= 'Z';
}

int main(void) {
    print_int('a' < 'b');
    print_int('b' < 'a');
    print_int('a' == 'a');
    print_int('A' < 'a');
    print_char(' ');

    // Ranges, which is what character comparison is actually used for.
    print_int(is_digit('0'));
    print_int(is_digit('9'));
    print_int(is_digit('5'));
    print_int(is_digit('a'));
    print_int(is_digit('/'));
    print_int(is_digit(':'));
    print_char(' ');

    print_int(is_lower('a'));
    print_int(is_lower('z'));
    print_int(is_lower('A'));
    print_int(is_upper('A'));
    print_int(is_upper('Z'));
    print_int(is_upper('a'));
    print_char(' ');

    // A char held in a variable rather than written as a literal, so the comparison reads it back
    // out of one byte of storage first.
    char c = 'm';
    print_int(c > 'a');
    print_int(c < 'z');
    print_int(c == 'm');
    print_char(' ');

    // Signedness: a byte with the high bit set is negative, so it is below every letter.
    char high = 200;
    print_int(high < 'a');
    print_int(high < 0);
    print_int(high == -56);
    print_char('\n');

    return 0;
}
