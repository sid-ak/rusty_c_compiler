// Real C that this compiler turns down: the subset has no declared pointer variables. clang builds
// it, rustycc rejects it, and the harness has to report that as a program one compiler would not
// build rather than as the two disagreeing about an answer.

void print_string(char s[]);

int main(void) {
    int value = 1;
    int *pointer = &value;
    print_string("unreachable");
    return *pointer;
}
