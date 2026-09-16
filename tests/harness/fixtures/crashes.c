// A program that dies on a signal rather than returning. Built by clang only — the dereference is
// outside the subset — so the harness's own reading of a signal death can be tested end to end.
// The exit status of a signal death must not be confused with a return value.

int *nowhere = 0;

int main(void) {
    *nowhere = 1;
    return 0;
}
