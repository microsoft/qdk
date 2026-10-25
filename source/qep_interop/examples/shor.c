// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include <errno.h>
#include <inttypes.h>
#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "qep_interop.h"

enum { MAX_BITS = 15, MAX_ATTEMPTS = 100 };
static const size_t NO_CONTROL = SIZE_MAX;

typedef struct {
    int bits;
    size_t phase, enable, carry, compare, scratch;
} Circuit;

/* Stops on an invalid input or a failed simulator invariant. */
static void fail(const char *message) {
    fprintf(stderr, "shor: %s\n", message);
    exit(EXIT_FAILURE);
}

/* Computes the nonnegative greatest common divisor. */
static int64_t gcd(int64_t a, int64_t b) {
    a = llabs(a);
    while (b != 0) {
        int64_t remainder = a % b;
        a = b;
        b = remainder;
    }
    return a;
}

/* Computes a modular power without constructing the full integer power. */
static int64_t power_mod(int64_t base, int64_t exponent, int64_t modulus) {
    int64_t value = 1;
    while (exponent != 0) {
        if ((exponent & 1) != 0) value = value * base % modulus;
        base = base * base % modulus;
        exponent >>= 1;
    }
    return value;
}

/* Finds the inverse of a coprime constant using extended Euclid. */
static int64_t inverse_mod(int64_t value, int64_t modulus) {
    int64_t a = modulus, b = value, u = 0, v = 1;
    while (b != 0) {
        int64_t quotient = a / b;
        int64_t next = a - quotient * b;
        a = b;
        b = next;
        next = u - quotient * v;
        u = v;
        v = next;
    }
    if (a != 1) fail("modular multiplier must be coprime to the modulus");
    return (u % modulus + modulus) % modulus;
}

/* Assigns disjoint eigenstate, work, control, carry, and clean scratch registers. */
static Circuit make_circuit(int bits) {
    Circuit circuit = {
        bits, (size_t)(2 * bits), (size_t)(2 * bits + 1),
        (size_t)(2 * bits + 2), (size_t)(2 * bits + 3), (size_t)(2 * bits + 4)
    };
    return circuit;
}

/* Decomposes a multi-controlled X into a clean, uncomputed Toffoli ladder. */
static void controlled_x(const Circuit *circuit, const size_t *controls, int count, size_t target) {
    if (count == 0) {
        x(target);
    } else if (count == 1) {
        cx(controls[0], target);
    } else if (count == 2) {
        ccx(controls[0], controls[1], target);
    } else {
        ccx(controls[0], controls[1], circuit->scratch);
        for (int i = 2; i < count - 1; ++i) {
            ccx(circuit->scratch + (size_t)i - 2, controls[i], circuit->scratch + (size_t)i - 1);
        }
        ccx(circuit->scratch + (size_t)count - 3, controls[count - 1], target);
        for (int i = count - 2; i >= 2; --i) {
            ccx(circuit->scratch + (size_t)i - 2, controls[i], circuit->scratch + (size_t)i - 1);
        }
        ccx(controls[0], controls[1], circuit->scratch);
    }
}

/* Adds a signed constant modulo 2^width using controlled binary increments. */
static void add_constant(
    const Circuit *circuit, int64_t constant, const size_t *targets, int width, size_t control
) {
    uint64_t encoded = (uint64_t)constant & ((UINT64_C(1) << width) - 1);
    for (int bit = 0; bit < width; ++bit) {
        if (((encoded >> bit) & 1) == 0) continue;
        for (int target = width - 1; target >= bit; --target) {
            size_t controls[MAX_BITS + 1];
            int count = 0;
            if (control != NO_CONTROL) controls[count++] = control;
            for (int i = bit; i < target; ++i) controls[count++] = targets[i];
            controlled_x(circuit, controls, count, targets[target]);
        }
    }
}

/* Implements the sample's controlled modular addition, restoring both borrow ancillas. */
static void modular_add(const Circuit *circuit, int64_t modulus, int64_t constant) {
    if (constant == 0) return;
    size_t work[MAX_BITS + 1];
    for (int i = 0; i < circuit->bits; ++i) work[i] = (size_t)(circuit->bits + i);
    work[circuit->bits] = circuit->carry;
    add_constant(circuit, constant, work, circuit->bits + 1, circuit->enable);
    add_constant(circuit, -modulus, work, circuit->bits + 1, circuit->enable);
    add_constant(circuit, modulus, work, circuit->bits, circuit->carry);

    // Toggle the carry iff the reduced work register is at least the added constant.
    work[circuit->bits] = circuit->compare;
    add_constant(circuit, -constant, work, circuit->bits + 1, NO_CONTROL);
    x(circuit->compare);
    ccx(circuit->enable, circuit->compare, circuit->carry);
    x(circuit->compare);
    add_constant(circuit, constant, work, circuit->bits + 1, NO_CONTROL);
}

