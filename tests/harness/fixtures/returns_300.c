// A return value that does not fit in an exit status. The operating system reports the low eight
// bits, so 300 arrives as 44 under either compiler, and the two have to compare equal.

void print_string(char s[]);

int main(void) {
    print_string("done");
    return 300;
}
