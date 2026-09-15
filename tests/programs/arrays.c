// Arrays: declaring them with and without an initializer list, indexing them for reading and for
// writing, and passing one to a function, which is the only place an array decays to a pointer.

void print_int(int n);
void print_char(char c);

int totals[4];

// The parameter is written `int values[]`: the length is not part of it, so it is passed
// alongside.
int sum(int values[], int count) {
    int total = 0;
    for (int i = 0; i < count; i = i + 1) {
        total = total + values[i];
    }
    return total;
}

void fill(int values[], int count, int value) {
    for (int i = 0; i < count; i = i + 1) {
        values[i] = value;
    }
}

int main(void) {
    int numbers[5];
    for (int i = 0; i < 5; i = i + 1) {
        numbers[i] = i * i;
    }

    print_int(numbers[0]);
    print_int(numbers[4]);
    print_int(numbers[2] + numbers[3]);
    print_int(sum(numbers, 5));

    // An initializer list, and one shorter than the array it fills.
    int primes[4] = {2, 3, 5, 7};
    print_int(primes[0]);
    print_int(primes[3]);
    print_int(sum(primes, 4));

    int partial[3] = {1, 2};
    print_int(partial[0] + partial[1]);

    // The subscript is an expression, not only a literal.
    int index = 1;
    print_int(primes[index + 1]);
    print_int(primes[index]++);
    print_int(primes[index]);

    // Writing through an index, including from another array's element.
    numbers[0] = primes[0];
    numbers[1] = numbers[0] * 2;
    print_int(numbers[0]);
    print_int(numbers[1]);

    // A global array, filled through a function that received it as a parameter.
    fill(totals, 4, 6);
    print_int(sum(totals, 4));

    char letters[3] = {'a', 'b', 'c'};
    print_char(letters[0]);
    print_char(letters[2]);
    print_char('\n');

    return 0;
}
