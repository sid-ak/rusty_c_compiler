// rule: assigning to an array name
// expect: array name is not assignable
// clang: rejects

int main(void) {
    int a[2];
    int b[2];
    a = b;
    return 0;
}
