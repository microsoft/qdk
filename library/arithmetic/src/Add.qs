// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arithmetic.RippleCarryCGIncByLE;
import Std.Arithmetic.RippleCarryTTKIncByLE;

/// This file re-exports addition algorithms from Std.Arithmetic.

/// Computes y += x (mod 2^n).
operation Add(x : Qubit[], y : Qubit[]) : Unit is Ctl + Adj {
    body (...) {
        if (Std.Core.ConfigValue("minimize_qubits", true)) {
            RippleCarryTTKIncByLE(x, y);
        } else {
            RippleCarryCGIncByLE(x, y);
        }
    }
    controlled (controls, ...) {
        if (Length(controls) == 0) {
            Add(x, y);
        } elif (Std.Core.ConfigValue("minimize_qubits", true)) {
            Controlled RippleCarryTTKIncByLE(controls, (x, y));
        } else {
            Controlled RippleCarryCGIncByLE(controls, (x, y));
        }
    }
}

/// Computes y -= x (mod 2^n).
operation Subtract(x : Qubit[], y : Qubit[]) : Unit is Ctl + Adj {
    Adjoint Add(x, y);
}

export Add, Subtract;
