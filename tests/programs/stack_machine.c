// A tiny stack machine: an array used as a stack, a global holding the top, and functions that push
// and pop it. Every operation reaches the same storage from a different frame, and the order they
// run in is the whole of what makes the answer right.
// expect-exit: 0
// expect-stdout: 200 3 49 5150\n

void print_int(int n);
void print_char(char c);

int cells[32];
int top = 0;

void push(int value) {
    cells[top] = value;
    top = top + 1;
}

int pop(void) {
    top = top - 1;
    return cells[top];
}

int depth(void) {
    return top;
}

void add(void) {
    int right = pop();
    int left = pop();
    push(left + right);
}

void subtract(void) {
    int right = pop();
    int left = pop();
    push(left - right);
}

void multiply(void) {
    int right = pop();
    int left = pop();
    push(left * right);
}

void duplicate(void) {
    int value = pop();
    push(value);
    push(value);
}

int main(void) {
    // (2 + 3) * 4
    push(2);
    push(3);
    add();
    push(4);
    multiply();
    print_int(pop());
    print_int(depth());
    print_char(' ');

    // 10 - 4 - 3, where the order the operands come off the stack in decides the answer.
    push(10);
    push(4);
    subtract();
    push(3);
    subtract();
    print_int(pop());
    print_char(' ');

    // Squaring through duplication, so one value is read twice without being pushed twice.
    push(7);
    duplicate();
    multiply();
    print_int(pop());
    print_char(' ');

    // A deeper expression, left on the stack in pieces and folded at the end.
    for (int i = 1; i <= 5; i = i + 1) {
        push(i);
    }
    print_int(depth());
    while (depth() > 1) {
        add();
    }
    print_int(pop());
    print_int(depth());
    print_char('\n');

    return 0;
}
