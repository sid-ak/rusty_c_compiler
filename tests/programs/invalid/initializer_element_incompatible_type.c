// rule: an array initializer element of an incompatible type
// expect: cannot initialize 'int' with 'int[2]'
// clang: rejects

int main(void) {
    int b[2];
    int a[2] = {1, b};
    return a[0];
}
