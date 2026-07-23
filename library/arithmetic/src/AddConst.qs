// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arithmetic.RippleCarryCGAddLE;
import Std.Arithmetic.RippleCarryCGIncByLE;
import Std.Convert.BigIntAsBoolArray;
import Std.Math.TrailingZeroCountL;

import ClassicalMath.SafeMod;

/// # Summary
/// Computes `input := (input + constant) % 2^Length(input)`.
///
/// # Reference
/// - [1](https://arxiv.org/pdf/2007.07391) "Compilation of Fault-Tolerant Quantum
///   Heuristics for Combinatorial Optimization", Sanders et al. (Fig. 18).
///
/// # Resources
/// Uses n-1 auxiliary qubits and 2n-2 Toffoli gates.
///
/// # Input
/// ## constant
/// Classical constant to add.
/// ## inp
/// Target register updated in place.
operation AddConstantSanders(constant : BigInt, inp : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled AddConstantSanders([], (constant, inp));
    }
    controlled (ctrl, ...) {
        let n = Length(inp);
        let constant_bits = BigIntAsBoolArray(constant, n);

        if n == 1 {
            // Base case: single qubit addition.
            if (constant_bits[0]) { Controlled X(ctrl, (inp[0])); }
        } else {
            use ancillas = Qubit[n - 1];

            if (constant_bits[0]) { CNOT(inp[0], ancillas[0]); }

            for i in 1..n - 2 {
                let j = i - 1;
                Controlled CNOT(ctrl, (ancillas[j], inp[i]));
                within {
                    if (constant_bits[i]) { X(ancillas[j]); }
                } apply {
                    AND(ancillas[j], inp[i], ancillas[i]);
                }
                CNOT(ancillas[j], ancillas[i]);
            }

            Controlled CNOT(ctrl, (ancillas[n - 2], inp[n - 1]));

            for i in n - 2..-1..1 {
                let j = i - 1;
                CNOT(ancillas[j], ancillas[i]);
                within {
                    if (constant_bits[i]) { X(ancillas[j]); }
                } apply {
                    Adjoint AND(ancillas[j], inp[i], ancillas[i]);
                }
            }

            if (constant_bits[0]) { CNOT(inp[0], ancillas[0]); }

            for i in 0..n - 1 {
                if (constant_bits[i]) { Controlled X(ctrl, (inp[i])); }
            }
        }
    }
}

/// # Summary
/// Constant adder using the Gidney ripple-carry adder.
///
/// # Reference
/// - [1](https://arxiv.org/abs/1709.06648) "Halving the cost of quantum addition", 
///   Craig Gidney.
operation AddConstantUsingCGAdd(constant : BigInt, input : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled AddConstantUsingCGAdd([], (constant, input));
    }
    controlled (ctrl, ...) {
        use anc = Qubit[Length(input)];
        within {
            Controlled ApplyXorInPlaceL(ctrl, (constant, anc));
        } apply {
            RippleCarryCGIncByLE(anc, input);
        }
    }
}

/// # Summary
/// Computes `input := (input + constant) % 2^Length(input)`.
///
/// # Input
/// ## constant
/// Classical constant to add.
/// ## input
/// Target register updated in place.
operation AddConstant(constant : BigInt, input : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled AddConstant([], (constant, input));
    }
    controlled (ctrl, ...) {
        let n = Length(input);
        let constant = SafeMod(constant, 1L <<< n);
        if (constant != 0L) {
            let tz = TrailingZeroCountL(constant);
            if (Std.Core.ConfigValue("minimize_qubits", true) or Length(ctrl) == 0) {
                Controlled AddConstantSanders(ctrl, (constant >>> tz, input[tz...]));
            } else {
                Controlled AddConstantUsingCGAdd(ctrl, (constant >>> tz, input[tz...]));
            }
        }
    }
}

export AddConstantSanders, AddConstant;
