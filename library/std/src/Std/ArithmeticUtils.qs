// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import
    Std.Diagnostics.Fact,
    Std.Arrays.*,
    Std.Math.Floor,
    Std.Math.Lg,
    Std.Math.HammingWeightI,
    Std.Math.TrailingZeroCountL,
    Std.Convert.IntAsDouble;

/// # Summary
/// Implements the outer operation for RippleCarryTTKIncByLE to conjugate
/// the inner operation to construct the full adder. Only Length(xs)
/// qubits are processed.
///
/// # Input
/// ## xs
/// Qubit register in a little-endian format containing the first summand
/// input to RippleCarryTTKIncByLE.
/// ## ys
/// Qubit register in a little-endian format containing the second summand
/// input to RippleCarryTTKIncByLE.
///
/// # References
/// - Yasuhiro Takahashi, Seiichiro Tani, Noboru Kunihiro: "Quantum
///   Addition Circuits and Unbounded Fan-Out", Quantum Information and
///   Computation, Vol. 10, 2010.
///   https://arxiv.org/abs/0910.2530
operation ApplyOuterTTKAdder(xs : Qubit[], ys : Qubit[]) : Unit is Adj + Ctl {
    Fact(Length(xs) <= Length(ys), "Input register ys must be at least as long as xs.");
    for i in 1..Length(xs) - 1 {
        CNOT(xs[i], ys[i]);
    }
    for i in Length(xs) - 2..-1..1 {
        CNOT(xs[i], xs[i + 1]);
    }
}

/// # Summary
/// Implements the inner addition function for the operation
/// RippleCarryTTKIncByLE. This is the inner operation that is conjugated
/// with the outer operation to construct the full adder.
///
/// # Input
/// ## xs
/// Qubit register in a little-endian format containing the first summand
/// input to RippleCarryTTKIncByLE.
/// ## ys
/// Qubit register in a little-endian format containing the second summand
/// input to RippleCarryTTKIncByLE.
///
/// # References
/// - Yasuhiro Takahashi, Seiichiro Tani, Noboru Kunihiro: "Quantum
///   Addition Circuits and Unbounded Fan-Out", Quantum Information and
///   Computation, Vol. 10, 2010.
///   https://arxiv.org/abs/0910.2530
///
/// # Remarks
/// The specified controlled operation makes use of symmetry and mutual
/// cancellation of operations to improve on the default implementation
/// that adds a control to every operation.
operation ApplyInnerTTKAdderNoCarry(xs : Qubit[], ys : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        (Controlled ApplyInnerTTKAdderNoCarry)([], (xs, ys));
    }
    controlled (controls, ...) {
        Fact(Length(xs) == Length(ys), "Input registers must have the same number of qubits.");

        for idx in 0..Length(xs) - 2 {
            CCNOT(xs[idx], ys[idx], xs[idx + 1]);
        }
        for idx in Length(xs) - 1..-1..1 {
            Controlled CNOT(controls, (xs[idx], ys[idx]));
            CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
        }
    }
}

/// # Summary
/// Implements the inner addition function for the operation
/// RippleCarryTTKIncByLE. This is the inner operation that is conjugated
/// with the outer operation to construct the full adder.
///
/// # Input
/// ## xs
/// Qubit register in a little-endian format containing the first summand
/// input to RippleCarryTTKIncByLE.
/// ## ys
/// Qubit register in a little-endian format containing the second summand
/// input to RippleCarryTTKIncByLE.
///
/// # References
/// - Yasuhiro Takahashi, Seiichiro Tani, Noboru Kunihiro: "Quantum
///   Addition Circuits and Unbounded Fan-Out", Quantum Information and
///   Computation, Vol. 10, 2010.
///   https://arxiv.org/abs/0910.2530
///
/// # Remarks
/// The specified controlled operation makes use of symmetry and mutual
/// cancellation of operations to improve on the default implementation
/// that adds a control to every operation.
operation ApplyInnerTTKAdderWithCarry(xs : Qubit[], ys : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        (Controlled ApplyInnerTTKAdderWithCarry)([], (xs, ys));
    }
    controlled (controls, ...) {
        Fact(Length(xs) + 1 == Length(ys), "ys must be one qubit longer than xs.");
        Fact(Length(xs) > 0, "Array should not be empty.");


        let nQubits = Length(xs);
        for idx in 0..nQubits - 2 {
            CCNOT(xs[idx], ys[idx], xs[idx + 1]);
        }
        (Controlled CCNOT)(controls, (xs[nQubits - 1], ys[nQubits - 1], ys[nQubits]));
        for idx in nQubits - 1..-1..1 {
            Controlled CNOT(controls, (xs[idx], ys[idx]));
            CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
        }
    }
}

