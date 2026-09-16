// rule: indexing a non-array and non-pointer
// expect: subscripted value is not an array or a pointer
// clang: rejects

int main(void) {
    int a;
    a = 1;
    return a[0];
}
