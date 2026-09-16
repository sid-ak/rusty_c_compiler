// rule: non-integer subscript
// expect: array subscript is not an integer
// clang: rejects

int main(void) {
    int a[2];
    int b[2];
    return a[b];
}
