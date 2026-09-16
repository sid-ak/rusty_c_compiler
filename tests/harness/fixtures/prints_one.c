// The reference half of the harness's fault-injection test: prints one thing, returns 0.

void print_string(char s[]);

int main(void) {
    print_string("one");
    return 0;
}