/* Multiplies the eigenstate register modulo N, controlled by the phase qubit. */
static void modular_multiply(const Circuit *circuit, int64_t modulus, int64_t constant) {
    for (int i = 0; i < circuit->bits; ++i) {
        int64_t shifted = (constant << i) % modulus;
        ccx(circuit->phase, (size_t)i, circuit->enable);
        modular_add(circuit, modulus, shifted);
        ccx(circuit->phase, (size_t)i, circuit->enable);
    }
    for (int i = 0; i < circuit->bits; ++i) {
        size_t work = (size_t)(circuit->bits + i);
        cx(work, (size_t)i);
        ccx(circuit->phase, (size_t)i, work);
        cx(work, (size_t)i);
    }
    int64_t inverse = inverse_mod(constant, modulus);
    for (int i = 0; i < circuit->bits; ++i) {
        int64_t shifted = (inverse << i) % modulus;
        ccx(circuit->phase, (size_t)i, circuit->enable);
        modular_add(circuit, modulus, (modulus - shifted) % modulus);
        ccx(circuit->phase, (size_t)i, circuit->enable);
    }
}

/* Measures and resets a qubit, rejecting backend error outcomes. */
static int64_t measure_reset(size_t qubit) {
    size_t result = mz_reset(qubit);
    if (result > 1) fail("backend returned a measurement error");
    return (int64_t)result;
}

/* Measures a little-endian register and restores it to zero. */
static int64_t read_register(size_t start, int bits) {
    int64_t value = 0;
    for (int i = 0; i < bits; ++i) value |= measure_reset(start + (size_t)i) << i;
    return value;
}

/* Runs the sample's iterative phase estimation with semiclassical Fourier feedback. */
static int64_t estimate_frequency(int64_t generator, int64_t modulus, int bits) {
    Circuit circuit = make_circuit(bits);
    int precision = 2 * bits + 1;
    int64_t frequency = 0;
    x(0);
    for (int index = precision - 1; index >= 0; --index) {
        int step = precision - 1 - index;
        h(circuit.phase);
        modular_multiply(&circuit, modulus, power_mod(generator, INT64_C(1) << index, modulus));
        // R1Frac and Rz differ only by an irrelevant global phase here.
        rz(circuit.phase, acos(-1.0) * (double)frequency / (double)(INT64_C(1) << step));
        h(circuit.phase);
        if (measure_reset(circuit.phase) == 1) frequency |= INT64_C(1) << step;
    }
    for (int i = 0; i < bits; ++i) reset((size_t)i);
    printf("Estimated frequency=%" PRId64 "/%" PRId64 "\n", frequency, INT64_C(1) << precision);
    return frequency;
}

/* Recovers the bounded continued-fraction denominator used by the Q# sample. */
static int64_t period_from_frequency(int64_t modulus, int64_t frequency, int precision) {
    if (frequency == 0) return 1;
    int64_t r1 = frequency, r2 = INT64_C(1) << precision, s1 = 1, s2 = 0;
    while (r2 != 0 && llabs(s2) <= modulus) {
        int64_t quotient = r1 / r2;
        int64_t next = r1 - quotient * r2;
        r1 = r2;
        r2 = next;
        next = s1 - quotient * s2;
        s1 = s2;
        s2 = next;
    }
    return llabs(r2 == 0 && llabs(s2) <= modulus ? s2 : s1);
}

/* Attempts the sample's even-period reduction to a nontrivial factor. */
static int64_t factor_from_period(int64_t number, int64_t generator, int64_t period) {
    if (period % 2 != 0) return 1;
    int64_t half = power_mod(generator, period / 2, number);
    if (half == number - 1) return 1;
    int64_t a = gcd(half - 1, number), b = gcd(half + 1, number);
    int64_t factor = a > b ? a : b;
    return factor > 1 && factor < number ? factor : 1;
}

