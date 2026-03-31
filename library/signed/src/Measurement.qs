// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

import Std.Diagnostics.Fact;
import Operations.Invert2sSI;

/// # Summary
/// Measures a signed integer of a given width.
/// If the width is 4, the qubit register will be measured as a 4-bit signed integer.
/// This means that bit n is the sign bit, and bits n-1 to 0 are the integer value.
/// If fewer than `width` qubits are provided, the remaining bits are assumed to be 0.
/// For example, if qubit register `[q1, q2, q3]` is passed in, but the width is 5,
/// the integer value will be measured as `[0, 0, q1, q2, q3]`, with the first 0 being
/// the sign bit (positive). This is in contrast to the standard library `MeasureInteger`,
/// which always measures unsigned integers up to and including 63 qubits in width.
/// If the length of the qubit register passed in is greater than the width, this operation
/// will throw an error.
///
/// # Input
/// ## target
/// A qubit register representing the signed integer to be measured.
///
/// ## width
/// The width of the signed integer to be measured.
operation MeasureSignedInteger(target : Qubit[], width : Int) : Int {
    let nBits = Length(target);
    Fact(nBits <= 64, $"`Length(target)` must be less than or equal to 64, but was {nBits}.");
    Fact(nBits <= width, $"`Length(target)` must be less than or equal to `width`, but was {nBits}.");
    Fact(nBits > 0, $"`width` must be greater than 0, but was {width}.");

    mutable coefficient = 1;
    let signBit = MResetZ(target[nBits - 1]);
    if (signBit == One) {
        Operations.Invert2sSI(target);
        set coefficient = -1;
    }

    mutable number = 0;
    for i in 0..nBits - 2 {
        if (MResetZ(target[i]) == One) {
            set number |||= 1 <<< i;
        }
    }

    ResetAll(target);

    number * coefficient
}

export MeasureSignedInteger;
