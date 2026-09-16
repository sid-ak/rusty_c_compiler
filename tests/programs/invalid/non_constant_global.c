// rule: a non-constant global initializer
// expect: global initializer is not a constant
// clang: rejects

int seed(void) {
    return 1;
}

int value = seed();

int main(void) {
    return value;
}
