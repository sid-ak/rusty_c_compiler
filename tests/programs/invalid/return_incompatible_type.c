// rule: returning a value of an incompatible type
// expect: cannot return 'int[2]' from a function returning 'int'
// clang: rejects

int first(void) {
    int a[2];
    return a;
}

int main(void) {
    return first();
}
