// rule: conflicting redeclaration of a function signature
// expect: conflicting declaration of 'helper'
// clang: rejects

int helper(int a);
char helper(int a);

int main(void) {
    return helper(1);
}
