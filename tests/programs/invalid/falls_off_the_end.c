// rule: control reaching the end of a non-void function
// expect: control reaches the end of non-void function 'one'
// clang: accepts - intentional deviation, see ADR 0008
// why: reaching the closing brace of a value-returning function and using the result is
// why: undefined behavior. clang reports it as a warning (-Wreturn-type); this compiler rejects
// why: statically detectable undefined behavior rather than emitting code for it.

int one(void) {
    int x;
    x = 1;
}

int main(void) {
    return one();
}
