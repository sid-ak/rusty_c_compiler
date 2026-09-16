// rule: call argument type mismatch
// expect: argument 1 of 'sum' has type 'int', but 'int *' was expected
// clang: rejects

int sum(int values[], int count);

int main(void) {
    return sum(1, 2);
}
