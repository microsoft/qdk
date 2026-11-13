// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <inttypes.h>
#include <math.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "arithmetic.h"
#include "qep_interop.h"

enum { MAX_BITS = 2048, MAX_ATTEMPTS = 100 };

typedef struct {
    int bits;
    size_t phase, enable, carry, scratch;
} Circuit;

/* Stops on an invalid input or a failed simulator invariant. */
static void fail(const char *message) {
    fprintf(stderr, "shor: %s\n", message);
    exit(EXIT_FAILURE);
}

/* Assigns disjoint eigenstate, work, control, carry, and clean scratch registers. */
static Circuit make_circuit(int bits) {
    Circuit circuit = {
        bits, (size_t)(2 * bits), (size_t)(2 * bits + 1),
        (size_t)(2 * bits + 2), (size_t)(2 * bits + 3)
    };
    return circuit;
}

/* Follows the sample's controlled modular addition using the Q# arithmetic ports. */
static void modular_add(const Circuit *circuit, const mpz_t modulus, const mpz_t constant) {
    size_t work[MAX_BITS + 1];
    for (int i = 0; i < circuit->bits; ++i) work[i] = (size_t)(circuit->bits + i);
    work[circuit->bits] = circuit->carry;
    inc_by_i(constant, work, (size_t)circuit->bits + 1, circuit->enable, false, circuit->scratch);
    inc_by_i(modulus, work, (size_t)circuit->bits + 1, circuit->enable, true, circuit->scratch);
    inc_by_i(modulus, work, (size_t)circuit->bits, circuit->carry, false, circuit->scratch);
    apply_if_less_or_equal_l(
        constant, work, (size_t)circuit->bits, circuit->carry, circuit->enable, circuit->scratch
    );
}

/* Advances a residue by one binary shift without constructing a large shifted integer. */
static void double_mod(mpz_t value, const mpz_t modulus) {
    mpz_mul_2exp(value, value, 1);
    if (mpz_cmp(value, modulus) >= 0) mpz_sub(value, value, modulus);
}

/* Multiplies the eigenstate register modulo N, controlled by the phase qubit. */
static void modular_multiply(const Circuit *circuit, const mpz_t modulus, const mpz_t constant) {
    mpz_t shifted, inverse, complement;
    mpz_inits(shifted, inverse, complement, NULL);
    if (mpz_invert(inverse, constant, modulus) == 0) {
        fail("modular multiplier must be coprime to the modulus");
    }
    mpz_mod(shifted, constant, modulus);
    for (int i = 0; i < circuit->bits; ++i) {
        qdk_ccx(circuit->phase, (size_t)i, circuit->enable);
        modular_add(circuit, modulus, shifted);
        qdk_ccx(circuit->phase, (size_t)i, circuit->enable);
        double_mod(shifted, modulus);
    }
    for (int i = 0; i < circuit->bits; ++i) {
        size_t work = (size_t)(circuit->bits + i);
        qdk_cx(work, (size_t)i);
        qdk_ccx(circuit->phase, (size_t)i, work);
        qdk_cx(work, (size_t)i);
    }
    mpz_set(shifted, inverse);
    for (int i = 0; i < circuit->bits; ++i) {
        mpz_sub(complement, modulus, shifted);
        qdk_ccx(circuit->phase, (size_t)i, circuit->enable);
        modular_add(circuit, modulus, complement);
        qdk_ccx(circuit->phase, (size_t)i, circuit->enable);
        double_mod(shifted, modulus);
    }
    mpz_clears(shifted, inverse, complement, NULL);
}

/* Measures and resets a qubit, rejecting backend error outcomes. */
static int measure_reset(size_t qubit) {
    size_t result = qdk_mz_reset(qubit);
    if (result > 1) fail("backend returned a measurement error");
    return (int)result;
}

/* Measures a little-endian register and restores it to zero. */
static void read_register(mpz_t value, size_t start, int bits) {
    mpz_set_ui(value, 0);
    for (int i = 0; i < bits; ++i) {
        if (measure_reset(start + (size_t)i) != 0) mpz_setbit(value, (mp_bitcnt_t)i);
    }
}

