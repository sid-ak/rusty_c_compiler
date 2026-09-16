// Loops over an array: filling it, reading it back, walking it in reverse, and walking two at once,
// which is where an index that is not advanced correctly stops being invisible.
// expect-exit: 0
// expect-stdout: 36 8 7 6 5 4 3 2 1 16 60 136\n

void print_int(int n);
void print_char(char c);

int main(void) {
    int values[8];

    for (int i = 0; i < 8; i = i + 1) {
        values[i] = i + 1;
    }

    int total = 0;
    for (int i = 0; i < 8; i = i + 1) {
        total = total + values[i];
    }
    print_int(total);
    print_char(' ');

    // Backwards, which needs the index to start at the last element rather than at the length.
    for (int i = 7; i >= 0; i = i - 1) {
        print_int(values[i]);
        print_char(' ');
    }

    // Every other element.
    int evens = 0;
    for (int i = 0; i < 8; i = i + 2) {
        evens = evens + values[i];
    }
    print_int(evens);
    print_char(' ');

    // Two indices moving toward each other, reading a pair per pass.
    int pairs = 0;
    int low = 0;
    int high = 7;
    while (low < high) {
        pairs = pairs + values[low] * values[high];
        low = low + 1;
        high = high - 1;
    }
    print_int(pairs);
    print_char(' ');

    // A running total written back into the array it is reading from.
    for (int i = 1; i < 8; i = i + 1) {
        values[i] = values[i] + values[i - 1];
    }
    print_int(values[0]);
    print_int(values[7]);
    print_char('\n');

    return 0;
}
