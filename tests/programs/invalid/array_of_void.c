// rule: an array of void
// expect: array has incomplete element type 'void'
// clang: rejects

void letters[3];

int main(void) {
    return 0;
}
