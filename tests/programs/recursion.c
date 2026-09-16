// Recursion in the shapes that stress a stack frame: two functions calling each other, a function
// calling itself twice per level, and one whose recursion is nested inside its own argument list.
// The last section combines recursion, arrays, and string output in one program.
// expect-exit: 0
// expect-stdout: 100\n961\n61\n1024243\ntriangles: 0 1 3 6 10 15 21 28 \nsum: 84\n

void print_int(int n);
void print_char(char c);
void print_string(char s[]);

int is_odd(int n);

int is_even(int n) {
    if (n == 0) {
        return 1;
    }
    return is_odd(n - 1);
}

int is_odd(int n) {
    if (n == 0) {
        return 0;
    }
    return is_even(n - 1);
}

// Ackermann grows fast enough that a frame leak shows up as a crash rather than a wrong number.
int ackermann(int m, int n) {
    if (m == 0) {
        return n + 1;
    }
    if (n == 0) {
        return ackermann(m - 1, 1);
    }
    return ackermann(m - 1, ackermann(m, n - 1));
}

int gcd(int a, int b) {
    if (b == 0) {
        return a;
    }
    return gcd(b, a % b);
}

int power(int base, int exponent) {
    if (exponent == 0) {
        return 1;
    }
    return base * power(base, exponent - 1);
}

// Fills `values` with the first `count` triangular numbers, recursively.
void triangles(int values[], int count, int at) {
    if (at >= count) {
        return;
    }
    if (at == 0) {
        values[at] = 0;
    } else {
        values[at] = values[at - 1] + at;
    }
    triangles(values, count, at + 1);
}

int sum(int values[], int count) {
    if (count == 0) {
        return 0;
    }
    return values[count - 1] + sum(values, count - 1);
}

int main(void) {
    print_int(is_even(10));
    print_int(is_odd(10));
    print_int(is_even(7));
    print_char('\n');

    print_int(ackermann(2, 3));
    print_int(ackermann(3, 3));
    print_char('\n');

    print_int(gcd(48, 18));
    print_int(gcd(17, 5));
    print_char('\n');

    print_int(power(2, 10));
    print_int(power(3, 5));
    print_char('\n');

    // Recursion, arrays, and string output together.
    int values[8];
    triangles(values, 8, 0);
    print_string("triangles: ");
    for (int i = 0; i < 8; i = i + 1) {
        print_int(values[i]);
        print_char(' ');
    }
    print_char('\n');

    print_string("sum: ");
    print_int(sum(values, 8));
    print_char('\n');

    return 0;
}
