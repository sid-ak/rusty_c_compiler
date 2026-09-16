// rule: call arity mismatch
// expect: 'add' takes 2 arguments, but 1 was passed
// clang: rejects

int add(int a, int b) {
    return a + b;
}

int main(void) {
    return add(1);
}
