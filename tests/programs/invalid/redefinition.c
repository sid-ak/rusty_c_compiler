// rule: multiple definitions of one function
// expect: redefinition of 'helper'
// clang: rejects

int helper(void) {
    return 1;
}

int helper(void) {
    return 2;
}

int main(void) {
    return helper();
}
