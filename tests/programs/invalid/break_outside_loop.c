// rule: break outside a loop
// expect: 'break' outside of a loop
// clang: rejects

int main(void) {
    break;
    return 0;
}
