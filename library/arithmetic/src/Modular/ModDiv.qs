// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import Std.Diagnostics.Fact;
import Std.Convert.IntAsDouble;
import Std.Arrays.Reversed;
import Std.Math;

import Add.Subtract;
import ClassicalMath.DivCeil;
import ClassicalMath.Log2;
import Compare.CompareGT;
import Modular.ModAdd.ModAdd;
import Modular.ModMul.ModDouble;
import ResourceEstimation.LoopA;
import Utils.CheckIfAllZero;
import Utils.ParallelSWAP;
import Utils.RotateRight;

/// This file contains an implementation of modular multiplication, division, and inversion
/// based on the Extended Euclidean Algorithm, as described in the paper:
/// Andre Schrottenloher, "Optimized Point Addition Circuits for Elliptic Curve Discrete
/// Logarithms", 2026, https://arxiv.org/abs/2606.02235.

/// Compresses six garbage qubits into five qubits.
///
/// Under the assumption that pairs `(q[2*i], q[2*i+1])` are never `(Zero, One)`
/// for `i in 0..2`, this operation maps states so that `q[5]` is always `Zero`.
/// This matches Fig. 1.
operation CompressGarbage(qs : Qubit[]) : Unit is Adj + Ctl {
    Fact(Length(qs) == 6, "CompressGarbage requires exactly 6 qubits.");

    CNOT(qs[1], qs[0]);
    CNOT(qs[3], qs[2]);
    CNOT(qs[5], qs[4]);
    CNOT(qs[0], qs[2]);
    CNOT(qs[5], qs[3]);
    X(qs[4]);
    CCNOT(qs[1], qs[3], qs[5]);
    CNOT(qs[1], qs[4]);
    X(qs[2]);
    CCNOT(qs[3], qs[4], qs[5]);
    CCNOT(qs[5], qs[4], qs[1]);
    CCNOT(qs[5], qs[2], qs[0]);
    CCNOT(qs[0], qs[1], qs[5]);
}

// Size to which we can truncate registers on i-th iteration of GCD.
function ActiveRegisterSize(n : Int, iterationIdx : Int) : Int {
    let cPad = 2.3;
    let regSizeApprox = IntAsDouble(n)
        - 0.5 * Log2(8.0 / 3.0) * IntAsDouble(iterationIdx + 1) + cPad * Math.Sqrt(IntAsDouble(n));
    return Math.MinI(n, Math.Ceiling(regSizeApprox));
}

// Number of GCD iterations from Schrottenloher (2026), Section 3.1.
//
// The paper shows that the required number of iterations follows a normal
// distribution with mean 1.413 and standard deviation 0.6*sqrt(n).
// Choosing c_iter=2.4 corresponds to 4 standard deviations, so the probability of
// error (not doing enough iterations) is 3e-5.
function NumGcdIterations(n : Int) : Int {
    let cIter = 2.4;
    let estimate = 1.413 * IntAsDouble(n) + cIter * Math.Sqrt(IntAsDouble(n));
    return Math.MinI(2 * n, Math.Ceiling(estimate));
}

// Size of the garbage vector with compression.
function GarbageVectorSize(n : Int) : Int {
    let numIterations = DivCeil(NumGcdIterations(n), 3);
    return 5 * numIterations + 1;
}

/// Auxiliary registers used by algorithms.
/// qubits = g ∪ t.
/// g and t may overlap if using qubit sharing.
struct _AuxQubits {
    // All auxiliary qubits.
    qubits : Qubit[],
    // Garbage register.
    g : Qubit[],
    // Register for the first argument to _Gcd.
    t : Qubit[],
}


// Allocates auxiliary registers with qubit sharing.
// Qubit sharing: share qubits between t and garbage vector in such a way that on every
// iteration of GCD used qubits don't overlap.
// We can do this because as we need to use more qubits of g, we need to use less
// qubits of t (as number in t becomes smaller).
// We share a suffix of g and a suffix of t.
operation _AllocateAuxQubits(n : Int) : _AuxQubits {
    let garbageSize = GarbageVectorSize(n);
    let numIterations = DivCeil(NumGcdIterations(n), 3);
    let minActiveSize = ActiveRegisterSize(n, 3 * numIterations - 1);
    let numSharedQubits = n - minActiveSize;

    let qubits = QIR.Runtime.AllocateQubitArray(garbageSize + minActiveSize);
    let g = qubits[0..garbageSize-1];
    let tExtra = qubits[garbageSize..garbageSize + minActiveSize - 1];
    let tShared = Reversed(g)[0..numSharedQubits - 1];
    let t = tExtra + tShared;
    return _AuxQubits(qubits, g, t);
}

