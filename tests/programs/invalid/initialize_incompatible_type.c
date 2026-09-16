// rule: initializing a variable with an incompatible type
// expect: cannot initialize 'int' with 'int[2]'
// clang: rejects

int main(void) {
    int a[2];
    int x = a;
    return x;
}