/* Caches generator^(2^i) modulo N by repeated squaring. */
static mpz_t *make_modular_powers(const mpz_t generator, const mpz_t modulus, int precision) {
    mpz_t *powers = malloc((size_t)precision * sizeof(*powers));
    if (powers == NULL) fail("could not allocate modular powers");
    for (int i = 0; i < precision; ++i) mpz_init(powers[i]);
    mpz_mod(powers[0], generator, modulus);
    for (int i = 1; i < precision; ++i) {
        mpz_mul(powers[i], powers[i - 1], powers[i - 1]);
        mpz_mod(powers[i], powers[i], modulus);
    }
    return powers;
}

/* Releases a cache of initialized modular powers. */
static void clear_modular_powers(mpz_t *powers, int precision) {
    for (int i = 0; i < precision; ++i) mpz_clear(powers[i]);
    free(powers);
}

/* Runs the sample's iterative phase estimation with semiclassical Fourier feedback. */
static void estimate_frequency(mpz_t frequency, const mpz_t generator, const mpz_t modulus, int bits) {
    Circuit circuit = make_circuit(bits);
    int precision = 2 * bits + 1;
    mpz_t *powers = make_modular_powers(generator, modulus, precision);
    mpz_set_ui(frequency, 0);
    double feedback = 0.0;
    qdk_x(0);
    for (int index = precision - 1; index >= 0; --index) {
        int step = precision - 1 - index;
        qdk_h(circuit.phase);
        modular_multiply(&circuit, modulus, powers[index]);
        // R1Frac and Rz differ only by an irrelevant global phase here.
        qdk_rz(circuit.phase, acos(-1.0) * feedback);
        qdk_h(circuit.phase);
        int bit = measure_reset(circuit.phase);
        if (bit != 0) mpz_setbit(frequency, (mp_bitcnt_t)step);
        // Tracks frequency / 2^(step + 1) without overflowing a floating-point conversion.
        feedback = (feedback + (double)bit) * 0.5;
    }
    for (int i = 0; i < bits; ++i) qdk_reset((size_t)i);
    mpz_t denominator;
    mpz_init_set_ui(denominator, 0);
    mpz_setbit(denominator, (mp_bitcnt_t)precision);
    gmp_printf("Estimated frequency=%Zd/%Zd\n", frequency, denominator);
    mpz_clear(denominator);
    clear_modular_powers(powers, precision);
}

/* Recovers the bounded continued-fraction denominator used by the Q# sample. */
static void period_from_frequency(mpz_t period, const mpz_t modulus, const mpz_t frequency, int precision) {
    if (mpz_sgn(frequency) == 0) {
        mpz_set_ui(period, 1);
        return;
    }
    mpz_t r1, r2, s1, s2, quotient, next;
    mpz_inits(r1, r2, s1, s2, quotient, next, NULL);
    mpz_set(r1, frequency);
    mpz_setbit(r2, (mp_bitcnt_t)precision);
    mpz_set_ui(s1, 1);
    while (mpz_sgn(r2) != 0 && mpz_cmpabs(s2, modulus) <= 0) {
        mpz_tdiv_qr(quotient, next, r1, r2);
        mpz_swap(r1, r2);
        mpz_swap(r2, next);
        mpz_mul(next, quotient, s2);
        mpz_sub(next, s1, next);
        mpz_swap(s1, s2);
        mpz_swap(s2, next);
    }
    mpz_abs(period, mpz_sgn(r2) == 0 && mpz_cmpabs(s2, modulus) <= 0 ? s2 : s1);
    mpz_clears(r1, r2, s1, s2, quotient, next, NULL);
}

