// Branches and loops: every statement form the subset has, including each combination of a
// present and an absent `for` clause, and both ways out of a loop early.

void print_int(int n);
void print_char(char c);

int main(void) {
    int i = 0;
    int total = 0;

    if (i == 0) {
        print_int(1);
    }

    if (i != 0) {
        print_int(2);
    } else {
        print_int(3);
    }

    // An else-if chain, which is an `else` whose statement is another `if`.
    if (i > 0) {
        print_int(4);
    } else if (i < 0) {
        print_int(5);
    } else {
        print_int(6);
    }

    // A body without braces, and a nested `if` whose `else` belongs to the inner one.
    if (i == 0)
        if (i == 1)
            print_int(7);
        else
            print_int(8);

    while (i < 3) {
        total = total + i;
        i = i + 1;
    }
    print_int(total);

    // All three clauses present.
    for (int j = 0; j < 3; j = j + 1) {
        print_int(j);
    }

    // No initializer.
    int k = 0;
    for (; k < 2; k = k + 1) {
        print_int(k);
    }

    // No step.
    for (k = 0; k < 2;) {
        print_int(k);
        k = k + 1;
    }

    // No condition, escaped with `break`.
    for (k = 0;; k = k + 1) {
        if (k > 1) {
            break;
        }
        print_int(k);
    }

    // No clauses at all.
    k = 0;
    for (;;) {
        k = k + 1;
        if (k > 2) {
            break;
        }
    }
    print_int(k);

    // `continue` skips the rest of one iteration; in a `for` the step still runs.
    for (int m = 0; m < 5; m = m + 1) {
        if (m % 2 == 0) {
            continue;
        }
        print_int(m);
    }

    // A `continue` in a `while` reaches the condition again, so the counter has to move first.
    int p = 0;
    while (p < 4) {
        p = p + 1;
        if (p == 2) {
            continue;
        }
        print_int(p);
    }

    // A nested loop, so that `break` is seen leaving the inner one only.
    for (int r = 0; r < 2; r = r + 1) {
        for (int c = 0; c < 4; c = c + 1) {
            if (c == 2) {
                break;
            }
            print_int(r * 10 + c);
        }
    }

    // A block inside a block, and a statement that does nothing.
    {
        int shadowed = 100;
        {
            print_int(shadowed);
        }
    }
    ;

    print_char('\n');

    return 0;
}
