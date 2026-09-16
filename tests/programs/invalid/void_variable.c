// rule: void as a variable type
// expect: variable has incomplete type 'void'
// clang: rejects

void value;

int main(void) {
    return 0;
}
