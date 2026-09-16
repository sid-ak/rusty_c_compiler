// rule: a definition disagreeing with an earlier declaration
// expect: conflicting declaration of 'helper'
// clang: rejects

int helper(int a);

int helper(char a) {
    return a;
}

int main(void) {
    return helper(1);
}
