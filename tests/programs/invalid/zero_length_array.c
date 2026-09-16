// rule: a zero-length array
// expect: array size must be greater than zero
// clang: accepts - intentional deviation, see ADR 0010
// why: a zero-length array is a constraint violation in C99 (6.7.5.2p1); clang accepts it as
// why: a GNU extension (-Wzero-length-array). This compiler treats a constraint violation as an
// why: error.

int main(void) {
    int a[0];
    return 0;
}
