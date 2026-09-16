// rule: an array used in arithmetic
// expect: invalid operands to binary '+': 'int[2]' and 'int'
// clang: rejects
// note: clang rejects this too, though for the consequence rather than the cause: the
// note: array decays and returning an 'int *' from an 'int' function is the error it reports.

int main(void) {
    int a[2];
    return a + 1;
}