/* Attempts the sample's even-period reduction to a nontrivial factor. */
static void factor_from_period(mpz_t factor, const mpz_t number, const mpz_t generator, const mpz_t period) {
    mpz_set_ui(factor, 1);
    if (mpz_odd_p(period)) return;
    mpz_t half, exponent, temp, a, b;
    mpz_inits(half, exponent, temp, a, b, NULL);
    mpz_fdiv_q_2exp(exponent, period, 1);
    mpz_powm(half, generator, exponent, number);
    mpz_sub_ui(temp, number, 1);
    if (mpz_cmp(half, temp) != 0) {
        mpz_sub_ui(temp, half, 1);
        mpz_gcd(a, temp, number);
        mpz_add_ui(temp, half, 1);
        mpz_gcd(b, temp, number);
        mpz_set(temp, mpz_cmp(a, b) > 0 ? a : b);
        if (mpz_cmp_ui(temp, 1) > 0 && mpz_cmp(temp, number) < 0) mpz_set(factor, temp);
    }
    mpz_clears(half, exponent, temp, a, b, NULL);
}

/* Runs one generator attempt, returning 1 when no nontrivial factor is found. */
static void shors_shot(mpz_t factor, const mpz_t number, const mpz_t generator) {
    if (mpz_even_p(number)) {
        mpz_set_ui(factor, 2);
        return;
    }
    int bits = (int)mpz_sizeinbase(number, 2);
    mpz_gcd(factor, generator, number);
    if (mpz_cmp_ui(factor, 1) == 0) {
        mpz_t frequency, period;
        mpz_inits(frequency, period, NULL);
        estimate_frequency(frequency, generator, number, bits);
        period_from_frequency(period, number, frequency, 2 * bits + 1);
        gmp_printf("Estimated period=%Zd\n", period);
        factor_from_period(factor, number, generator, period);
        mpz_clears(frequency, period, NULL);
    } else {
        gmp_printf("Guessed a divisor by accident: %Zd\n", factor);
    }
    if (mpz_cmp_ui(factor, 1) <= 0 || mpz_cmp(factor, number) >= 0) mpz_set_ui(factor, 1);
}

/* Factors an input with quantum order finding and at most 100 generator attempts. */
static void factor_semiprime(mpz_t factor, const mpz_t number, gmp_randstate_t random) {
    if (mpz_even_p(number)) {
        mpz_set_ui(factor, 2);
        return;
    }
    mpz_t generator, range;
    mpz_inits(generator, range, NULL);
    mpz_sub_ui(range, number, 1);
    for (int attempt = 1; attempt <= MAX_ATTEMPTS; ++attempt) {
        // Start with a coprime base for odd inputs so the example exercises quantum order finding.
        if (attempt == 1) mpz_set_ui(generator, 2);
        else {
            mpz_urandomm(generator, random, range);
            mpz_add_ui(generator, generator, 1);
        }
        gmp_printf("Factorizing %Zd, attempt %d, generator=%Zd\n", number, attempt, generator);
        shors_shot(factor, number, generator);
        if (mpz_cmp_ui(factor, 1) > 0 && mpz_cmp(factor, number) < 0) {
            mpz_clears(generator, range, NULL);
            return;
        }
    }
    mpz_clears(generator, range, NULL);
    fail("failed to find factors after 100 attempts");
}

/* Samples an odd probable prime with exactly the requested number of bits. */
static void random_prime(mpz_t prime, int bits, gmp_randstate_t random) {
    do {
        mpz_urandomb(prime, random, (mp_bitcnt_t)bits);
        mpz_setbit(prime, (mp_bitcnt_t)(bits - 1));
        mpz_setbit(prime, 0);
    } while (mpz_probab_prime_p(prime, 25) == 0);
}

/* Prepares a little-endian basis-state register that is initially zero. */
static void prepare_register(const mpz_t value, size_t start, int bits) {
    for (int i = 0; i < bits; ++i) {
        if (mpz_tstbit(value, (mp_bitcnt_t)i) != 0) qdk_x(start + (size_t)i);
    }
}