operation _ReleaseAuxQubits(aux : _AuxQubits) : Unit {
    QIR.Runtime.ReleaseQubitArray(aux.qubits);
}

/// Single iteration of GCD algorithm from Schrottenloher (2026), Alg. 2.
///
/// This operation updates only the active prefixes of `u` and `v`.
operation _GcdIteration(iteration_idx : Int, u : Qubit[], v : Qubit[], b0 : Qubit, b01 : Qubit) : Unit is Adj {
    let n = Length(u);
    let regSize = ActiveRegisterSize(n, iteration_idx);
    let uReg = u[0..regSize - 1];
    let vReg = v[0..regSize - 1];

    // b0 := v % 2.
    CNOT(vReg[0], b0);
    // b01 := b0 & (u > v).
    (Controlled CompareGT)([b0], (uReg, vReg, b01));
    // if (b0 & b1) swap(u, v).
    (Controlled ParallelSWAP)([b01], (uReg, vReg));
    // if (b0) v -= u.
    (Controlled Subtract)([b0], (uReg, vReg));
    // v := v / 2.
    RotateRight(vReg);
}

/// GCD algorithm from Schrottenloher (2026), Alg. 2, with garbage compression enabled.
///
/// Preconditions: `u` is odd, `g` is initialized to `0`, and `gcd(u, v) = 1`.
/// Postcondition: `u = 1`, `v = 0`, and `g` stores compressed garbage bits.
operation _Gcd(u : Qubit[], v : Qubit[], g : Qubit[]) : Unit is Adj {
    let n = Length(u);
    Fact(n > 0, "Registers must be non-empty.");
    Fact(Length(v) == n, "Registers u and v must have the same length.");

    let numIterations = DivCeil(NumGcdIterations(n), 3);
    Fact(
        Length(g) == GarbageVectorSize(n),
        "Garbage register size mismatch for compressed GCD."
    );

    LoopA(numIterations, idx => {
        let page = g[5 * idx..5 * idx + 5];
        for i in 0..2 {
            _GcdIteration(3 * idx + i, u, v, page[2 * i], page[2 * i + 1]);
        }
        CompressGarbage(page);
    });
}

/// Single reconstruction iteration, corresponds to a single GCD iteration.
operation _ReconstructionIteration(
    r : Qubit[],
    s : Qubit[],
    b0 : Qubit,
    b01 : Qubit,
    modulus : BigInt
) : Unit is Adj {
    // s := (2*s) % modulus.
    ModDouble(s, modulus);
    // if (b0) s += r (mod modulus).
    Controlled ModAdd([b0], (r, s, modulus));
    // if (b0 & b1) swap(r, s).
    (Controlled ParallelSWAP)([b01], (r, s));
}

/// Restores Bezout coefficients from compressed garbage vector produced by GCD.
///
/// Algorithm 3 from Schrottenloher (2026), using compressed pages.
operation _BezoutReconstruction(modulus : BigInt, r : Qubit[], s : Qubit[], g : Qubit[]) : Unit is Adj {
    let n = Length(r);
    Fact(Length(s) == n, "Registers r and s must have the same length.");

    let numIterations = DivCeil(NumGcdIterations(n), 3);
    Fact(
        Length(g) == GarbageVectorSize(n),
        "Garbage register size mismatch for compressed reconstruction."
    );

    LoopA(numIterations, idx => {
        let reverseIdx = numIterations - 1 - idx;
        let page = g[5 * reverseIdx..5 * reverseIdx + 4] + [g[Length(g) - 1]];

        Adjoint CompressGarbage(page);
        _ReconstructionIteration(r, s, page[4], page[5], modulus);
        _ReconstructionIteration(r, s, page[2], page[3], modulus);
        _ReconstructionIteration(r, s, page[0], page[1], modulus);
        CompressGarbage(page);
    });
}

/// Computes `(x, y) -> (x, (x * y) % modulus)`.
/// Takes all ancillas as inputs.
/// Returns ancillas (g, t) in the zero state if x != 0.
/// If x == 0, they will not be returned in the zero state and must be reset.
operation _ModMul(x : Qubit[], y : Qubit[], modulus : BigInt, aux : _AuxQubits) : Unit is Adj {
    let n = Length(x);
    Fact(Length(y) == n, "Registers x and y must have the same length.");
    Fact(3L <= modulus and modulus < (1L <<< n), "Modulus must satisfy 3 <= modulus < 2^n.");
    Fact(modulus % 2L == 1L, "Modulus must be odd.");
    Fact(Length(aux.g) == GarbageVectorSize(n), "Garbage size mismatch");

    // Prepare t = modulus, run GCD on (t, x).
    within {
        ApplyXorInPlaceL(modulus, aux.t);
        _Gcd(aux.t, x, aux.g);
        ApplyXorInPlaceL(1L, aux.t);
    } apply {
        _BezoutReconstruction(modulus, y, x, aux.g);
        ParallelSWAP(x, y);
    }
}


