// rule: calling a non-function
// expect: called object is not a function
// clang: rejects

int main(void) {
    int helper;
    helper = 1;
    return helper();
}
