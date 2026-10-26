// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#include "arithmetic.h"
#include "qep_interop.h"

#include <stdio.h>
#include <stdlib.h>

/* Reports invalid arithmetic arguments or backend outcomes. */
static void fail(const char *message) {
    fprintf(stderr, "arithmetic: %s\n", message);
    exit(EXIT_FAILURE);
}

/* Checks the representable constants, register widths, and workspace range. */
static void validate_arguments(
    int64_t constant, const size_t *qubits, size_t width, size_t workspace
) {
    if (constant < 0) fail("constant must be nonnegative");
    if (qubits == NULL) fail("register must not be null");
    if (width == 0 || width > ARITHMETIC_MAX_BITS) fail("register width must be between 1 and 63");
    if (workspace > SIZE_MAX - width) fail("workspace qubit IDs overflow");
}

/* Applies X with the optional external control. */
static void controlled_x(size_t control, size_t target) {
    if (control == ARITHMETIC_NO_CONTROL) qdk_x(target);
    else qdk_cx(control, target);
}

/* Applies CNOT with the optional external control. */
static void controlled_cnot(size_t control, size_t source, size_t target) {
    if (control == ARITHMETIC_NO_CONTROL) qdk_cx(source, target);
    else qdk_ccx(control, source, target);
}

/* Ports ApplyOuterTTKAdder and its inverse; conjugation stays uncontrolled. */
static void outer_ttk_adder(const size_t *xs, const size_t *ys, size_t count, bool adjoint) {
    if (!adjoint) {
        for (size_t i = 1; i < count; ++i) qdk_cx(xs[i], ys[i]);
        for (size_t i = count - 1; i > 1; --i) qdk_cx(xs[i - 1], xs[i]);
    } else {
        for (size_t i = 1; i + 1 < count; ++i) qdk_cx(xs[i], xs[i + 1]);
        for (size_t i = count; i > 1; --i) qdk_cx(xs[i - 1], ys[i - 1]);
    }
}

/* Ports ApplyInnerTTKAdderNoCarry's custom controlled body and its inverse. */
static void inner_ttk_adder(
    const size_t *xs, const size_t *ys, size_t count, size_t control, bool adjoint
) {
    if (!adjoint) {
        for (size_t i = 0; i + 1 < count; ++i) qdk_ccx(xs[i], ys[i], xs[i + 1]);
        for (size_t i = count - 1; i > 0; --i) {
            controlled_cnot(control, xs[i], ys[i]);
            qdk_ccx(xs[i - 1], ys[i - 1], xs[i]);
        }
    } else {
        for (size_t i = 1; i < count; ++i) {
            qdk_ccx(xs[i - 1], ys[i - 1], xs[i]);
            controlled_cnot(control, xs[i], ys[i]);
        }
        for (size_t i = count - 1; i > 0; --i) qdk_ccx(xs[i - 1], ys[i - 1], xs[i]);
    }
}

/* Ports the equal-width RippleCarryTTKIncByLE path used by IncByI. */
static void ripple_carry_ttk_inc_by_le(
    const size_t *xs, const size_t *ys, size_t count, size_t control, bool adjoint
) {
    if (adjoint) controlled_cnot(control, xs[0], ys[0]);
    if (count > 1) {
        outer_ttk_adder(xs, ys, count, false);
        inner_ttk_adder(xs, ys, count, control, adjoint);
        outer_ttk_adder(xs, ys, count, true);
    }
    if (!adjoint) controlled_cnot(control, xs[0], ys[0]);
}

/* Prepares a constant register, runs the TTK adder, and unprepares it. */
void inc_by_i(
    int64_t constant, const size_t *targets, size_t width,
    size_t control, bool adjoint, size_t workspace
) {
    validate_arguments(constant, targets, width, workspace);
    if ((uint64_t)constant >= (UINT64_C(1) << width)) fail("constant does not fit the register");
    if (constant == 0) return;

    uint64_t normalized = (uint64_t)constant;
    size_t trailing_zeros = 0;
    while ((normalized & 1) == 0) {
        normalized >>= 1;
        ++trailing_zeros;
    }
    size_t count = width - trailing_zeros;
    size_t xs[ARITHMETIC_MAX_BITS] = {0};
    for (size_t i = 0; i < count; ++i) {
        xs[i] = workspace + i;
        if (((normalized >> i) & 1) != 0) qdk_x(xs[i]);
    }
    ripple_carry_ttk_inc_by_le(xs, targets + trailing_zeros, count, control, adjoint);
    // ApplyXorInPlace is adjoint self, including the ascending gate order.
    for (size_t i = 0; i < count; ++i) {
        if (((normalized >> i) & 1) != 0) qdk_x(xs[i]);
    }
}

/* Ports the adaptive Adjoint AND, including its measurement correction. */
static void uncompute_and(size_t a, size_t b, size_t target) {
    qdk_h(target);
    size_t result = qdk_mz_reset(target);
    if (result > 1) fail("backend returned a measurement error");
    if (result == 1) qdk_cz(a, b);
}

/* Ports AND or ApplyOrAssuming0Target and its adjoint on a clean target. */
static void comparison_step(size_t a, size_t b, size_t target, bool is_and, bool adjoint) {
    if (!is_and) {
        qdk_x(a);
        qdk_x(b);
    }
    if (adjoint && !is_and) qdk_x(target);
    if (adjoint) uncompute_and(a, b, target);
    else qdk_ccx(a, b, target);
    if (!adjoint && !is_and) qdk_x(target);
    if (!is_and) {
        qdk_x(b);
        qdk_x(a);
    }
}

/* Computes the library's comparison chain, controls X, then uncomputes it. */
void apply_if_less_or_equal_l(
    int64_t constant, const size_t *register_qubits, size_t width,
    size_t target, size_t control, size_t workspace
) {
    validate_arguments(constant, register_qubits, width, workspace);
    if (constant == 0) {
        controlled_x(control, target);
        return;
    }
    if ((uint64_t)constant >= (UINT64_C(1) << width)) return;

    uint64_t normalized = (uint64_t)constant;
    size_t trailing_zeros = 0;
    while ((normalized & 1) == 0) {
        normalized >>= 1;
        ++trailing_zeros;
    }
    const size_t *xs = register_qubits + trailing_zeros;
    size_t count = width - trailing_zeros - 1;
    for (size_t i = 0; i < count; ++i) {
        size_t previous = i == 0 ? xs[0] : workspace + i - 1;
        bool is_and = ((normalized >> (i + 1)) & 1) != 0;
        comparison_step(previous, xs[i + 1], workspace + i, is_and, false);
    }
    size_t predicate = count == 0 ? register_qubits[width - 1] : workspace + count - 1;
    controlled_cnot(control, predicate, target);
    for (size_t i = count; i > 0; --i) {
        size_t index = i - 1;
        size_t previous = index == 0 ? xs[0] : workspace + index - 1;
        bool is_and = ((normalized >> (index + 1)) & 1) != 0;
        comparison_step(previous, xs[index + 1], workspace + index, is_and, true);
    }
}
