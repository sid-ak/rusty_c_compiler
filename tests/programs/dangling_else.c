// The dangling `else`: an `else` with two unbraced `if`s in front of it belongs to the nearer one.
// Both shapes are written out, so the difference between "the else ran" and "nothing ran" is
// visible rather than inferred.
// clang-warns: -Wdangling-else
// expect-exit: 0
// expect-stdout: 1233 1322\n

void print_int(int n);
void print_char(char c);
void print_string(char s[]);

// Unbraced, so the `else` attaches to the inner `if`. With outer false, neither arm runs.
int nearest(int outer, int inner) {
    if (outer)
        if (inner)
            return 1;
        else
            return 2;
    return 3;
}

// The same source with braces forcing the other reading, for comparison.
int furthest(int outer, int inner) {
    if (outer) {
        if (inner) {
            return 1;
        }
    } else {
        return 2;
    }
    return 3;
}

int main(void) {
    print_int(nearest(1, 1));
    print_int(nearest(1, 0));
    print_int(nearest(0, 1));
    print_int(nearest(0, 0));
    print_char(' ');

    print_int(furthest(1, 1));
    print_int(furthest(1, 0));
    print_int(furthest(0, 1));
    print_int(furthest(0, 0));
    print_char('\n');

    return 0;
}
