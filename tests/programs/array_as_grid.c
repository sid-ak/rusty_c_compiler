// A two-dimensional grid held in a flat array, since the subset has one dimension. Every access is
// `row * width + column`, which is the arithmetic a real multi-dimensional index would compile to.
// expect-exit: 0
// expect-stdout: 0 1 2 3 |10 11 12 13 |20 21 22 23 | 4636 33\n

void print_int(int n);
void print_char(char c);

// The width is a global so the index arithmetic reads the same in both directions and in one
// place, which is the closest this subset gets to declaring the shape of the grid.
int width = 4;

int at(int grid[], int row, int column) {
    return grid[row * width + column];
}

void put(int grid[], int row, int column, int value) {
    grid[row * width + column] = value;
}

int main(void) {
    int grid[12];

    for (int row = 0; row < 3; row = row + 1) {
        for (int column = 0; column < 4; column = column + 1) {
            put(grid, row, column, row * 10 + column);
        }
    }

    for (int row = 0; row < 3; row = row + 1) {
        for (int column = 0; column < 4; column = column + 1) {
            print_int(at(grid, row, column));
            print_char(' ');
        }
        print_char('|');
    }
    print_char(' ');

    // Summing one row and one column, which walk the same storage with different strides.
    int row_total = 0;
    for (int column = 0; column < 4; column = column + 1) {
        row_total = row_total + at(grid, 1, column);
    }
    print_int(row_total);

    int column_total = 0;
    for (int row = 0; row < 3; row = row + 1) {
        column_total = column_total + at(grid, row, 2);
    }
    print_int(column_total);
    print_char(' ');

    // The diagonal, where both indices advance together.
    int diagonal = 0;
    for (int i = 0; i < 3; i = i + 1) {
        diagonal = diagonal + at(grid, i, i);
    }
    print_int(diagonal);
    print_char('\n');

    return 0;
}
