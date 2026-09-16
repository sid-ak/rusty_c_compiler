// rule: return with a value in a void function
// expect: 'return' with a value in a function returning 'void'
// clang: rejects

void nothing(void) {
    return 1;
}

int main(void) {
    nothing();
    return 0;
}
