// A recursion deep enough that every frame has to be laid out and torn down correctly a thousand
// times over. A frame that leaks a few bytes per call, or that fails to restore the frame pointer,
// shows up here and not in a three-level example.
// expect-exit: 0
// expect-stdout: 1000 5050 500500 1000\n

void print_int(int n);
void print_char(char c);

int count_down(int n) {
    if (n == 0) {
        return 0;
    }
    return 1 + count_down(n - 1);
}

// Locals in every frame, so each level has state of its own to preserve across the call.
int sum_to(int n) {
    int doubled = n * 2;
    int halved = doubled / 2;
    if (n == 0) {
        return 0;
    }
    return halved + sum_to(n - 1);
}

int main(void) {
    print_int(count_down(1000));
    print_char(' ');
    print_int(sum_to(100));
    print_char(' ');
    print_int(sum_to(1000));
    print_char(' ');

    // Called repeatedly, so a frame that is not fully restored accumulates rather than cancelling.
    int total = 0;
    for (int i = 0; i < 20; i = i + 1) {
        total = total + count_down(50);
    }
    print_int(total);
    print_char('\n');

    return 0;
}
