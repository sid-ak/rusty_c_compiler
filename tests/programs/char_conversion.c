// Converting between characters and the numbers behind them: case changes by a fixed offset, and a
// digit character becomes its value by subtracting '0'.
// expect-exit: 0
// expect-stdout: ABCXY abcxy 77  4092 0123456789 1\n

void print_int(int n);
void print_char(char c);
void print_string(char s[]);

char to_upper(char c) {
    if (c >= 'a' && c <= 'z') {
        return c - 32;
    }
    return c;
}

char to_lower(char c) {
    if (c >= 'A' && c <= 'Z') {
        return c + 32;
    }
    return c;
}

int digit_value(char c) {
    return c - '0';
}

char digit_char(int value) {
    return value + '0';
}

int main(void) {
    char word[6] = "abcXy";

    for (int i = 0; i < 5; i = i + 1) {
        print_char(to_upper(word[i]));
    }
    print_char(' ');
    for (int i = 0; i < 5; i = i + 1) {
        print_char(to_lower(word[i]));
    }
    print_char(' ');

    // Characters outside the letters are left alone.
    print_char(to_upper('7'));
    print_char(to_lower('7'));
    print_char(to_upper(' '));
    print_char(' ');

    // Digits, both directions.
    int total = 0;
    char digits[5] = "4092";
    for (int i = 0; i < 4; i = i + 1) {
        total = total * 10 + digit_value(digits[i]);
    }
    print_int(total);
    print_char(' ');

    for (int value = 0; value < 10; value = value + 1) {
        print_char(digit_char(value));
    }
    print_char(' ');

    // Round trip: every letter survives going up and back down.
    int same = 1;
    for (char c = 'a'; c <= 'z'; c = c + 1) {
        if (to_lower(to_upper(c)) != c) {
            same = 0;
        }
    }
    print_int(same);
    print_char('\n');

    return 0;
}