/* Exercises 2048-bit modular arithmetic and 4097-bit phase postprocessing without a circuit. */
static void test_large_classical_arithmetic(void) {
    mpz_t number, generator, exponent, expected, actual, frequency, period, prime;
    mpz_inits(number, generator, exponent, expected, actual, frequency, period, prime, NULL);
    mpz_setbit(number, MAX_BITS);
    mpz_sub_ui(number, number, 159);
    mpz_set_ui(generator, 2);
    int precision = 2 * MAX_BITS + 1;
    mpz_t *powers = make_modular_powers(generator, number, precision);
    const int indices[] = {64, 1024, 4096};
    for (size_t i = 0; i < sizeof(indices) / sizeof(indices[0]); ++i) {
        mpz_set_ui(exponent, 0);
        mpz_setbit(exponent, (mp_bitcnt_t)indices[i]);
        mpz_powm(expected, generator, exponent, number);
        if (mpz_cmp(expected, powers[indices[i]]) != 0) fail("large modular powers failed");
    }
    clear_modular_powers(powers, precision);
    mpz_sub_ui(actual, number, 1);
    mpz_mul_2exp(expected, actual, 1);
    mpz_mod(expected, expected, number);
    double_mod(actual, number);
    if (mpz_cmp(actual, expected) != 0) fail("large modular doubling failed");

    mpz_setbit(frequency, (mp_bitcnt_t)(precision - 1000));
    period_from_frequency(period, number, frequency, precision);
    mpz_set_ui(expected, 0);
    mpz_setbit(expected, 1000);
    if (mpz_cmp(period, expected) != 0) fail("large continued fractions failed");
    mpz_set_ui(frequency, 0);
    period_from_frequency(period, number, frequency, precision);
    if (mpz_cmp_ui(period, 1) != 0) fail("zero frequency handling failed");

    mpz_set_ui(prime, 0);
    mpz_setbit(prime, MAX_BITS - 2);
    mpz_add_ui(prime, prime, 1);
    mpz_mul_ui(number, prime, 3);
    mpz_mul_ui(generator, prime, 2);
    mpz_add_ui(generator, generator, 1);
    mpz_set_ui(period, 2);
    factor_from_period(actual, number, generator, period);
    if (mpz_cmp(actual, prime) != 0) fail("large period-to-factor reduction failed");

    gmp_randstate_t random;
    gmp_randinit_mt(random);
    gmp_randseed_ui(random, 1);
    random_prime(prime, MAX_BITS / 2, random);
    if (mpz_sizeinbase(prime, 2) != MAX_BITS / 2 || mpz_probab_prime_p(prime, 25) == 0) {
        fail("large prime generation failed");
    }
    gmp_randclear(random);
    mpz_clears(number, generator, exponent, expected, actual, frequency, period, prime, NULL);
}

