// rule: bare return in a non-void function
// expect: 'return' with no value in a function returning 'int'
// clang: rejects

int one(void) {
    return;
}

int main(void) {
    return one();
}
