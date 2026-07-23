// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Diagnostics.Fact;
import Std.Arithmetic.RippleCarryCGAddLE;
import Std.Arithmetic.RippleCarryTTKIncByLE;


/// # Summary
/// Flips the output qubit if and only if all the input qubits are 1.
///
/// # Resources
/// For n>=2, uses n-2 auxiliary qubits, n-2 AND/IAND pairs and 1 CCNOT gate.
///
/// # Input
/// ## controlQubits
/// Input qubits to check.
/// ## output
/// Output qubit to flip.
///
/// # References
/// - Michael A. Nielsen and Isaac L. Chuang,
///   "Quantum Computation and Quantum Information" (Section 4.3, Fig. 4.10), 2010.
operation MultiControl(controlQubits : Qubit[], output : Qubit) : Unit is Adj + Ctl {
    body (...) {
        let nQubits = Length(controlQubits);
        if (nQubits == 0) {
            // Do nothing.
        } elif (nQubits == 1) {
            CNOT(controlQubits[0], output);
        } elif (nQubits == 2) {
            CCNOT(controlQubits[0], controlQubits[1], output);
        } else {
            use anc = Qubit[nQubits-2];
            within {
                AND(controlQubits[0], controlQubits[1], anc[0]);
                for i in 0..nQubits-4 {
                    AND(anc[i], controlQubits[i + 2], anc[i + 1]);
                }
            } apply {
                CCNOT(anc[nQubits-3], controlQubits[nQubits-1], output);
            }
        }
    }
    controlled (controls, ...) {
        MultiControl(controls + controlQubits, output);
    }
    adjoint self;
}

export MultiControl;