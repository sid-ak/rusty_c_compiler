// A sieve of Eratosthenes over a global array: nested loops whose inner bound depends on the outer
// index, a step that is not one, and a second pass that reads what the first pass wrote.
// expect-exit: 0
// expect-stdout: 2 3 5 7 11 13 17 19 23 29 31 37 41 43 47 15 1\n

void print_int(int n);
void print_char(char c);

int limit = 50;
int composite[50];

void sieve(void) {
    for (int i = 2; i * i < limit; i = i + 1) {
        if (composite[i] == 0) {
            for (int multiple = i * i; multiple < limit; multiple = multiple + i) {
                composite[multiple] = 1;
            }
        }
    }
}

int count_primes(void) {
    int found = 0;
    for (int i = 2; i < limit; i = i + 1) {
        if (composite[i] == 0) {
            found = found + 1;
        }
    }
    return found;
}

int is_prime(int n) {
    if (n < 2) {
        return 0;
    }
    for (int divisor = 2; divisor * divisor <= n; divisor = divisor + 1) {
        if (n % divisor == 0) {
            return 0;
        }
    }
    return 1;
}

int main(void) {
    sieve();

    for (int i = 2; i < limit; i = i + 1) {
        if (composite[i] == 0) {
            print_int(i);
            print_char(' ');
        }
    }
    print_int(count_primes());
    print_char(' ');

    // Trial division agrees with the sieve on every number in range, which is two different pieces
    // of code reaching the same answer rather than one piece agreeing with itself.
    int agree = 1;
    for (int i = 0; i < limit; i = i + 1) {
        int sieved = 0;
        if (i >= 2 && composite[i] == 0) {
            sieved = 1;
        }
        if (sieved != is_prime(i)) {
            agree = 0;
        }
    }
    print_int(agree);
    print_char('\n');

    return 0;
}