/// # Summary
/// Implements Half-adder. Adds qubit x to qubit y and sets carryOut appropriately
operation HalfAdderForInc(x : Qubit, y : Qubit, carryOut : Qubit) : Unit is Adj + Ctl {
    body (...) {
        CCNOT(x, y, carryOut);
        CNOT(x, y);
    }
    adjoint auto;

    controlled (ctls, ...) {
        Fact(Length(ctls) == 1, "HalfAdderForInc should be controlled by exactly one control qubit.");

        let ctl = ctls[0];
        use helper = Qubit();

        within {
            AND(x, y, helper);
        } apply {
            AND(ctl, helper, carryOut);
        }
        CCNOT(ctl, x, y);
    }
    controlled adjoint auto;
}

/// # Summary
/// Implements Full-adder. Adds qubit carryIn and x to qubit y and sets carryOut appropriately.
operation FullAdderForInc(carryIn : Qubit, x : Qubit, y : Qubit, carryOut : Qubit) : Unit is Adj + Ctl {
    body (...) {
        // TODO: cannot use `Carry` operation here
        CNOT(carryIn, x);
        CNOT(carryIn, y);
        CCNOT(x, y, carryOut);
        CNOT(carryIn, carryOut);
        CNOT(carryIn, x);
        CNOT(x, y);
    }
    adjoint auto;

    controlled (ctls, ...) {
        Fact(Length(ctls) == 1, "FullAdderForInc should be controlled by exactly one control qubit.");

        let ctl = ctls[0];
        use helper = Qubit();

        CarryForInc(carryIn, x, y, helper);
        CCNOT(ctl, helper, carryOut);
        Controlled UncarryForInc(ctls, (carryIn, x, y, helper));
    }
    controlled adjoint auto;
}

// Computes carryOut := carryIn + x + y
operation FullAdder(carryIn : Qubit, x : Qubit, y : Qubit, carryOut : Qubit) : Unit is Adj {
    CNOT(x, y);
    CNOT(x, carryIn);
    AND(y, carryIn, carryOut);
    CNOT(x, y);
    CNOT(x, carryOut);
    CNOT(y, carryIn);
}

/// # Summary
/// Computes carry bit for a full adder.
operation CarryForInc(carryIn : Qubit, x : Qubit, y : Qubit, carryOut : Qubit) : Unit is Adj + Ctl {
    body (...) {
        CNOT(carryIn, x);
        CNOT(carryIn, y);
        AND(x, y, carryOut);
        CNOT(carryIn, carryOut);
    }
    adjoint auto;
    controlled (ctls, ...) {
        // This CarryForInc is intended to be used only in an in-place
        // ripple-carry implementation. Only such particular use case allows
        // for this simple implementation where controlled version
        // is the same as uncontrolled body.
        CarryForInc(carryIn, x, y, carryOut);
    }
    controlled adjoint auto;
}

/// # Summary
/// Uncomputes carry bit for a full adder.
operation UncarryForInc(carryIn : Qubit, x : Qubit, y : Qubit, carryOut : Qubit) : Unit is Adj + Ctl {
    body (...) {
        CNOT(carryIn, carryOut);
        Adjoint AND(x, y, carryOut);
        CNOT(carryIn, x);
        CNOT(x, y);
    }
    adjoint auto;
    controlled (ctls, ...) {
        Fact(Length(ctls) == 1, "UncarryForInc should be controlled by exactly one control qubit.");

        let ctl = ctls[0];

        CNOT(carryIn, carryOut);
        Adjoint AND(x, y, carryOut);
        CCNOT(ctl, x, y); // Controlled X(ctls + [x], y);
        CNOT(carryIn, x);
        CNOT(carryIn, y);
    }
    controlled adjoint auto;
}

operation ApplyOrAssuming0Target(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
    within {
        X(control1);
        X(control2);
    } apply {
        AND(control1, control2, target);
        X(target);
    }
}

/// # Summary
/// Computes carries for the look-ahead adder
operation ComputeCarries(ps : Qubit[], gs : Qubit[]) : Unit is Adj {
    let n = Length(gs);
    Fact(Length(ps) + 1 == n, "Register gs must be one qubit longer than register ps.");

    let T = Floor(Lg(IntAsDouble(n)));
    use qs = Qubit[n - HammingWeightI(n) - T];

    let registerPartition = MappedOverRange(t -> Floor(IntAsDouble(n) / IntAsDouble(2^t)) - 1, 1..T - 1);
    let pWorkspace = [ps] + Partitioned(registerPartition, qs);

    // Note that we cannot use AND gate targeting gs[0] as it may not be in the 0 state.
    // We use regular CCNOT in GRounds and CRounds.
    within {
        PRounds(pWorkspace);
    } apply {
        // U_G
        GRounds(pWorkspace, gs);

        // U_C
        CRounds(pWorkspace, gs);
    }
}

