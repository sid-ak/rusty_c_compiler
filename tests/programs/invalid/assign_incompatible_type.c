// rule: assigning a value of an incompatible type
// expect: cannot assign 'int[2]' to 'int'
// clang: rejects

int main(void) {
    int x;
    int a[2];
    x = a;
    return x;
}
