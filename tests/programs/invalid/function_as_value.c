// rule: using a function name as a value
// expect: 'helper' is a function; it can only be called
// clang: rejects

int helper(void) {
    return 1;
}

int main(void) {
    return helper;
}