/// # Summary
/// Computes all p[i, j] values in workspace for the look-ahead adder.
///
/// The register array `pWorkspace` has T entries, where T = ⌊log₂ n⌋.
///
/// The first entry `pWorkspace[0]` is initialized with `P_0` which is
/// computed before `ComputeCarries` is called.  The other registers are
/// 0-initialized and will be computed in successive rounds t = 1, ..., T - 1.
///
/// In each round t we compute
///
/// p[i, j] = p[2ᵗ × m, 2ᵗ × (m + 1)] = p[i, k] ∧ p[k, j]
///
/// in `pWorkspace[t][m - 1]` and use that for k = 2ᵗ × m + 2ᵗ⁻¹, p[i, k] and p[k, j]
/// have already been computed in round t - 1 in `pWorkspace[t - 1][2 * m - 1]` and
/// `pWorkspace[t - 1][2 * m]`, respectively.
operation PRounds(pWorkspace : Qubit[][]) : Unit is Adj {
    for ws in Windows(2, pWorkspace) {
        // note that we are using Rest, since pWorkspace[t - 1][0] is never
        // accessed in round t.
        let (current, next) = (Rest(ws[0]), ws[1]);

        for m in IndexRange(next) {
            AND(current[2 * m], current[2 * m + 1], next[m]);
        }
    }
}

/// # Summary
/// Computes g[i ∧ (i + 1), i + 1] into gs[i] for the look-ahead adder.
///
/// The register gs has n entries initialized to gs[i] = g[i, i + 1].
///
/// After successive rounds t = 1, ..., T, the register is updated to
/// gs[i] = g[i ∧ (i + 1), i + 1], from which we can compute the carries
/// in the C-rounds.
operation GRounds(pWorkspace : Qubit[][], gs : Qubit[]) : Unit is Adj {
    let T = Length(pWorkspace);
    let n = Length(gs);

    for t in 1..T {
        let length = Floor(IntAsDouble(n) / IntAsDouble(2^t)) - 1;
        let ps = pWorkspace[t - 1][0..2...];

        for m in 0..length {
            CCNOT(gs[2^t * m + 2^(t - 1) - 1], ps[m], gs[2^t * m + 2^t - 1]);
        }
    }
}

/// # Summary
/// Computes carries into gs for the look-ahead adder.
operation CRounds(pWorkspace : Qubit[][], gs : Qubit[]) : Unit is Adj {
    let n = Length(gs);

    let start = Floor(Lg(IntAsDouble(2 * n) / 3.0));
    for t in start..-1..1 {
        let length = Floor(IntAsDouble(n - 2^(t - 1)) / IntAsDouble(2^t));
        let ps = pWorkspace[t - 1][1..2...];

        for m in 1..length {
            CCNOT(gs[2^t * m - 1], ps[m - 1], gs[2^t * m + 2^(t - 1) - 1]);
        }
    }
}

operation PhaseGradient(qs : Qubit[]) : Unit is Adj + Ctl {
    for i in IndexRange(qs) {
        R1Frac(1, i, qs[i]);
    }
}

//
// operations for comparisons
//

/// # Summary
/// Applies `action` to `target` if register `x` is greater or equal to BigInt `c`
/// (if `invertControl` is false). If `invertControl` is true, the `action`
/// is applied in the opposite situation.
operation ApplyActionIfGreaterThanOrEqualConstant<'T>(
    invertControl : Bool,
    action : 'T => Unit is Adj + Ctl,
    c : BigInt,
    x : Qubit[],
    target : 'T
) : Unit is Adj + Ctl {

    let bitWidth = Length(x);
    if c == 0L {
        if not invertControl {
            action(target);
        }
    } elif c >= (2L^bitWidth) {
        if invertControl {
            action(target);
        }
    } else {
        // normalize constant
        let l = TrailingZeroCountL(c);

        let cNormalized = c >>> l;
        let xNormalized = x[l...];
        let bitWidthNormalized = Length(xNormalized);

        // If c == 2L^(bitwidth - 1), then bitWidthNormalized will be 1,
        // and qs will be empty.  In that case, we do not need to compute
        // any temporary values, and some optimizations are apply, which
        // are considered in the remainder.
        use qs = Qubit[bitWidthNormalized - 1];
        let cs1 = IsEmpty(qs) ? [] | [Head(xNormalized)] + Most(qs);

        Fact(Length(cs1) == Length(qs), "Arrays should be of the same length.");

        within {
            for i in 0..Length(cs1) - 1 {
                let op = cNormalized &&& (1L <<< (i + 1)) != 0L ? AND | ApplyOrAssuming0Target;
                op(cs1[i], xNormalized[i + 1], qs[i]);
            }
        } apply {
            let control = IsEmpty(qs) ? Tail(x) | Tail(qs);
            within {
                if invertControl {
                    X(control);
                }
            } apply {
                Controlled action([control], target);
            }
        }
    }
}

