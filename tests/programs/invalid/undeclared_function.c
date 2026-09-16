// rule: undeclared function
// expect: undeclared identifier 'helper'
// clang: rejects

int main(void) {
    return helper();
}
