// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Diagnostics.Fact;

import AddConst.AddConstant;
import Modular.ModAdd.ModAdd;
import ResourceEstimation.LoopA;
import Utils.RotateLeft;

/// # Summary
/// Computes `x := (2 * x) % modulus` in place.
/// Requires an odd modulus satisfying `0 < modulus < 2^Length(x)`.
///
/// # Reference
/// - [1](https://arxiv.org/abs/1706.06752) "Quantum resource estimates for computing
///   elliptic curve discrete logarithms", Martin Roetteler, Michael Naehrig, Krysta M.
///   Svore, Kristin Lauter. (Fig. 4).
///
/// # Input
/// ## x
/// Register storing `x`, updated in place.
/// ## modulus
/// Odd classical modulus satisfying `0 < modulus < 2^Length(x)`.
operation ModDouble(x : Qubit[], modulus : BigInt) : Unit is Adj {
    let n = Length(x);
    Fact(modulus % 2L == 1L, "Modulus must be odd.");
    Fact(modulus > 0L and modulus < (1L <<< n), "Modulus must satisfy 0 < modulus < 2^Length(x).");
    use carry = Qubit();

    // Step 1. Multiply by 2 by bit shift.
    RotateLeft(x + [carry]);

    // Step 2. "-p".
    AddConstant((1L <<< n) - modulus, x + [carry]);
    X(carry);

    // Step 3. Controlled "+p".
    Controlled AddConstant([carry], (modulus, x));

    // Step 4. Uncompute carry (with CNOT).
    // The result 2x mod p is odd iff p was subtracted (since p is odd).
    CNOT(x[0], carry);
    X(carry);
}

/// # Summary
/// Computes `(x, y, ans) -> (x, y, ((ans << (n - 1)) + x * y) % modulus)`.
/// Assumes `ans` is initialized to `0` for multiplication semantics.
/// Requires an odd modulus satisfying `0 < modulus < 2^Length(x)`.
///
/// # Reference
/// - [1](https://arxiv.org/abs/1706.06752) "Quantum resource estimates for computing
///   elliptic curve discrete logarithms", Martin Roetteler, Michael Naehrig, Krysta M.
///   Svore, Kristin Lauter. (Fig. 5).
///
/// # Input
/// ## x
/// First multiplicand register.
/// ## y
/// Second multiplicand register.
/// ## ans
/// Accumulator register updated in place.
/// ## modulus
/// Odd classical modulus satisfying `0 < modulus < 2^Length(x)`.
operation ModMul(x : Qubit[], y : Qubit[], ans : Qubit[], modulus : BigInt) : Unit is Adj {
    let n = Length(x);
    Fact(Length(y) == n, "Registers must have the same length.");
    Fact(Length(ans) == n, "Registers must have the same length.");
    Fact(modulus % 2L == 1L, "Modulus must be odd.");
    Fact(modulus > 0L and modulus < (1L <<< n), "Modulus must satisfy 0 < modulus < 2^Length(x).");

    LoopA(n, idx => {
        Controlled ModAdd([x[n - 1 - idx]], (y, ans, modulus));
        if idx != n - 1 {
            ModDouble(ans, modulus);
        }
    });
}

/// # Summary
/// Computes `(x, ans) -> (x, ((ans << (n - 1)) + x * x) % modulus)`.
/// Requires an odd modulus satisfying `0 < modulus < 2^Length(x)`.
///
/// # Reference
/// - [1](https://arxiv.org/abs/1706.06752) "Quantum resource estimates for computing
///   elliptic curve discrete logarithms", Martin Roetteler, Michael Naehrig, Krysta M.
///   Svore, Kristin Lauter. (Fig. 6).
///
/// # Input
/// ## x
/// Register storing the value to be squared.
/// ## ans
/// Accumulator register updated in place.
/// ## modulus
/// Odd classical modulus satisfying `0 < modulus < 2^Length(x)`.
operation ModSquare(x : Qubit[], ans : Qubit[], modulus : BigInt) : Unit is Adj {
    let n = Length(x);
    Fact(Length(ans) == n, "Registers must have the same length.");
    Fact(modulus % 2L == 1L, "Modulus must be odd.");
    Fact(modulus > 0L and modulus < (1L <<< n), "Modulus must satisfy 0 < modulus < 2^Length(x).");
    use x_copy = Qubit();

    LoopA(n, idx => {
        let bit_idx = n - 1 - idx;
        CNOT(x[bit_idx], x_copy);
        Controlled ModAdd([x_copy], (x, ans, modulus));
        CNOT(x[bit_idx], x_copy);
        if bit_idx != 0 {
            ModDouble(ans, modulus);
        }
    });
}

export ModDouble, ModMul, ModSquare;