/// # Summary
/// Computes `(x, y) -> (x, (x * y) % modulus)`.
/// Requires `0 < x < modulus` and `gcd(x, modulus) = 1`.
/// If `x == 0`, this operation calls `Reset` on non-zero qubits.
///
/// # Input
/// ## x
/// Register storing `x`.
/// ## y
/// Register storing `y` and updated in place.
/// ## modulus
/// Odd modulus satisfying `3 <= modulus < 2^Length(x)`.
operation ModMul(x : Qubit[], y : Qubit[], modulus : BigInt) : Unit {
    let aux = _AllocateAuxQubits(Length(x));
    _ModMul(x, y, modulus, aux);
    ResetAll(aux.qubits);
    _ReleaseAuxQubits(aux);
}

/// # Summary
/// Computes `(x, y) -> (x, (y * x^-1) % modulus)`.
/// Requires `0 < x < modulus` and `gcd(x, modulus) = 1`.
/// If `x == 0`, this operation calls `Reset` on non-zero qubits.
///
/// # Input
/// ## x
/// Register storing `x`.
/// ## y
/// Register storing `y` and updated in place.
/// ## modulus
/// Odd modulus satisfying `3 <= modulus < 2^Length(x)`.
operation ModDiv(x : Qubit[], y : Qubit[], modulus : BigInt) : Unit {
    let aux = _AllocateAuxQubits(Length(x));
    Adjoint _ModMul(x, y, modulus, aux);
    ResetAll(aux.qubits);
    _ReleaseAuxQubits(aux);
}

/// # Summary
/// Computes `(x, y) -> (x, (x * y) % modulus)` if `x != 0`.
/// Computes `(0, y) -> (0, y)` if `x == 0`.
/// Requires `0 <= x < modulus` and `gcd(x, modulus) = 1`.
///
/// # Input
/// ## x
/// Register storing `x`.
/// ## y
/// Register storing `y` and updated in place.
/// ## modulus
/// Odd modulus satisfying `3 <= modulus < 2^Length(x)`.
operation SafeModMul(x : Qubit[], y : Qubit[], modulus : BigInt) : Unit {
    use isXZero = Qubit();
    within {
        CheckIfAllZero(x, isXZero);
        CNOT(isXZero, x[0]);
    } apply {
        let aux = _AllocateAuxQubits(Length(x));
        _ModMul(x, y, modulus, aux);
        _ReleaseAuxQubits(aux);
    }
}

/// # Summary
/// Computes `(x, y) -> (x, (y * x^-1) % modulus)` if `x != 0`.
/// Computes `(0, y) -> (0, y)` if `x == 0`.
/// Requires `0 <= x < modulus` and `gcd(x, modulus) = 1`.
///
/// # Input
/// ## x
/// Register storing `x`.
/// ## y
/// Register storing `y` and updated in place.
/// ## modulus
/// Odd modulus satisfying `3 <= modulus < 2^Length(x)`.
operation SafeModDiv(x : Qubit[], y : Qubit[], modulus : BigInt) : Unit {
    use isXZero = Qubit();
    within {
        CheckIfAllZero(x, isXZero);
        CNOT(isXZero, x[0]);
    } apply {
        let aux = _AllocateAuxQubits(Length(x));
        Adjoint _ModMul(x, y, modulus, aux);
        _ReleaseAuxQubits(aux);
    }
}

/// # Summary
/// Computes `(x, 0) -> (x, x^-1 (mod modulus))`.
/// Requires `x != 0` and `gcd(x, modulus) = 1`.
///
/// # Input
/// ## x
/// Register storing `x`.
/// ## ans
/// Zero-initialized register that is updated to `x^-1 (mod modulus)`.
/// ## modulus
/// Odd modulus satisfying `3 <= modulus < 2^Length(x)`.
operation ModInv(x : Qubit[], ans : Qubit[], modulus : BigInt) : Unit {
    X(ans[0]); // ans:=1.
    ModDiv(x, ans, modulus);
}

export ModMul, ModDiv, SafeModMul, SafeModDiv, ModInv;