/* Checks high bits, trailing zeros, controls, comparisons, carry wraparound, and clean workspace. */
static void test_wide_arithmetic(void) {
    const int widths[] = {64, 65, 2048, 2049};
    size_t qubits[ARITHMETIC_MAX_BITS];
    for (size_t i = 0; i < ARITHMETIC_MAX_BITS; ++i) qubits[i] = i;
    mpz_t constant, value, measured, expected;
    mpz_inits(constant, value, measured, expected, NULL);
    for (size_t w = 0; w < sizeof(widths) / sizeof(widths[0]); ++w) {
        int width = widths[w];
        size_t control = (size_t)width, target = control + 1, workspace = control + 2;
        for (int shape = 0; shape < 3; ++shape) {
            mpz_set_ui(constant, shape == 2 ? 1 : 0);
            if (shape != 0) mpz_setbit(constant, (mp_bitcnt_t)(width - 1));
            for (int enabled = 0; enabled <= 1; ++enabled) {
                if (enabled != 0) qdk_x(control);
                inc_by_i(constant, qubits, (size_t)width, control, false, workspace);
                read_register(measured, 0, width);
                if (enabled != 0) mpz_set(expected, constant);
                else mpz_set_ui(expected, 0);
                if (mpz_cmp(measured, expected) != 0) fail("wide controlled addition failed");
                prepare_register(measured, 0, width);
                inc_by_i(constant, qubits, (size_t)width, control, true, workspace);
                read_register(measured, 0, width);
                if (mpz_sgn(measured) != 0) fail("wide adjoint addition failed");
                if (measure_reset(control) != enabled) fail("wide addition changed its control");
            }
            for (int offset = -1; offset <= 1; ++offset) {
                if (offset < 0 && mpz_sgn(constant) == 0) continue;
                if (offset < 0) mpz_sub_ui(value, constant, 1);
                else mpz_add_ui(value, constant, (unsigned long)offset);
                for (int enabled = 0; enabled <= 1; ++enabled) {
                    prepare_register(value, 0, width);
                    if (enabled != 0) qdk_x(control);
                    apply_if_less_or_equal_l(constant, qubits, (size_t)width, target, control, workspace);
                    if (measure_reset(target) != (enabled != 0 && offset >= 0)) {
                        fail("wide comparison failed");
                    }
                    if (measure_reset(control) != enabled) fail("wide comparison changed its control");
                    read_register(measured, 0, width);
                    if (mpz_cmp(measured, value) != 0) fail("wide comparison changed its input");
                }
            }
            for (int i = 0; i < width; ++i) {
                if (measure_reset(workspace + (size_t)i) != 0) fail("wide arithmetic left dirty workspace");
            }
        }
        mpz_set_ui(value, 0);
        mpz_setbit(value, (mp_bitcnt_t)width);
        mpz_sub_ui(value, value, 1);
        prepare_register(value, 0, width);
        inc_by_i(constant, qubits, (size_t)width, ARITHMETIC_NO_CONTROL, false, workspace);
        read_register(measured, 0, width);
        mpz_sub_ui(expected, constant, 1);
        if (mpz_cmp(measured, expected) != 0) fail("wide addition wraparound failed");
        prepare_register(measured, 0, width);
        inc_by_i(constant, qubits, (size_t)width, ARITHMETIC_NO_CONTROL, true, workspace);
        read_register(measured, 0, width);
        if (mpz_cmp(measured, value) != 0) fail("wide adjoint wraparound failed");
        mpz_add_ui(constant, value, 1);
        apply_if_less_or_equal_l(constant, qubits, (size_t)width, target, ARITHMETIC_NO_CONTROL, workspace);
        if (measure_reset(target) != 0) fail("oversized comparison constant was truncated");
        for (int i = 0; i < width; ++i) {
            if (measure_reset(workspace + (size_t)i) != 0) fail("wide wraparound left dirty workspace");
        }
    }
    mpz_clears(constant, value, measured, expected, NULL);
}

/* Verifies all arithmetic work qubits are restored rather than hiding garbage by resetting it. */
static void check_clean(const Circuit *circuit) {
    for (size_t qubit = 0; qubit < (size_t)(3 * circuit->bits + 4); ++qubit) {
        if (measure_reset(qubit) != 0) fail("arithmetic left a dirty ancilla");
    }
}