/// # Summary
/// Applies `action` to `target` if the sum of `x` and `y` registers
/// overflows, i.e. there's a carry out (if `invertControl` is false).
/// If `invertControl` is true, the `action` is applied when there's no carry out.
operation ApplyActionIfSumOverflows<'T>(
    action : 'T => Unit is Adj + Ctl,
    x : Qubit[],
    y : Qubit[],
    invertControl : Bool,
    target : 'T
) : Unit is Adj + Ctl {

    let n = Length(x);
    Fact(n >= 1, "Registers must contain at least one qubit.");
    Fact(Length(y) == n, "Registers must be of the same length.");

    use carries = Qubit[n];

    within {
        CarryWith1CarryIn(x[0], y[0], carries[0]);
        for i in 1..n - 1 {
            CarryForInc(carries[i - 1], x[i], y[i], carries[i]);
        }
    } apply {
        within {
            if invertControl {
                X(carries[n - 1]);
            }
        } apply {
            Controlled action([carries[n - 1]], target);
        }
    }
}

/// # Summary
/// Computes carry out assuming carry in is 1.
/// Simplified version that is only applicable for scenarios
/// where controlled version is the same as non-controlled.
operation CarryWith1CarryIn(
    x : Qubit,
    y : Qubit,
    carryOut : Qubit
) : Unit is Adj + Ctl {

    body (...) {
        X(x);
        X(y);
        AND(x, y, carryOut);
        X(carryOut);
    }

    adjoint auto;

    controlled (ctls, ...) {
        Fact(Length(ctls) <= 1, "Number of control lines must be at most 1");
        CarryWith1CarryIn(x, y, carryOut);
    }

    controlled adjoint auto;
}

/// # Summary
/// This wrapper allows operations that support only one control
/// qubit to be used in a multi-controlled scenarios. It provides
/// controlled version that collects controls into one qubit
/// by applying AND chain using auxiliary qubit array.
operation ApplyAsSinglyControlled<'TIn>(
    op : ('TIn => Unit is Adj + Ctl),
    input : 'TIn
) : Unit is Adj + Ctl {

    body (...) {
        op(input);
    }

    controlled (ctls, ...) {
        let n = Length(ctls);
        if n == 0 {
            op(input);
        } elif n == 1 {
            Controlled op(ctls, input);
        } else {
            use aux = Qubit[n - 1];
            within {
                LogDepthAndChain(ctls, aux);
            } apply {
                Controlled op([Tail(aux)], input);
            }
        }
    }
}

/// # Summary
/// This helper function computes the AND of all control bits in `ctls` into
/// the last qubit of `tgts`, using the other qubits in `tgts` as helper
/// qubits for the AND of subsets of control bits.  The operation has a
/// logarithmic depth of AND gates by aligning them using a balanced binary
/// tree.
operation LogDepthAndChain(ctls : Qubit[], tgts : Qubit[]) : Unit is Adj {
    let lc = Length(ctls);
    let lt = Length(tgts);

    Fact(lc == lt + 1, $"There must be exactly one more control qubit than target qubits (got {lc}, {lt})");

    if lt == 1 {
        AND(ctls[0], ctls[1], tgts[0]);
    } elif lt == 2 {
        AND(ctls[0], ctls[1], tgts[0]);
        AND(ctls[2], tgts[0], tgts[1]);
    } else {
        let left = lc / 2;
        let right = lc - left;

        let ctlsLeft = ctls[...left - 1];
        let tgtsLeft = tgts[...left - 2];

        let ctlsRight = ctls[left..left + right - 1];
        let tgtsRight = tgts[left - 1..left + right - 3];

        LogDepthAndChain(ctlsLeft, tgtsLeft);
        LogDepthAndChain(ctlsRight, tgtsRight);
        AND(Tail(tgtsLeft), Tail(tgtsRight), Tail(tgts));
    }
}

// Writes little-endian integer to quantum register (prepared in 0 state).
operation ApplyBigInt(val : BigInt, reg : Qubit[]) : Unit is Adj + Ctl {
    let bits = Std.Convert.BigIntAsBoolArray(val, Length(reg));
    ApplyPauliFromBitString(PauliX, true, bits, reg);
}

// Measures content of register as little-endian BigInt.
// Resets register to zero state.
operation MResetL(reg : Qubit[]) : BigInt {
    let result = MResetEachZ(reg);
    mutable base : BigInt = 1L;
    mutable ans : BigInt = 0L;
    for i in 0..Length(result)-1 {
        if (result[i] == One) {
            set ans += base;
        }
        set base *= 2L;
    }
    return ans;
}
