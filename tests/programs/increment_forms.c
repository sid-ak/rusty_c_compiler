// Prefix and postfix increment and decrement: the value each form produces, which is the half that
// is easy to get wrong, alongside the effect they share.
// expect-exit: 0
// expect-stdout: 66 55 56 65 134 144 31\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int n = 5;

    // Prefix yields the new value.
    print_int(++n);
    print_int(n);
    print_char(' ');
    print_int(--n);
    print_int(n);
    print_char(' ');

    // Postfix yields the old one.
    print_int(n++);
    print_int(n);
    print_char(' ');
    print_int(n--);
    print_int(n);
    print_char(' ');

    // Inside a larger expression, where the difference between the two forms shows up as a
    // different total rather than a different variable.
    int a = 3;
    print_int(a++ + 10);
    print_int(a);
    print_char(' ');

    int b = 3;
    print_int(++b + 10);
    print_int(b);
    print_char(' ');

    // As a statement, where only the effect matters.
    int count = 0;
    count++;
    count++;
    ++count;
    print_int(count);
    count--;
    --count;
    print_int(count);
    print_char('\n');

    return 0;
}