/* Exhaustively tests controlled modular arithmetic on small basis-state inputs. */
static void self_test(void) {
    mpz_t n, c, measured, frequency, period, factor;
    mpz_inits(n, c, measured, frequency, period, factor, NULL);
    for (int64_t modulus = 3; modulus <= 7; ++modulus) {
        mpz_set_ui(n, (unsigned long)modulus);
        int bits = modulus < 4 ? 2 : 3;
        Circuit circuit = make_circuit(bits);
        for (int64_t constant = 0; constant < modulus; ++constant) {
            mpz_set_ui(c, (unsigned long)constant);
            for (int64_t value = 0; value < modulus; ++value) {
                for (int enabled = 0; enabled <= 1; ++enabled) {
                    for (int i = 0; i < bits; ++i) {
                        if (((value >> i) & 1) != 0) qdk_x((size_t)(bits + i));
                    }
                    if (enabled != 0) qdk_x(circuit.enable);
                    modular_add(&circuit, n, c);
                    int64_t expected = enabled != 0 ? (value + constant) % modulus : value;
                    read_register(measured, (size_t)bits, bits);
                    if (mpz_cmp_ui(measured, (unsigned long)expected) != 0) fail("modular addition failed");
                    if (measure_reset(circuit.enable) != enabled) fail("addition changed its control");
                    check_clean(&circuit);

                    if (mpz_gcd_ui(NULL, n, (unsigned long)constant) != 1) continue;
                    for (int i = 0; i < bits; ++i) {
                        if (((value >> i) & 1) != 0) qdk_x((size_t)i);
                    }
                    if (enabled != 0) qdk_x(circuit.phase);
                    modular_multiply(&circuit, n, c);
                    expected = enabled != 0 ? value * constant % modulus : value;
                    read_register(measured, 0, bits);
                    if (mpz_cmp_ui(measured, (unsigned long)expected) != 0) fail("modular multiplication failed");
                    if (measure_reset(circuit.phase) != enabled) fail("multiplication changed its control");
                    check_clean(&circuit);
                }
            }
        }
        qdk_x((size_t)bits);
        qdk_h(circuit.enable);
        mpz_sub_ui(c, n, 1);
        modular_add(&circuit, n, c);
        mpz_set_ui(c, 1);
        modular_add(&circuit, n, c);
        qdk_h(circuit.enable);
        read_register(measured, (size_t)bits, bits);
        if (mpz_cmp_ui(measured, 1) != 0) fail("coherent addition did not invert");
        check_clean(&circuit);

        qdk_x(0);
        qdk_h(circuit.phase);
        mpz_sub_ui(c, n, 1);
        modular_multiply(&circuit, n, c);
        modular_multiply(&circuit, n, c);
        qdk_h(circuit.phase);
        read_register(measured, 0, bits);
        if (mpz_cmp_ui(measured, 1) != 0) fail("coherent multiplication did not invert");
        check_clean(&circuit);
    }
    mpz_set_ui(n, 15);
    mpz_set_ui(frequency, 32);
    period_from_frequency(period, n, frequency, 7);
    if (mpz_cmp_ui(period, 4) != 0) fail("continued fractions failed");
    mpz_set_ui(c, 2);
    factor_from_period(factor, n, c, period);
    if (mpz_cmp_ui(factor, 5) != 0) fail("period-to-factor reduction failed");
    mpz_clears(n, c, measured, frequency, period, factor, NULL);
    test_large_classical_arithmetic();
    test_wide_arithmetic();
    puts("Arithmetic self-test passed.");
}

/* Reads the integer count from the pretty-printed JSON produced by this profiler. */
static uint64_t read_operation_count(const char *path) {
    FILE *file = fopen(path, "r");
    if (file == NULL) fail("could not open saved profile");
    const char key[] = "\"total_operations_count\"";
    char line[512];
    bool found = false;
    uintmax_t count = 0;
    while (fgets(line, sizeof(line), file) != NULL) {
        char *field = line + strspn(line, " \t\r\n");
        if (strncmp(field, key, sizeof(key) - 1) != 0) continue;
        field += sizeof(key) - 1;
        field += strspn(field, " \t");
        if (*field++ != ':') fail("invalid operation count in saved profile");
        field += strspn(field, " \t");
        if (*field < '0' || *field > '9') fail("invalid operation count in saved profile");
        char *end = NULL;
        errno = 0;
        count = strtoumax(field, &end, 10);
        end += strspn(end, " \t\r\n");
        if (errno != 0 || count > UINT64_MAX || (*end != ',' && *end != '}')) {
            fail("invalid operation count in saved profile");
        }
        found = true;
        break;
    }
    if (ferror(file)) fail("could not read saved profile");
    if (fclose(file) != 0) fail("could not close saved profile");
    if (!found) fail("saved profile is missing total_operations_count");
    return (uint64_t)count;
}

