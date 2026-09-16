// Falling off the end of `main`, which returns zero. Every other non-`void` function has to say
// what it returns; `main` is the one exception, and it is the shape most C programs are written in.
// expect-exit: 0
// expect-stdout: 6\n

void print_int(int n);
void print_char(char c);

int side_effects = 0;

void work(void) {
    for (int i = 0; i < 4; i = i + 1) {
        side_effects = side_effects + i;
    }
}

int main(void) {
    work();
    print_int(side_effects);
    print_char('\n');
}