/* Factors an input with quantum order finding and at most 100 generator attempts. */
static int64_t factor_semiprime(int64_t number) {
    if (number % 2 == 0) return 2;
    int bits = 0;
    for (int64_t value = number; value != 0; value >>= 1) ++bits;
    for (int attempt = 1; attempt <= MAX_ATTEMPTS; ++attempt) {
        // Start with a coprime base for odd inputs so the example exercises quantum order finding.
        int64_t generator = attempt == 1 ? 2 : 1 + rand() % (number - 1);
        printf("Factorizing %" PRId64 ", attempt %d, generator=%" PRId64 "\n", number, attempt, generator);
        int64_t factor = gcd(generator, number);
        if (factor == 1) {
            int64_t frequency = estimate_frequency(generator, number, bits);
            int64_t period = period_from_frequency(number, frequency, 2 * bits + 1);
            printf("Estimated period=%" PRId64 "\n", period);
            factor = factor_from_period(number, generator, period);
        } else {
            printf("Guessed a divisor by accident: %" PRId64 "\n", factor);
        }
        if (factor > 1 && factor < number) return factor;
    }
    fail("failed to find factors after 100 attempts");
    return 1;
}

/* Verifies all arithmetic work qubits are restored rather than hiding garbage by resetting it. */
static void check_clean(const Circuit *circuit) {
    for (size_t qubit = 0; qubit < (size_t)(3 * circuit->bits + 4); ++qubit) {
        if (measure_reset(qubit) != 0) fail("arithmetic left a dirty ancilla");
    }
}

/* Exhaustively tests controlled modular arithmetic on small basis-state inputs. */
static void self_test(void) {
    for (int64_t modulus = 3; modulus <= 7; ++modulus) {
        int bits = modulus < 4 ? 2 : 3;
        Circuit circuit = make_circuit(bits);
        for (int64_t constant = 0; constant < modulus; ++constant) {
            for (int64_t value = 0; value < modulus; ++value) {
                for (int enabled = 0; enabled <= 1; ++enabled) {
                    for (int i = 0; i < bits; ++i) {
                        if (((value >> i) & 1) != 0) x((size_t)(bits + i));
                    }
                    if (enabled != 0) x(circuit.enable);
                    modular_add(&circuit, modulus, constant);
                    int64_t expected = enabled != 0 ? (value + constant) % modulus : value;
                    if (read_register((size_t)bits, bits) != expected) fail("modular addition failed");
                    if (measure_reset(circuit.enable) != enabled) fail("addition changed its control");
                    check_clean(&circuit);

                    if (gcd(constant, modulus) != 1) continue;
                    for (int i = 0; i < bits; ++i) {
                        if (((value >> i) & 1) != 0) x((size_t)i);
                    }
                    if (enabled != 0) x(circuit.phase);
                    modular_multiply(&circuit, modulus, constant);
                    expected = enabled != 0 ? value * constant % modulus : value;
                    if (read_register(0, bits) != expected) fail("modular multiplication failed");
                    if (measure_reset(circuit.phase) != enabled) fail("multiplication changed its control");
                    check_clean(&circuit);
                }
            }
        }
        x((size_t)bits);
        h(circuit.enable);
        modular_add(&circuit, modulus, modulus - 1);
        modular_add(&circuit, modulus, 1);
        h(circuit.enable);
        if (read_register((size_t)bits, bits) != 1) fail("coherent addition did not invert");
        check_clean(&circuit);

        x(0);
        h(circuit.phase);
        modular_multiply(&circuit, modulus, modulus - 1);
        modular_multiply(&circuit, modulus, modulus - 1);
        h(circuit.phase);
        if (read_register(0, bits) != 1) fail("coherent multiplication did not invert");
        check_clean(&circuit);
    }
    if (period_from_frequency(15, 32, 7) != 4) fail("continued fractions failed");
    if (factor_from_period(15, 2, 4) != 5) fail("period-to-factor reduction failed");
    puts("Arithmetic self-test passed.");
}

/* Runs the sample's default factorization or the reversible-arithmetic regression tests. */
int main(int argc, char **argv) {
    if (argc > 2) fail("usage: shor [number | --self-test]");
    init("sparse_sim");
    if (argc == 2 && strcmp(argv[1], "--self-test") == 0) {
        self_test();
        return EXIT_SUCCESS;
    }
    int64_t number = 187;
    if (argc == 2) {
        char *end = NULL;
        errno = 0;
        number = strtoll(argv[1], &end, 10);
        if (errno != 0 || *end != '\0' || end == argv[1]) fail("number must be an integer");
    }
    if (number < 4 || number >= (INT64_C(1) << MAX_BITS)) fail("number must be in [4, 32767]");
    srand((unsigned)time(NULL));
    int64_t factor = factor_semiprime(number);
    printf("Found factorization %" PRId64 " = %" PRId64 " * %" PRId64 "\n", number, factor, number / factor);
    return EXIT_SUCCESS;
}
