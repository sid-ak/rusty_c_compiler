// Multiplying two matrices held as flat arrays: three nested loops, an accumulator carried across
// the innermost one, and index arithmetic that reads one operand by row and the other by column.
// expect-exit: 0
// expect-stdout: 1 2 3 4 5 6 7 8 9 |1 2 3 4 5 6 7 8 9 |30 36 42 66 81 96 102 126 150 |261\n

void print_int(int n);
void print_char(char c);

int size = 3;

void multiply(int left[], int right[], int result[]) {
    for (int row = 0; row < 3; row = row + 1) {
        for (int column = 0; column < 3; column = column + 1) {
            int total = 0;
            for (int k = 0; k < 3; k = k + 1) {
                total = total + left[row * 3 + k] * right[k * 3 + column];
            }
            result[row * 3 + column] = total;
        }
    }
}

void show(int matrix[]) {
    for (int i = 0; i < 9; i = i + 1) {
        print_int(matrix[i]);
        print_char(' ');
    }
    print_char('|');
}

int main(void) {
    int identity[9] = {1, 0, 0, 0, 1, 0, 0, 0, 1};
    int values[9] = {1, 2, 3, 4, 5, 6, 7, 8, 9};
    int result[9];

    // Times the identity, which has to give back exactly what went in.
    multiply(values, identity, result);
    show(result);

    // The identity on the other side, which reads the operands in the other order.
    multiply(identity, values, result);
    show(result);

    // A real product, where every element depends on a whole row and a whole column.
    multiply(values, values, result);
    show(result);

    // The trace of the product, which touches one element per row.
    int trace = 0;
    for (int i = 0; i < 3; i = i + 1) {
        trace = trace + result[i * 3 + i];
    }
    print_int(trace);
    print_char('\n');

    return 0;
}
