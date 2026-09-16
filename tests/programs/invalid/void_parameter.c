// rule: void as a parameter type
// expect: parameter has incomplete type 'void'
// clang: rejects

int helper(void a);

int main(void) {
    return 0;
}
