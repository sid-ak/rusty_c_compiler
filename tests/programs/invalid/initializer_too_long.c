// rule: an array initializer longer than the array
// expect: 3 initializers for an array of 2
// clang: accepts - intentional deviation, see ADR 0010
// why: excess initializers are a constraint violation in C99 (6.7.8p2), which clang
// why: diagnoses as a warning (-Wexcess-initializers) and then truncates. This compiler treats a
// why: constraint violation as an error.

int a[2] = {1, 2, 3};

int main(void) {
    return a[0];
}
