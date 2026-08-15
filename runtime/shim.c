/*
 * The runtime a compiled program links against: three fixed-arity output functions.
 *
 * Why three functions instead of printf is ADR 0006. The short version is that printf is variadic,
 * and variadic argument passing under AAPCS64 is its own distinct calling convention, and that
 * printf is buffered, so two binaries could flush at different points and produce a difference that
 * has nothing to do with either compiler.
 *
 * This file is ordinary C compiled by clang, not by mycc, so it may use the preprocessor and the
 * system headers that the compiled subset deliberately excludes. It is built once and the same
 * object is linked into both sides of every differential comparison, so there is no second runtime
 * implementation that could itself be wrong.
 */

#include <errno.h>
#include <unistd.h>

void print_int(int n);
void print_char(char c);
void print_string(char *s);

/*
 * Write every byte of `bytes`, or give up.
 *
 * write(2) is permitted to write fewer bytes than asked and to fail with EINTR if a signal arrives
 * mid-call, so a single call is not enough. There is no error channel to report a real failure
 * through — these functions return void by design — so a genuine write error ends the attempt
 * rather than looping forever.
 */
static void write_all(const char *bytes, size_t length) {
    size_t written = 0;

    while (written < length) {
        ssize_t result = write(STDOUT_FILENO, bytes + written, length - written);

        if (result < 0) {
            if (errno == EINTR) {
                continue;
            }
            return;
        }
        if (result == 0) {
            return;
        }

        written += (size_t)result;
    }
}

/*
 * Print `n` in decimal, with a leading '-' when negative.
 *
 * Digits are extracted in unsigned arithmetic. Negating INT_MIN as an int overflows, which is
 * undefined behavior, so the magnitude is computed as `0u - (unsigned int)n`: unsigned arithmetic
 * wraps by definition, and for INT_MIN that yields exactly 2147483648.
 */
void print_int(int n) {
    /* Eleven digits and a sign is the longest possible: -2147483648. */
    char digits[12];
    size_t index = sizeof(digits);
    unsigned int magnitude = (n < 0) ? (0u - (unsigned int)n) : (unsigned int)n;

    /* A do-while, so that zero prints as "0" rather than as nothing. */
    do {
        index--;
        digits[index] = (char)('0' + (magnitude % 10u));
        magnitude /= 10u;
    } while (magnitude != 0u);

    if (n < 0) {
        index--;
        digits[index] = '-';
    }

    write_all(digits + index, sizeof(digits) - index);
}

/* Print one byte. */
void print_char(char c) {
    write_all(&c, 1);
}

/* Print each byte of `s` up to, but not including, its null terminator. */
void print_string(char *s) {
    while (*s != '\0') {
        print_char(*s);
        s++;
    }
}
