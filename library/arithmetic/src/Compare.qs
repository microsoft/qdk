// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arithmetic.MAJ;
import Std.Arithmetic.ApplyIfGreaterLE;
import Std.Diagnostics.Fact;

import Utils.ParallelCNOT;
import Utils.CheckIfAllZero;

/// # Summary
/// Flips `q_answer` iff `q_input > q_out` using a Cuccaro-style ripple comparator.
/// The registers `q_input` and `q_out` are restored to their original values.
/// In controlled form, this operation supports at most one control qubit.
///
/// # Reference
/// Igor L. Markov, Mehdi Saeedi, "Constant-Optimized Quantum Circuits for
/// Modular Multiplication and Exponentiation" (fig. 6), 2015.
///
/// # Input
/// ## q_input
/// First input register.
/// ## q_out
/// Second input register with the same length as `q_input`.
/// ## q_answer
/// Target qubit flipped when `q_input > q_out`.
operation CompareCuccaro(
    q_input : Qubit[],
    q_out : Qubit[],
    q_answer : Qubit
) : Unit is Adj + Ctl {
    body (...) {
        Controlled CompareCuccaro([], (q_input, q_out, q_answer));
    }
    controlled (controls, ...) {
        let n = Length(q_input);
        Fact(n >= 1, "Input register must contain at least one qubit.");
        Fact(Length(q_out) == n, "Input size mismatch.");
        Fact(Length(controls) <= 1, "Operation takes at most 1 control.");

        use q_anc = Qubit();

        // Negate q_out so q_input + q_out becomes q_input - q_out.
        ApplyToEach(X, q_out);

        MAJ(q_anc, q_input[0], q_out[0]);
        for i in 1..n - 1 {
            MAJ(q_out[i - 1], q_input[i], q_out[i]);
        }

        Controlled CNOT(controls, (q_out[n - 1], q_answer));

        for i in n - 1..-1..1 {
            Adjoint MAJ(q_out[i - 1], q_input[i], q_out[i]);
        }
        Adjoint MAJ(q_anc, q_input[0], q_out[0]);

        ApplyToEach(X, q_out);
    }
    adjoint self;
}

operation CompareRippleCarry(x : Qubit[], y : Qubit[], result : Qubit) : Unit is Adj + Ctl {
    body (...) {
        ApplyIfGreaterLE(X, x, y, result);
    }
    controlled (controls, ...) {
        ApplyIfGreaterLE(Controlled X(controls, _), x, y, result);
    }
    adjoint self;
}

// Flips `result` iff x > y.
operation CompareGT(x : Qubit[], y : Qubit[], result : Qubit) : Unit is Adj + Ctl {
    if (Std.Core.ConfigValue("minimize_qubits", true)) {
        CompareCuccaro(x, y, result);
    } else {
        CompareRippleCarry(x, y, result);
    }
}

// Flips `result` iff x < y.
operation CompareLT(x : Qubit[], y : Qubit[], result : Qubit) : Unit is Adj + Ctl {
    CompareGT(y, x, result);
}

// Flips `result` iff x <= y.
operation CompareLE(x : Qubit[], y : Qubit[], result : Qubit) : Unit is Adj + Ctl {
    CompareGT(x, y, result);
    X(result);
}

// Flips `result` iff x >= y.
operation CompareGE(x : Qubit[], y : Qubit[], result : Qubit) : Unit is Adj + Ctl {
    CompareGT(y, x, result);
    X(result);
}

// Flips `result` iff x == y.
operation CompareEQ(x : Qubit[], y : Qubit[], result : Qubit) : Unit is Adj + Ctl {
    within {
        ParallelCNOT(x, y);
    } apply {
        CheckIfAllZero(y, result);
    }
}


export CompareCuccaro, CompareRippleCarry, CompareGT, CompareLT, CompareLE, CompareGE, CompareEQ;