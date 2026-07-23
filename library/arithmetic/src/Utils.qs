// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Simple helpers used in various arithmetic circuits.

import Std.Diagnostics.Fact;

import MultiControl.MultiControl;

/// Swaps two same-length qubit arrays element-wise.
operation ParallelSWAP(xs : Qubit[], ys : Qubit[]) : Unit is Adj + Ctl {
    Fact(Length(xs) == Length(ys), "ParallelSWAP: registers must have equal length");
    for idx in 0..Length(xs) - 1 {
        SWAP(xs[idx], ys[idx]);
    }
}

/// Applies CNOT between two registers.
operation ParallelCNOT(controls : Qubit[], targets : Qubit[]) : Unit is Ctl + Adj {
    let n : Int = Length(controls);
    Fact(Length(targets) == n, "Size mismatch.");
    for i in 0..n-1 {
        CNOT(controls[i], targets[i]);
    }
}

/// Cyclically rotates qubits left with SWAPs.
/// If the high qubit is 0, this is multiplication by 2.
operation RotateLeft(x : Qubit[]) : Unit is Adj + Ctl {
    for i in Length(x) - 1.. -1..1 {
        SWAP(x[i], x[i - 1]);
    }
}

/// Cyclically rotates qubits right with SWAPs.
/// If x stores an even number, this is division by 2.
operation RotateRight(x : Qubit[]) : Unit is Adj + Ctl {
    Adjoint RotateLeft(x);
}

/// Flips `output` if all qubits in `xs` are in |1> state.
operation CheckIfAllOnes(xs : Qubit[], output : Qubit) : Unit is Ctl + Adj {
    MultiControl(xs, output);
}

/// Flips `output` if all qubits in `xs` are in |0> state.
operation CheckIfAllZero(xs : Qubit[], output : Qubit) : Unit is Adj + Ctl {
    body (...) {
        (Controlled CheckIfAllZero)([], (xs, output));
    }
    controlled (controls, ...) {
        ApplyToEachCA(X, xs);
        MultiControl(controls + xs, output);
        ApplyToEachCA(X, xs);
    }
}

export ParallelSWAP, ParallelCNOT, RotateLeft, RotateRight, CheckIfAllOnes, CheckIfAllZero;