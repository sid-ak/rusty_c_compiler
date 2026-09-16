// rule: use before declaration in the same scope
// expect: undeclared identifier 'x'
// clang: rejects

int main(void) {
    x = 1;
    int x;
    return x;
}
