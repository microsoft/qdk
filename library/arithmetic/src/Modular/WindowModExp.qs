// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Q# operations for windowed modular exponentiation.

import Std.Arrays.Mapped;
import Std.Convert.IntAsBigInt;
import Std.Diagnostics.Fact;
import Std.Math;
import Std.Math.*;

import AddLookup.ModAddLookup;
import ClassicalMath.*;
import ResourceEstimation.LoopA;
import Utils.ParallelSWAP;

// References:
// - Craig Gidney, "Windowed quantum arithmetic", 2019.
//   https://arxiv.org/abs/1905.07682
// - Craig Gidney, "How to factor 2048 bit RSA integers with less than a million
//   noisy qubits", 2025.
//   https://arxiv.org/abs/2505.15917


/// Computes powers of base: base_pows[i] = base^i % modulus.
function ComputeBasePowers(base : BigInt, modulus : BigInt, num_powers : Int) : BigInt[] {
    mutable base_pows = Repeated(0L, num_powers);
    base_pows[0] = 1L;
    let base_mod = base % modulus;
    for a in 1..num_powers - 1 {
        base_pows[a] = (base_pows[a - 1] * base_mod) % modulus;
    }
    return base_pows;
}

// Generates a modular-exponentiation lookup table.
// For each a in [0,num_a), b in [0, num_b):
// data[(b * num_a) + a] = (factor * b * base^a * sign) % modulus.
function ModExpLookupTable(
    factor : BigInt,
    exp_length : Int,
    mul_length : Int,
    base : BigInt,
    modulus : BigInt,
    sign : BigInt
) : BigInt[] {
    let num_a = 1 <<< exp_length;
    let num_b = 1 <<< mul_length;
    let base_pows = ComputeBasePowers(base, modulus, num_a);

    mutable data = Repeated(0L, num_a * num_b);
    for b in 0..num_b - 1 {
        let fb : BigInt = SafeMod(sign * factor * IntAsBigInt(b), modulus);
        for a in 0..num_a - 1 {
            data[(b * num_a) + a] = (fb * base_pows[a]) % modulus;
        }
    }
    return data;
}

/// Internal helper for windowed modular multiplication.
operation WindowModularMultiply(
    q_exponent_window : Qubit[],
    q_source : Qubit[],
    q_target : Qubit[],
    multiply_window_size : Int,
    adjusted_base : BigInt,
    modulus : BigInt,
    sign : BigInt
) : Unit is Adj {
    let num_multiply_windows = (Length(q_source) + multiply_window_size - 1) / multiply_window_size;

    LoopA(num_multiply_windows, j => {
        let window_start = j * multiply_window_size;
        let window_end = Math.MinI(window_start + multiply_window_size, Length(q_source));
        let q_multiply_window = q_source[window_start..window_end - 1];

        let data = ModExpLookupTable(
            1L <<< window_start,
            Length(q_exponent_window),
            Length(q_multiply_window),
            adjusted_base,
            modulus,
            sign
        );

        let q_address = q_exponent_window + q_multiply_window;
        ModAddLookup(q_address, q_target, data, modulus);
    });
}

/// # Summary
/// Windowed modular exponentiation over a little-endian exponent register.
///
/// Computes `q_target := (q_target * base^exponent)%modulus`
/// using alternating forward and inverse windowed modular-multiply updates.
///
/// Follows Gidney (2019), Section 3.5.
///
/// # Input
/// ## q_target
/// Target register that is multiplied in place by `base^exponent (mod modulus)`.
/// ## q_exponent
/// Little-endian exponent register controlling the modular exponentiation.
/// ## base
/// Classical base of the exponentiation.
/// ## modulus
/// Classical modulus used for all modular arithmetic.
/// ## multiply_window_size
/// Number of source-register bits used per multiplication lookup window.
/// ## exponent_window_size
/// Number of exponent bits used per exponentiation window.
operation WindowModularExp(
    q_target : Qubit[],
    q_exponent : Qubit[],
    base : BigInt,
    modulus : BigInt,
    multiply_window_size : Int,
    exponent_window_size : Int
) : Unit {
    let result_size = Length(q_target);
    let exponent_size = Length(q_exponent);
    Fact(multiply_window_size <= result_size, "multiply_window_size too large");
    Fact(exponent_window_size <= exponent_size, "exponent_window_size too large");

    let num_exponent_windows = DivCeil(exponent_size, exponent_window_size);
    let base_sqs = ComputeSequentialSquares(base, modulus, exponent_size);
    use q_minus_reg = Qubit[result_size];

    LoopA(num_exponent_windows, window_idx => {
        let i = window_idx * exponent_window_size;
        let window_end = Math.MinI(i + exponent_window_size, exponent_size);
        let q_exponent_window = q_exponent[i..window_end - 1];
        let adjusted_base = base_sqs[i];  // base^(2^i) % modulus.

        // Determine q_plus and q_minus based on window parity.
        let (q_plus, q_minus) = if (window_idx % 2 == 0) {
            (q_target, q_minus_reg)
        } else {
            (q_minus_reg, q_target)
        };

        // Forward pass: q_minus += (q_plus * adjusted_base) % modulus.
        WindowModularMultiply(
            q_exponent_window,
            q_plus,
            q_minus,
            multiply_window_size,
            adjusted_base,
            modulus,
            1L
        );

        // Inverse pass: q_plus -= q_minus * inv(adjusted_base) (mod modulus).
        let adjusted_base_inv = Math.InverseModL(adjusted_base, modulus);
        WindowModularMultiply(
            q_exponent_window,
            q_minus,
            q_plus,
            multiply_window_size,
            adjusted_base_inv,
            modulus,
            -1L
        );
    });

    // If num_exponent_windows is odd, swap q_minus_reg back to q_target.
    if (num_exponent_windows % 2 == 1) {
        ParallelSWAP(q_minus_reg, q_target);
    }
}

export ComputeBasePowers, WindowModularMultiply, WindowModularExp;