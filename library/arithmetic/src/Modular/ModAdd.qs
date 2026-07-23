// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Diagnostics.Fact;

import Add.Add;
import Add.Subtract;
import AddConst.AddConstant;
import Compare.CompareGT;

// Modular addition: y = (x+y) % modulus.
// Uses measurement-based uncomputation, which makes it non-adjointable.
operation _ModAddMBU(x : Qubit[], y : Qubit[], modulus : BigInt) : Unit {
    body (...) {
        Controlled _ModAddMBU([], (x, y, modulus));
    }
    controlled (controls, ...) {
        let n = Length(x);
        Fact(Length(y) == n, "Input size mismatch.");
        Fact(modulus >= 2L, "Modulus must be at least 2.");
        Fact(modulus < (1L <<< n), "Modulus is too large for number of bits.");

        use carry = Qubit();

        // Step 1. Add.
        Controlled Add(controls, (x, y + [carry]));

        // Step 2. "-p".
        AddConstant((1L <<< n) - modulus, y + [carry]);
        X(carry);

        // Step 3. Controlled "+p".
        Controlled AddConstant([carry], (modulus, y));

        // Step 4. Uncompute carry with comparator (measurement-based).
        H(carry);
        let qflag = M(carry);
        if (qflag == One) {
            H(carry);
            Controlled CompareGT(controls, (x, y, carry));
            H(carry);
            X(carry);
        }
    }
}

// Computes (x, y) := x, ((x+y)%p).
// See Roetteler et al. (2017), Fig. 3.
operation _ModAddNoMBU(x : Qubit[], y : Qubit[], modulus : BigInt) : Unit is Ctl + Adj {
    body (...) {
        Controlled _ModAddNoMBU([], (x, y, modulus));
    }
    controlled (controls, ...) {
        let n = Length(x);
        Fact(Length(y) == n, "Registers must have the same length");
        use carry = Qubit();

        // Step 1. Add.
        Controlled Add(controls, (x, y + [carry]));

        // Step 2. "-p".
        AddConstant((1L <<< n) - modulus, y + [carry]);
        X(carry);

        // Step 3. Controlled "+p".
        Controlled AddConstant([carry], (modulus, y));

        // Step 4. Uncompute carry with comparator.
        Controlled CompareGT(controls, (x, y, carry));
        X(carry);
    }
}

/// # Summary
/// In-place modular addition of two equal-length little-endian registers.
///
/// Given registers `x` and `y` encoding integers with `0 <= x, y < modulus`,
/// this operation computes `(x, y) -> (x, (x + y) mod modulus)`.
///
/// # References
/// - [1](https://arxiv.org/abs/1706.06752) "Quantum resource estimates for computing 
///   elliptic curve discrete logarithms", Martin Roetteler, Michael Naehrig, Krysta M. 
///   Svore, Kristin Lauter. (Fig. 3).
/// - [2](https://arxiv.org/abs/2407.20167) "Measurement-based uncomputation of quantum 
///   circuits for modular arithmetic", Alessandro Luongo, Antonio Michele Miti, 
///   Varun Narasimhachar, Adithya Sireesh.
///
/// # Input
/// ## x
/// Addend register. This register is preserved.
/// ## y
/// Accumulator register. This register is updated in place.
/// ## modulus
/// Classical modulus. Must satisfy `2 <= modulus < 2^Length(x)`.
operation ModAdd(x : Qubit[], y : Qubit[], modulus : BigInt) : Unit is Adj + Ctl {
    body (...) {
        _ModAddMBU(x, y, modulus);
    }
    adjoint (...) {
        Adjoint _ModAddNoMBU(x, y, modulus);
    }
}


export ModAdd;
