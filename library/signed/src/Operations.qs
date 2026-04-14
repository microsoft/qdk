// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License.

import Std.Arrays.Tail, Std.Arrays.Most, Std.Arrays.Enumerated;
import Utils.AndLadder;
import Std.Arithmetic.RippleCarryTTKIncByLE, Std.Arithmetic.ApplyIfGreaterLE;
import Std.Diagnostics.Fact;

/// # Summary
/// Square signed integer `xs` and store
/// the result in `result`, which must be zero initially.
///
/// # Input
/// ## xs
/// 𝑛-bit integer to square
/// ## result
/// 2𝑛-bit result, must be in state |0⟩
/// initially.
///
/// # Remarks
/// The implementation relies on `SquareI`.
operation SquareSI(xs : Qubit[], result : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled SquareSI([], (xs, result));
    }
    controlled (controls, ...) {
        let n = Length(xs);
        use signx = Qubit();
        use signy = Qubit();

        within {
            CNOT(Tail(xs), signx);
            Controlled Invert2sSI([signx], xs);
        } apply {
            Controlled SquareI(controls, (xs, result));
        }
    }
}

/// # Summary
/// Computes the square of the integer `xs` into `result`,
/// which must be zero initially.
///
/// # Input
/// ## xs
/// 𝑛-bit number to square
/// ## result
/// 2𝑛-bit result, must be in state |0⟩ initially.
///
/// # Remarks
/// Uses a standard shift-and-add approach to compute the square. Saves
/// 𝑛-1 qubits compared to the straight-forward solution which first
/// copies out `xs` before applying a regular multiplier and then undoing
/// the copy operation.
operation SquareI(xs : Qubit[], result : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled SquareI([], (xs, result));
    }
    controlled (controls, ...) {
        let n = Length(xs);


        let numControls = Length(controls);
        if numControls == 0 {
            use aux = Qubit();
            for (idx, ctl) in Enumerated(xs) {
                within {
                    CNOT(ctl, aux);
                } apply {
                    Controlled RippleCarryTTKIncByLE([aux], (xs, (result[idx..idx + n])));
                }
            }
        } elif numControls == 1 {
            use aux = Qubit();
            for (idx, ctl) in Enumerated(xs) {
                within {
                    AND(controls[0], ctl, aux);
                } apply {
                    Controlled RippleCarryTTKIncByLE([aux], (xs, (result[idx..idx + n])));
                }
            }
        } else {
            use aux = Qubit[numControls];
            within {
                AndLadder(controls, Most(aux));
            } apply {
                for (idx, ctl) in Enumerated(xs) {
                    within {
                        AND(Tail(Most(aux)), ctl, Tail(aux));
                    } apply {
                        Controlled RippleCarryTTKIncByLE([Tail(aux)], (xs, (result[idx..idx + n])));
                    }
                }
            }
        }
    }
}

/// # Summary
/// Multiply integer `xs` by integer `ys` and store the result in `result`,
/// which must be zero initially.
///
/// # Input
/// ## xs
/// 𝑛₁-bit multiplicand
/// ## ys
/// 𝑛₂-bit multiplier
/// ## result
/// (𝑛₁+𝑛₂)-bit result, must be in state |0⟩ initially.
///
/// # Remarks
/// Uses a standard shift-and-add approach to implement the multiplication.
/// The controlled version was improved by copying out 𝑥ᵢ to an ancilla
/// qubit conditioned on the control qubits, and then controlling the
/// addition on the ancilla qubit.
operation MultiplyI(xs : Qubit[], ys : Qubit[], result : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        let na = Length(xs);
        let nb = Length(ys);

        for (idx, actl) in Enumerated(xs) {
            Controlled RippleCarryTTKIncByLE([actl], (ys, (result[idx..idx + nb])));
        }
    }
    controlled (controls, ...) {
        let na = Length(xs);
        let nb = Length(ys);

        // Perform various optimizations based on number of controls
        let numControls = Length(controls);
        if numControls == 0 {
            MultiplyI(xs, ys, result);
        } elif numControls == 1 {
            use aux = Qubit();
            for (idx, actl) in Enumerated(xs) {
                within {
                    AND(controls[0], actl, aux);
                } apply {
                    Controlled RippleCarryTTKIncByLE([aux], (ys, (result[idx..idx + nb])));
                }
            }
        } else {
            use helper = Qubit[numControls];
            within {
                AndLadder(controls, Most(helper));
            } apply {
                for (idx, actl) in Enumerated(xs) {
                    within {
                        AND(Tail(Most(helper)), actl, Tail(helper));
                    } apply {
                        Controlled RippleCarryTTKIncByLE([Tail(helper)], (ys, (result[idx..idx + nb])));
                    }
                }
            }
        }
    }
}

