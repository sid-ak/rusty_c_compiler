// rule: an array used as a condition
// expect: value of type 'int[2]' is not a condition
// clang: accepts - intentional deviation, see ADR 0007
// why: clang decays the array to a pointer and tests that, warning that it is always true
// why: (-Wpointer-bool-conversion). This subset has no decay outside the argument position, so
// why: there is nothing for the condition to test.

int main(void) {
    int a[2];
    if (a) {
        return 1;
    }
    return 0;
}
