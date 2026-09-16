// Two classic numeric routines, each written twice — once with a loop and once with recursion — so
// the two forms can be held to the same answers rather than each to its own.
// expect-exit: 0
// expect-stdout: 66 12020 10241024 115 11\n

void print_int(int n);
void print_char(char c);

int gcd_loop(int a, int b) {
    while (b != 0) {
        int remainder = a % b;
        a = b;
        b = remainder;
    }
    return a;
}

int gcd_recursive(int a, int b) {
    if (b == 0) {
        return a;
    }
    return gcd_recursive(b, a % b);
}

int power_loop(int base, int exponent) {
    int result = 1;
    for (int i = 0; i < exponent; i = i + 1) {
        result = result * base;
    }
    return result;
}

int power_recursive(int base, int exponent) {
    if (exponent == 0) {
        return 1;
    }
    if (exponent % 2 == 0) {
        int half = power_recursive(base, exponent / 2);
        return half * half;
    }
    return base * power_recursive(base, exponent - 1);
}

int main(void) {
    print_int(gcd_loop(48, 18));
    print_int(gcd_recursive(48, 18));
    print_char(' ');
    print_int(gcd_loop(17, 5));
    print_int(gcd_loop(20, 0));
    print_int(gcd_loop(0, 20));
    print_char(' ');

    print_int(power_loop(2, 10));
    print_int(power_recursive(2, 10));
    print_char(' ');
    print_int(power_loop(3, 0));
    print_int(power_recursive(3, 0));
    print_int(power_recursive(5, 1));
    print_char(' ');

    // The loop and the recursion agree everywhere in range, which neither proves on its own.
    int agree = 1;
    for (int a = 1; a <= 30; a = a + 1) {
        for (int b = 0; b <= 30; b = b + 1) {
            if (gcd_loop(a, b) != gcd_recursive(a, b)) {
                agree = 0;
            }
        }
    }
    print_int(agree);

    int powers_agree = 1;
    for (int exponent = 0; exponent <= 9; exponent = exponent + 1) {
        if (power_loop(2, exponent) != power_recursive(2, exponent)) {
            powers_agree = 0;
        }
    }
    print_int(powers_agree);
    print_char('\n');

    return 0;
}