/// # Summary
/// Multiply signed integer `xs` by signed integer `ys` and store
/// the result in `result`, which must be zero initially.
///
/// # Input
/// ## xs
/// 𝑛₁-bit multiplicand
/// ## ys
/// 𝑛₂-bit multiplier
/// ## result
/// (𝑛₁+𝑛₂)-bit result, must be in state |0⟩
/// initially.
operation MultiplySI(xs : Qubit[], ys : Qubit[], result : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled MultiplySI([], (xs, ys, result));
    }
    controlled (controls, ...) {
        use signx = Qubit();
        use signy = Qubit();

        within {
            CNOT(Tail(xs), signx);
            CNOT(Tail(ys), signy);
            Controlled Invert2sSI([signx], xs);
            Controlled Invert2sSI([signy], ys);
        } apply {
            Controlled MultiplyI(controls, (xs, ys, result));
            within {
                CNOT(signx, signy);
            } apply {
                // No controls required since `result` will still be zero
                // if we did not perform the multiplication above.
                Controlled Invert2sSI([signy], result);
            }
        }
    }
}



/// # Summary
/// Implements a reversible sum gate. Given a carry-in bit encoded in
/// qubit `carryIn` and two summand bits encoded in `summand1` and `summand2`,
/// computes the bitwise xor of `carryIn`, `summand1` and `summand2` in the qubit
/// `summand2`.
///
/// # Input
/// ## carryIn
/// Carry-in qubit.
/// ## summand1
/// First summand qubit.
/// ## summand2
/// Second summand qubit, is replaced with the lower bit of the sum of
/// `summand1` and `summand2`.
///
/// # Remarks
/// In contrast to the `Carry` operation, this does not compute the carry-out bit.
operation Sum(carryIn : Qubit, summand1 : Qubit, summand2 : Qubit) : Unit is Adj + Ctl {
    CNOT(summand1, summand2);
    CNOT(carryIn, summand2);
}

/// # Summary
/// Inverts a given integer modulo 2's complement.
///
/// # Input
/// ## xs
/// n-bit signed integer (SignedLittleEndian), will be inverted modulo
/// 2's complement.
operation Invert2sSI(xs : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled Invert2sSI([], xs);
    }
    controlled (controls, ...) {
        ApplyToEachCA((Controlled X)(controls, _), xs);

        use aux = Qubit[Length(xs)];
        within {
            Controlled X(controls, aux[0]);
        } apply {
            RippleCarryTTKIncByLE(aux, xs);
        }
    }
}

/// # Summary
/// Divides two quantum integers.
///
/// # Description
/// `xs` will hold the
/// remainder `xs - floor(xs/ys) * ys` and `result` will hold
/// `floor(xs/ys)`.
///
/// # Input
/// ## xs
/// $n$-bit dividend, will be replaced by the remainder.
/// ## ys
/// $n$-bit divisor
/// ## result
/// $n$-bit result, must be in state $\ket{0}$ initially
/// and will be replaced by the result of the integer division.
///
/// # Remarks
/// Uses a standard shift-and-subtract approach to implement the division.
/// The controlled version is specialized such that the subtraction does not
/// require additional controls.
operation DivideI(xs : Qubit[], ys : Qubit[], result : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled DivideI([], (xs, ys, result));
    }
    controlled (controls, ...) {
        let n = Length(result);

        Fact(n == Length(ys), "Integer division requires
                           equally-sized registers ys and result.");
        Fact(n == Length(xs), "Integer division
                            requires an n-bit dividend registers.");

        let xpadded = xs + result;

        for i in (n - 1)..(-1)..0 {
            let xtrunc = xpadded[i..i + n-1];
            Controlled ApplyIfGreaterLE(controls, (X, ys, xtrunc, result[i]));
            // if ys > xtrunc, we don't subtract:
            (Controlled X)(controls, result[i]);
            (Controlled Adjoint RippleCarryTTKIncByLE)([result[i]], (ys, xtrunc));
        }
    }
}

/// # Summary
/// Computes the reciprocal 1/x for an unsigned integer x
/// using integer division. The result, interpreted as an integer,
/// will be `floor(2^(2*n-1) / x)`.
///
/// # Input
/// ## xs
/// n-bit unsigned integer
/// ## result
/// 2n-bit output, must be in $\ket{0}$ initially.
///
/// # Remarks
/// For the input x=0, the output will be all-ones.
operation ComputeReciprocalI(xs : Qubit[], result : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Controlled ComputeReciprocalI([], (xs, result));
    }
    controlled (controls, ...) {
        let n = Length(xs);
        Fact(Length(result) == 2 * n, "Result register must contain 2n qubits.");
        use lhs = Qubit[2 * n];
        use padding = Qubit[n];
        let paddedxs = xs + padding;
        X(Tail(lhs)); // initialize left-hand side to 2^{2n-1}
        // ... and divide:
        (Controlled DivideI)(controls, (lhs, paddedxs, result));
        // uncompute lhs
        for i in 0..2 * n - 1 {
            (Controlled RippleCarryTTKIncByLE)([result[i]], (paddedxs[0..2 * n-1-i], lhs[i..2 * n-1]));
        }
        X(Tail(lhs));
    }
}

export
    Sum,
    MultiplyI,
    MultiplySI,
    SquareSI,
    SquareI,
    Invert2sSI,
    DivideI,
    ComputeReciprocalI;