/* Profiles one shot on a random semiprime, saves JSON, and reports time per operation. */
static void profile_shors_shot(int bit_len) {
    char path[64];
    int length = snprintf(path, sizeof(path), "source/qep/profile_viewer/profiles/shor_c_%d.json", bit_len);
    if (length < 0 || (size_t)length >= sizeof(path)) fail("could not format profile path");
    gmp_randstate_t random;
    gmp_randinit_mt(random);
    gmp_randseed_ui(random, (unsigned long)time(NULL));
    mpz_t p, q, number, generator, factor;
    mpz_inits(p, q, number, generator, factor, NULL);
    random_prime(p, bit_len / 2, random);
    random_prime(q, bit_len / 2, random);
    mpz_mul(number, p, q);
    mpz_set_ui(generator, 2);
    gmp_randclear(random);
    gmp_printf("Profiling %Zd = %Zd * %Zd (%d-bit primes), generator=2\n", number, p, q, bit_len / 2);
    qdk_init("counting_profiler");
    struct timespec start, end;
    if (clock_gettime(CLOCK_MONOTONIC, &start) != 0) fail("could not start profiling timer");
    shors_shot(factor, number, generator);
    if (clock_gettime(CLOCK_MONOTONIC, &end) != 0) fail("could not stop profiling timer");
    double elapsed_ns = (double)(end.tv_sec - start.tv_sec) * 1e9 +
                        (double)(end.tv_nsec - start.tv_nsec);
    qdk_save_profile(path);
    uint64_t count = read_operation_count(path);
    if (count == 0) fail("profile counted no operations");
    puts("Profiling measurements are bogus; the factorization result is ignored.");
    printf("Total operations count: %" PRIu64 "\n", count);
    printf("Profiling time: %.9f s\n", elapsed_ns / 1e9);
    printf("Profiling speed: %.3f ns/op\n", elapsed_ns / (double)count);
    mpz_clears(p, q, number, generator, factor, NULL);
}

/* CLI modes (all arguments are required exactly as shown):
   --self-test       Run arithmetic regression tests on sparse_sim; no extra arguments.
   --factor <number> Factor a decimal integer in [4, 2^2048) on sparse_sim, retrying as needed.
                      Large inputs are representable but generally infeasible to simulate.
   --profile <bitlen> Profile one shot on counting_profiler using two random primes,
                      each of bitlen/2 bits; bitlen must be even and in [4, 2048].
                      Save to source/qep/profile_viewer/profiles/shor_c_<bitlen>.json
                      and print operation count, elapsed
                      shot time, and ns/op. Bogus profiling measurements are ignored.
   Profiling timing excludes initialization, prime generation, and saving the file. */
int main(int argc, char **argv) {
    if (argc == 2 && strcmp(argv[1], "--self-test") == 0) {
        qdk_init("sparse_sim");
        self_test();
        return EXIT_SUCCESS;
    }
    if (argc != 3) fail("usage: shor --self-test | --factor <number> | --profile <bitlen>");
    if (strcmp(argv[1], "--factor") == 0) {
        mpz_t number, factor, cofactor;
        mpz_inits(number, factor, cofactor, NULL);
        const char *text = argv[2];
        if (*text == '+') ++text;
        if (*text == '\0' || strspn(text, "0123456789") != strlen(text) ||
            mpz_set_str(number, text, 10) != 0) fail("number must be a nonnegative decimal integer");
        if (mpz_cmp_ui(number, 4) < 0 || mpz_sizeinbase(number, 2) > MAX_BITS) {
            fail("number must be in [4, 2^2048)");
        }
        gmp_randstate_t random;
        gmp_randinit_mt(random);
        gmp_randseed_ui(random, (unsigned long)time(NULL));
        qdk_init("sparse_sim");
        factor_semiprime(factor, number, random);
        mpz_divexact(cofactor, number, factor);
        gmp_printf("Found factorization %Zd = %Zd * %Zd\n", number, factor, cofactor);
        gmp_randclear(random);
        mpz_clears(number, factor, cofactor, NULL);
        return EXIT_SUCCESS;
    }
    if (strcmp(argv[1], "--profile") == 0) {
        char *parsed_end = NULL;
        errno = 0;
        long parsed = strtol(argv[2], &parsed_end, 10);
        if (errno != 0 || parsed_end == argv[2] || *parsed_end != '\0' ||
            parsed < 4 || parsed > MAX_BITS || parsed % 2 != 0) {
            fail("bit length must be an even integer in [4, 2048]");
        }
        profile_shors_shot((int)parsed);
        return EXIT_SUCCESS;
    }
    fail("usage: shor --self-test | --factor <number> | --profile <bitlen>");
}
