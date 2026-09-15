// Integer arithmetic: every operator in the subset, the precedence rules that decide how they
// group, and the four increment forms.

void print_int(int n);
void print_char(char c);

int main(void) {
    int a = 17;
    int b = 5;

    print_int(a + b);
    print_int(a - b);
    print_int(a * b);
    print_int(a / b);
    print_int(a % b);

    print_int(-a);
    print_int(+b);
    print_int(!b);
    print_int(!0);

    // Precedence: multiplication before addition, relational before equality, && before ||.
    print_int(1 + 2 * 3);
    print_int((1 + 2) * 3);
    print_int(1 < 2 == 3 < 4);
    print_int(a > b && b > 0);
    print_int(a < b || b < a);
    print_int(a > b || a > b && a < b);

    // Left associativity, written so that grouping the other way gives a different answer.
    print_int(100 - 10 - 1);
    print_int(100 / 10 / 2);
    print_int(100 % 30 % 4);

    // Short-circuiting: the right operand of a && whose left is 0 is never reached, so this
    // divides by zero only if short-circuiting is broken.
    print_int(b - 5 && a / (b - 5));

    int n = 0;
    n = n + 1;
    print_int(n++);
    print_int(n);
    print_int(++n);
    print_int(n--);
    print_int(--n);

    // Right-associative assignment: both names end up holding the same value.
    int x = 0;
    int y = 0;
    x = y = 9;
    print_int(x);
    print_int(y);

    // A char promotes to int for the arithmetic and stays an int through it.
    char c = 'A';
    print_int(c);
    print_int(c + 2);

    print_char('\n');

    return 0;
}
