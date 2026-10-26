// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#ifndef QEP_EXAMPLE_ARITHMETIC_H
#define QEP_EXAMPLE_ARITHMETIC_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define ARITHMETIC_NO_CONTROL SIZE_MAX
enum { ARITHMETIC_MAX_BITS = 63 };

/* Ports Std.Arithmetic.IncByI and its adjoint, with zero or one control.
   Adds/subtracts a nonnegative constant smaller than 2^width modulo 2^width.
   targets is little-endian; width must be between 1 and ARITHMETIC_MAX_BITS.
   workspace starts width clean qubits, disjoint from targets and control.
   Workspace qubits are restored to zero, not reset to hide garbage. */
void inc_by_i(
    int64_t constant, const size_t *targets, size_t width,
    size_t control, bool adjoint, size_t workspace
);

/* Ports Std.Arithmetic.ApplyIfLessOrEqualL specialized to action X.
   Flips target when the nonnegative constant <= the little-endian register,
   additionally conditioned on control unless it is ARITHMETIC_NO_CONTROL.
   Uses the adaptive Q# AND uncomputation (measurement and conditional CZ).
   The register, target, control, and width clean workspace qubits must be
   disjoint; workspace is restored to zero. Supports the same widths as above.
   Constants are limited to int64_t, sufficient for the Shor example. */
void apply_if_less_or_equal_l(
    int64_t constant, const size_t *register_qubits, size_t width,
    size_t target, size_t control, size_t workspace
);

#endif
