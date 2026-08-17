// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Arithmetic.RippleCarryCGIncByLE;
import Std.Arithmetic.RippleCarryTTKIncByLE;

/// This file re-exports addition algorithms from Std.Arithmetic.

/// Computes y += x (mod 2^n).
operation Add(x : Qubit[], y : Qubit[]) : Unit is Ctl + Adj {
    body (...) {
        let optimize = Std.Core.ConfigValue("optimize", "");
        if (optimize == "space") {
            RippleCarryTTKIncByLE(x, y);
        } elif (optimize == "time") {
            RippleCarryCGIncByLE(x, y);
        } else {
            RippleCarryTTKIncByLE(x, y);
        }
    }
    controlled (controls, ...) {
        let optimize = Std.Core.ConfigValue("optimize", "");
        if (Length(controls) == 0) {
            Add(x, y);
        } elif (optimize == "space") {
            Controlled RippleCarryTTKIncByLE(controls, (x, y));
        } elif (optimize == "time") {
            Controlled RippleCarryCGIncByLE(controls, (x, y));
        } else {
            Controlled RippleCarryTTKIncByLE(controls, (x, y));
        }
    }
}

/// Computes y -= x (mod 2^n).
operation Subtract(x : Qubit[], y : Qubit[]) : Unit is Ctl + Adj {
    Adjoint Add(x, y);
}

export Add, Subtract;
