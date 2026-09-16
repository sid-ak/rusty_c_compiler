// rule: redeclaration in the same scope
// expect: redeclaration of 'x' in this scope
// clang: rejects

int main(void) {
    int x;
    int x;
    return x;
}
