// rule: continue outside a loop
// expect: 'continue' outside of a loop
// clang: rejects

int main(void) {
    continue;
    return 0;
}
