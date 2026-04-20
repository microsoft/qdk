// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

import Types.FixedPoint;
import Init.PrepareFxP;
import Operations.AddFxP;
import Signed.Operations.Invert2sSI, Signed.Operations.MultiplySI, Signed.Operations.SquareSI;
import Facts.AssertPointPositionsIdenticalFxP;
import Std.Arrays.Zipped;
import Std.Arithmetic.RippleCarryTTKIncByLE;

/// # Summary
/// Adds a classical constant to a quantum fixed-point number.
///
/// # Input
/// ## constant
/// Constant to add to the quantum fixed-point number.
/// ## fp
/// Fixed-point number to which the constant will
/// be added.
@Config(Unrestricted)
operation AddConstantFxP(constant : Double, fp : FixedPoint) : Unit is Adj + Ctl {
    let n = Length(fp::Register);
    use ys = Qubit[n];
    let tmpFp = FixedPoint(fp::IntegerBits, ys);
    within {
        PrepareFxP(constant, tmpFp);
    } apply {
        AddFxP(tmpFp, fp);
    }
}

/// # Summary
/// Adds two fixed-point numbers stored in quantum registers.
///
/// # Description
/// Given two fixed-point registers respectively in states $\ket{f_1}$ and $\ket{f_2}$,
/// performs the operation $\ket{f_1} \ket{f_2} \mapsto \ket{f_1} \ket{f_1 + f_2}$.
///
/// # Input
/// ## fp1
/// First fixed-point number
/// ## fp2
/// Second fixed-point number, will be updated to contain the sum of the
/// two inputs.
///
/// # Remarks
/// The current implementation requires the two fixed-point numbers
/// to have the same point position counting from the least-significant
/// bit, i.e., $n_i$ and $p_i$ must be equal.
@Config(Unrestricted)
operation AddFxP(fp1 : FixedPoint, fp2 : FixedPoint) : Unit is Adj + Ctl {
    AssertPointPositionsIdenticalFxP([fp1, fp2]);

    RippleCarryTTKIncByLE(fp1::Register, fp2::Register);
}

/// # Summary
/// Computes the additive inverse of `fp`.
///
/// # Input
/// ## fp
/// Fixed-point number to invert.
///
/// # Remarks
/// Numerical inaccuracies may occur depending on the
/// bit-precision of the fixed-point number.
@Config(Unrestricted)
operation InvertFxP(fp : FixedPoint) : Unit is Adj + Ctl {
    let (_, reg) = fp!;
    Invert2sSI(reg);
}

/// # Summary
/// Computes `minuend - subtrahend` and stores the difference in `minuend`.
///
/// # Input
/// ## subtrahend
/// The subtrahend of the subtraction - the number to be subtracted.
/// ## minuend
/// The minuend of the subtraction - the number from which the other is subtracted.
///
/// # Remarks
/// Computes the difference by inverting `subtrahend` before and after adding
/// it to `minuend`.  Notice that `minuend`, the first argument is updated.
@Config(Unrestricted)
operation SubtractFxP(minuend : FixedPoint, subtrahend : FixedPoint) : Unit is Adj + Ctl {
    within {
        InvertFxP(subtrahend);
    } apply {
        AddFxP(subtrahend, minuend);
    }
}


/// # Summary
/// Multiplies two fixed-point numbers in quantum registers.
///
/// # Input
/// ## fp1
/// First fixed-point number.
/// ## fp2
/// Second fixed-point number.
/// ## result
/// Result fixed-point number, must be in state $\ket{0}$ initially.
///
/// # Remarks
/// The current implementation requires the three fixed-point numbers
/// to have the same point position and the same number of qubits.
@Config(Unrestricted)
operation MultiplyFxP(fp1 : FixedPoint, fp2 : FixedPoint, result : FixedPoint) : Unit is Adj {

    body (...) {
        Controlled MultiplyFxP([], (fp1, fp2, result));
    }
    controlled (controls, ...) {
        Facts.AssertFormatsAreIdenticalFxP([fp1, fp2, result]);
        let n = Length(fp1::Register);

        use tmpResult = Qubit[2 * n];
        let xsInt = ((fp1::Register));
        let ysInt = ((fp2::Register));
        let tmpResultInt = tmpResult;

        within {
            MultiplySI(xsInt, ysInt, tmpResultInt);
        } apply {
            Controlled ApplyToEachCA(controls, (CNOT, Zipped(tmpResult[n - fp1::IntegerBits..2 * n - fp1::IntegerBits - 1], result::Register)));
        }
    }
}

/// # Summary
/// Squares a fixed-point number.
///
/// # Input
/// ## fp
/// Fixed-point number.
/// ## result
/// Result fixed-point number,
/// must be in state $\ket{0}$ initially.
@Config(Unrestricted)
operation SquareFxP(fp : FixedPoint, result : FixedPoint) : Unit is Adj {
    body (...) {
        Controlled SquareFxP([], (fp, result));
    }
    controlled (controls, ...) {
        Facts.AssertFormatsAreIdenticalFxP([fp, result]);
        let n = Length(fp::Register);

        use tmpResult = Qubit[2 * n];
        let xsInt = fp::Register;
        let tmpResultInt = tmpResult;
        within {
            SquareSI(xsInt, tmpResultInt);
        } apply {
            Controlled ApplyToEachCA(controls, (CNOT, Zipped(tmpResult[n - fp::IntegerBits..2 * n - fp::IntegerBits - 1], result::Register)));
        }
    }
}

export
    AddConstantFxP,
    AddFxP,
    InvertFxP,
    SubtractFxP,
    MultiplyFxP,
    SquareFxP;
