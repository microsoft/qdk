// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


import QIR.Intrinsic.*;
import Std.Intrinsic.*;
import Std.Diagnostics.*;
import Std.Math.*;

/// # Summary
/// Applies an operation to each element in a register.
///
/// # Input
/// ## singleElementOperation
/// Operation to apply to each element.
/// ## register
/// Array of elements on which to apply the given operation.
///
/// # Type Parameters
/// ## 'T
/// The target on which the operation acts.
///
/// # Example
/// Prepare a three-qubit |+⟩ state:
/// ```qsharp
/// use register = Qubit[3];
/// ApplyToEach(H, register);
/// ```
operation ApplyToEach<'T>(singleElementOperation : ('T => Unit), register : 'T[]) : Unit {
    for item in register {
        singleElementOperation(item);
    }
}

/// # Summary
/// Applies an operation to each element in a register.
/// The modifier `A` indicates that the single-element operation is adjointable.
///
/// # Input
/// ## singleElementOperation
/// Operation to apply to each element.
/// ## register
/// Array of elements on which to apply the given operation.
///
/// # Type Parameters
/// ## 'T
/// The target on which the operation acts.
///
/// # Example
/// Prepare a three-qubit |+⟩ state:
/// ```qsharp
/// use register = Qubit[3];
/// ApplyToEach(H, register);
/// ```
///
/// # See Also
/// - [Std.Canon.ApplyToEach](xref:Qdk.Std.Canon.ApplyToEach)
operation ApplyToEachA<'T>(singleElementOperation : ('T => Unit is Adj), register : 'T[]) : Unit is Adj {
    for item in register {
        singleElementOperation(item);
    }
}

/// # Summary
/// Applies an operation to each element in a register.
/// The modifier `C` indicates that the single-element operation is controllable.
///
/// # Input
/// ## singleElementOperation
/// Operation to apply to each element.
/// ## register
/// Array of elements on which to apply the given operation.
///
/// # Type Parameters
/// ## 'T
/// The target on which the operation acts.
///
/// # Example
/// Prepare a three-qubit |+⟩ state:
/// ```qsharp
/// use register = Qubit[3];
/// ApplyToEach(H, register);
/// ```
///
/// # See Also
/// - [Std.Canon.ApplyToEach](xref:Qdk.Std.Canon.ApplyToEach)
operation ApplyToEachC<'T>(singleElementOperation : ('T => Unit is Ctl), register : 'T[]) : Unit is Ctl {
    for item in register {
        singleElementOperation(item);
    }
}

/// # Summary
/// Applies an operation to each element in a register.
/// The modifier `CA` indicates that the single-element operation is controllable and adjointable.
///
/// # Input
/// ## singleElementOperation
/// Operation to apply to each element.
/// ## register
/// Array of elements on which to apply the given operation.
///
/// # Type Parameters
/// ## 'T
/// The target on which the operation acts.
///
/// # Example
/// Prepare a three-qubit |+⟩ state:
/// ```qsharp
/// use register = Qubit[3];
/// ApplyToEach(H, register);
/// ```
///
/// # See Also
/// - [Std.Canon.ApplyToEach](xref:Qdk.Std.Canon.ApplyToEach)
operation ApplyToEachCA<'T>(singleElementOperation : ('T => Unit is Adj + Ctl), register : 'T[]) : Unit is Adj + Ctl {
    for item in register {
        singleElementOperation(item);
    }
}

/// # Summary
/// Applies the controlled-X (CX) gate to a pair of qubits.
///
/// # Input
/// ## control
/// Control qubit for the CX gate.
/// ## target
/// Target qubit for the CX gate.
///
/// # Remarks
/// This operation can be simulated by the unitary matrix
/// $$
/// \begin{align}
///     \left(\begin{matrix}
///         1 & 0 & 0 & 0 \\\\
///         0 & 1 & 0 & 0 \\\\
///         0 & 0 & 0 & 1 \\\\
///         0 & 0 & 1 & 0
///      \end{matrix}\right)
/// \end{align},
/// $$
/// where rows and columns are organized as in the quantum concepts guide.
///
/// Equivalent to:
/// ```qsharp
/// Controlled X([control], target);
/// ```
/// and to:
/// ```qsharp
/// CNOT(control, target);
/// ```
operation CX(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__cx__body(control, target);
    }
    controlled (ctls, ...) {
        Controlled X(ctls + [control], target);
    }
    adjoint self;
}

/// # Summary
/// Applies the controlled-Y (CY) gate to a pair of qubits.
///
/// # Input
/// ## control
/// Control qubit for the CY gate.
/// ## target
/// Target qubit for the CY gate.
///
/// # Remarks
/// This operation can be simulated by the unitary matrix
/// $$
/// \begin{align}
///     \left(\begin{matrix}
///         1 & 0 & 0 & 0 \\\\
///         0 & 1 & 0 & 0 \\\\
///         0 & 0 & 0 & -i \\\\
///         0 & 0 & i & 0
///      \end{matrix}\right)
/// \end{align},
/// $$
/// where rows and columns are organized as in the quantum concepts guide.
///
/// Equivalent to:
/// ```qsharp
/// Controlled Y([control], target);
/// ```
operation CY(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__cy__body(control, target);
    }
    controlled (ctls, ...) {
        Controlled Y(ctls + [control], target);
    }
    adjoint self;
}

/// # Summary
/// Applies the controlled-Z (CZ) gate to a pair of qubits.
///
/// # Input
/// ## control
/// Control qubit for the CZ gate.
/// ## target
/// Target qubit for the CZ gate.
///
/// # Remarks
/// This operation can be simulated by the unitary matrix
/// $$
/// \begin{align}
///     \left(\begin{matrix}
///         1 & 0 & 0 & 0 \\\\
///         0 & 1 & 0 & 0 \\\\
///         0 & 0 & 1 & 0 \\\\
///         0 & 0 & 0 & -1
///     \end{matrix}\right)
/// \end{align},
/// $$
/// where rows and columns are organized as in the quantum concepts guide.
///
/// Equivalent to:
/// ```qsharp
/// Controlled Z([control], target);
/// ```
operation CZ(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__cz__body(control, target);
    }
    controlled (ctls, ...) {
        Controlled Z(ctls + [control], target);
    }
    adjoint self;
}

/// Given a pair, returns its first element.
function Fst<'T, 'U>(pair : ('T, 'U)) : 'T {
    let (fst, _) = pair;
    return fst;
}

/// Given a pair, returns its second element.
function Snd<'T, 'U>(pair : ('T, 'U)) : 'U {
    let (_, snd) = pair;
    return snd;
}

/// # Summary
/// Computes the parity of a register of qubits in-place.
///
/// # Input
/// ## qubits
/// Array of qubits whose parity is to be computed and stored.
///
/// # Remarks
/// This operation transforms the state of its input as
/// $$
/// \begin{align}
///     \ket{q_0} \ket{q_1} \cdots \ket{q_{n - 1}} & \mapsto
///     \ket{q_0} \ket{q_0 \oplus q_1} \ket{q_0 \oplus q_1 \oplus q_2} \cdots
///         \ket{q_0 \oplus \cdots \oplus q_{n - 1}}.
/// \end{align}
/// $$
operation ApplyCNOTChain(qubits : Qubit[]) : Unit is Adj + Ctl {
    for i in 0..Length(qubits) - 2 {
        CNOT(qubits[i], qubits[i + 1]);
    }
}

/// # Summary
/// Given a single-qubit Pauli operator, applies the corresponding operation
/// to a single qubit.
///
/// # Input
/// ## pauli
/// The Pauli operator to be applied.
/// ## target
/// The qubit to which `pauli` is to be applied as an operation.
///
/// # Example
/// The following are equivalent:
/// ```qsharp
/// ApplyP(PauliX, q);
/// ```
/// and
/// ```qsharp
/// X(q);
/// ```
operation ApplyP(pauli : Pauli, target : Qubit) : Unit is Adj + Ctl {
    if pauli == PauliX { X(target); } elif pauli == PauliY { Y(target); } elif pauli == PauliZ { Z(target); }
}

/// # Summary
/// Given a multi-qubit Pauli operator, applies the corresponding operation
/// to a quantum register.
///
/// # Input
/// ## pauli
/// A multi-qubit Pauli operator represented as an array of single-qubit Pauli operators.
/// ## target
/// Register to apply the given Pauli operation on.
///
/// # Example
/// The following are equivalent:
/// ```qsharp
/// ApplyPauli([PauliY, PauliZ, PauliX], target);
/// ```
/// and
/// ```qsharp
/// Y(target[0]);
/// Z(target[1]);
/// X(target[2]);
/// ```
operation ApplyPauli(pauli : Pauli[], target : Qubit[]) : Unit is Adj + Ctl {
    Fact(Length(pauli) == Length(target), "`pauli` and `target` must be of the same length.");
    for i in 0..Length(pauli) - 1 {
        ApplyP(pauli[i], target[i]);
    }
}

/// # Summary
/// Applies a Pauli operator on each qubit in an array if the corresponding
/// bit of a Boolean array matches a given input.
///
/// # Input
/// ## pauli
/// Pauli operator to apply to `qubits[idx]` where `bitApply == bits[idx]`
/// ## bitApply
/// apply Pauli if bit is this value
/// ## bits
/// Boolean register specifying which corresponding qubit in `qubits` should be operated on
/// ## qubits
/// Quantum register on which to selectively apply the specified Pauli operator
///
/// # Remarks
/// The Boolean array and the quantum register must be of equal length.
///
/// # Example
/// The following applies an X operation on qubits 0 and 2, and a Z operation on qubits 1 and 3.
/// ```qsharp
/// use qubits = Qubit[4];
/// let bits = [true, false, true, false];
/// // Apply when index in `bits` is `true`.
/// ApplyPauliFromBitString(PauliX, true, bits, qubits);
/// // Apply when index in `bits` is `false`.
/// ApplyPauliFromBitString(PauliZ, false, bits, qubits);
/// ```
operation ApplyPauliFromBitString(pauli : Pauli, bitApply : Bool, bits : Bool[], qubits : Qubit[]) : Unit is Adj + Ctl {
    let nBits = Length(bits);
    Fact(nBits == Length(qubits), "Number of bits must be equal to number of qubits.");
    for i in 0..nBits - 1 {
        if bits[i] == bitApply {
            ApplyP(pauli, qubits[i]);
        }
    }
}

/// # Summary
/// Applies a Pauli operator on each qubit in an array if the corresponding
/// bit of a Little-endian integer matches a given input.
///
/// # Input
/// ## pauli
/// Pauli operator to apply to `qubits[idx]` when bit of numberState
/// in idx position is the same as bitApply.
/// ## bitApply
/// apply Pauli if bit is this value
/// ## numberState
/// Little-endian integer specifying which corresponding qubit in `qubits` should be operated on
/// ## qubits
/// Quantum register on which to selectively apply the specified Pauli operator
///
/// # Example
/// The following applies an X operation on qubits 0 and 2, and a Z operation on qubits 1 and 3.
/// ```qsharp
/// use qubits = Qubit[4];
/// let n = 5;
/// // Apply when index in `bits` is `true`.
/// ApplyPauliFromBitString(PauliX, true, n, qubits);
/// // Apply when index in `bits` is `false`.
/// ApplyPauliFromBitString(PauliZ, false, n, qubits);
/// ```
operation ApplyPauliFromInt(
    pauli : Pauli,
    bitApply : Bool,
    numberState : Int,
    qubits : Qubit[]
) : Unit is Adj + Ctl {

    let length = Length(qubits);
    Fact(numberState >= 0, "number must be non-negative");
    Fact(BitSizeI(numberState) <= length, "Bit size of numberState must not exceed qubits length");

    for i in 0..length - 1 {
        // If we assume loop unrolling, 2^i will be optimized to a constant.
        if ((numberState &&& (1 <<< i)) != 0) == bitApply {
            ApplyP(pauli, qubits[i]);
        }
    }
}

/// # Summary
/// Maps a Pauli axis from one direction to another by applying an appropriate Clifford transformation.
/// For example, use `within MapPauliAxis(PauliZ, PauliX, q)` and perform Z rotation when X rotation is desired.
///
/// # Description
/// This function applies single-qubit Clifford transformations that remap Pauli axes on a Bloch sphere.
/// These mappings are useful to apply operations known for one Pauli basis in different Pauli bases.
/// Provide `from` and `to` parameters in terms of a passive transformation. For example,
/// when a rotation around the X axis is desired and a rotation around the Z axis is available,
/// the Z axis gets pointed in the direction of the X axis so that the rotation around Z axis can be used.
/// Out of several transformations that achieve the requested mapping, the one with fewer gates is used.
///
/// # Input
/// ## from
/// The Pauli axis to map from. Perform subsequent operations on this axis.
/// ## to
/// The Pauli axis to map to. Subsequent operations on `from` axis will perform as if they act on this axis.
/// ## q
/// The qubit on which the transformation will be applied.
///
/// # Remarks
/// The complete list of possible mappings and a list of gate sequences to achieve them.
/// For example, a transformation of +X+Y+Z ↦ +X+Z-Y means that the bloch sphere is rotated so that
/// the X axis remains unchanged, Y axis points in Z direction, and Z axis points in -Y direction.
/// (Y with direction reversed). Such transformation could be used to achieve PauliZ to PauliY mapping.
///
/// +X+Y+Z ↦ +X+Y+Z: I (used when `from` == `to`)            \
/// +X+Y+Z ↦ +Z-Y+X: H (used when mapping Z to X and X to Z) \
/// +X+Y+Z ↦ +Y+Z+X: S⁻¹H (used when mapping Z to Y)         \
/// +X+Y+Z ↦ +Z+X+Y: HS (used when mapping Y to Z)           \
/// +X+Y+Z ↦ +Y-X+Z: S (used when mapping Y to X)            \
/// +X+Y+Z ↦ -Y+X+Z: S⁻¹ (used when mapping X to Y)          \
/// +X+Y+Z ↦ -X+Z+Y: S⁻¹HS                                   \
/// +X+Y+Z ↦ -Y-Z+X: SH                                      \
/// +X+Y+Z ↦ +Y-Z-X: SHZ, S⁻¹HY, YSH, SXH, S⁻¹YH, XS⁻¹H      \
/// +X+Y+Z ↦ -X-Z-Y: SHS⁻¹                                   \
/// +X+Y+Z ↦ +X-Y-Z: X                                       \
/// +X+Y+Z ↦ -Z+X-Y: HXS⁻¹, HSX, ZHS⁻¹, YHS, HYS, HS⁻¹Y      \
/// +X+Y+Z ↦ +X-Z+Y: SHS, HS⁻¹H                              \
/// +X+Y+Z ↦ -Y+Z-X: SHY, S⁻¹HZ, YS⁻¹H, SYH, S⁻¹XH, XSH      \
/// +X+Y+Z ↦ -X+Y-Z: Y                                       \
/// +X+Y+Z ↦ -Z+Y+X: HX, ZH                                  \
/// +X+Y+Z ↦ -Y-X-Z: YS, SX, S⁻¹Y, XS⁻¹                      \
/// +X+Y+Z ↦ +Y+X-Z: YS⁻¹, SY, S⁻¹X, XS                      \
/// +X+Y+Z ↦ +Z+Y-X: XH, HZ                                  \
/// +X+Y+Z ↦ +X+Z-Y: S⁻¹HS⁻¹, HSH                            \
/// +X+Y+Z ↦ -Z-X+Y: HXS, HSY, ZHS, YHS⁻¹, HYS⁻¹, HS⁻¹X      \
/// +X+Y+Z ↦ -X-Y+Z: Z                                       \
/// +X+Y+Z ↦ -Z-Y-X: YH, HY                                  \
/// +X+Y+Z ↦ +Z-X-Y: HS⁻¹
///
/// # Example
/// ```qsharp
/// // The following implements Rx(0.1, q) via Rz.
/// within {
///    MapPauliAxis(PauliZ, PauliX, q);
/// } apply {
///    Rz(0.1, q);
/// }
/// ```
///
/// # References
/// - [Wikipedia: Bloch sphere](https://wikipedia.org/wiki/Bloch_sphere)
/// - [Wikipedia: Clifford group](https://wikipedia.org/wiki/Clifford_group)
/// - [Wikipedia: Active and passive transformation](https://wikipedia.org/wiki/Active_and_passive_transformation)
operation MapPauliAxis(from : Pauli, to : Pauli, q : Qubit) : Unit is Adj + Ctl {
    if from == to {
        // +X+Y+Z ↦ +X+Y+Z, No gates are needed.
    } elif (from == PauliZ and to == PauliX) or (from == PauliX and to == PauliZ) {
        // +X+Y+Z ↦ +Z-Y+X
        H(q);
    } elif from == PauliZ and to == PauliY {
        // +X+Y+Z ↦ +Y+Z+X
        Adjoint S(q);
        H(q);
    } elif from == PauliY and to == PauliZ {
        // +X+Y+Z ↦ +Z+X+Y
        H(q);
        S(q);
    } elif from == PauliY and to == PauliX {
        // +X+Y+Z ↦ +Y-X+Z
        S(q);
    } elif from == PauliX and to == PauliY {
        // +X+Y+Z ↦ -Y+X+Z
        Adjoint S(q);
    } else {
        fail "Unsupported mapping of Pauli axes.";
    }
}

/// # Summary
/// Applies a unitary operation on the target if the control
/// register state corresponds to a specified nonnegative integer.
///
/// # Input
/// ## numberState
/// A nonnegative integer on which the operation `oracle` should be
/// controlled.
/// ## oracle
/// A unitary operation to be controlled.
/// ## target
/// A target on which to apply `oracle`.
/// ## controlRegister
/// A quantum register that controls application of `oracle`.
///
/// # Remarks
/// The value of `numberState` is interpreted using a little-endian encoding.
///
/// `numberState` must be at most $2^\texttt{Length(controlRegister)} - 1$.
/// For example, `numberState = 537` means that `oracle`
/// is applied if and only if `controlRegister` is in the state $\ket{537}$.
operation ApplyControlledOnInt<'T>(
    numberState : Int,
    oracle : ('T => Unit is Adj + Ctl),
    controlRegister : Qubit[],
    target : 'T
) : Unit is Adj + Ctl {

    within {
        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
    } apply {
        Controlled oracle(controlRegister, target);
    }
}

/// # Summary
/// Applies `oracle` on `target` when `controlRegister`
/// is in the state specified by `bits`.
///
/// # Description
/// Applies a unitary operation `oracle` on the `target`, controlled
/// on a state specified by a given bit mask `bits`.
/// The bit at `bits[i]` corresponds to qubit at `controlRegister[i]`.
/// The pattern given by `bits` may be shorter than `controlRegister`,
/// in which case additional control qubits are ignored (that is, neither
/// controlled on |0⟩ nor |1⟩).
/// If `bits` is longer than `controlRegister`, an error is raised.
///
/// # Input
/// ## bits
/// The bit string to control the given unitary operation on.
/// ## oracle
/// The unitary operation to be applied on the target.
/// ## target
/// The target to be passed to `oracle` as an input.
/// ## controlRegister
/// A quantum register that controls application of `oracle`.
///
/// # Example
/// ```qsharp
/// // When bits = [1,0,0] oracle is applied if and only if controlRegister
/// // is in the state |100⟩.
/// use t = Qubit();
/// use c = Qubit[3];
/// X(c[0]);
/// ApplyControlledOnBitString([true, false, false], X, c, t);
/// Message($"{M(t)}"); // Prints `One` since oracle `X` was applied.
/// ```
operation ApplyControlledOnBitString<'T>(
    bits : Bool[],
    oracle : ('T => Unit is Adj + Ctl),
    controlRegister : Qubit[],
    target : 'T
) : Unit is Adj + Ctl {

    // The control register must have enough bits to implement the requested control.
    Fact(Length(bits) <= Length(controlRegister), "Control register shorter than control pattern.");

    // Use a subregister of the controlled register when
    // bits is shorter than controlRegister.
    let controlSubregister = controlRegister[...Length(bits) - 1];
    within {
        ApplyPauliFromBitString(PauliX, false, bits, controlSubregister);
    } apply {
        Controlled oracle(controlSubregister, target);
    }
}

/// # Summary
/// Applies the rotations of Quantum Fourier Transform (QFT) to a little-endian quantum register.
///
/// # Description
/// Applies the rotations of QFT to a little-endian register `qs` of length n
/// containing |x₁⟩⊗|x₂⟩⊗…⊗|xₙ⟩. The qs[0] initially contains the
/// least significant bit xₙ. The state of qs[0] becomes
/// (|0⟩+𝑒^(2π𝑖[0.xₙ])|1⟩)/sqrt(2) after the operation.
///
/// # Input
/// ## qs
/// Quantum register in a little-endian format to which the rotations are applied.
///
/// # Remarks
/// Note that this operation applies only the rotations part of the QFT.
/// To complete the transform, you need to reverse the order of qubits after this operation,
/// for example, using the operation `SwapReverseRegister`.
///
/// # Reference
///  - [Quantum Fourier transform](https://en.wikipedia.org/wiki/Quantum_Fourier_transform)
operation ApplyQFT(qs : Qubit[]) : Unit is Adj + Ctl {
    let length = Length(qs);
    Fact(length >= 1, "ApplyQFT: Length(qs) must be at least 1.");
    for i in length - 1..-1..0 {
        H(qs[i]);
        for j in 0..i - 1 {
            Controlled R1Frac([qs[i]], (1, j + 1, qs[i - j - 1]));
        }
    }
}

/// # Summary
/// Uses SWAP gates to reverse the order of the qubits in a register.
///
/// # Input
/// ## register
/// The qubits order of which should be reversed using SWAP gates
operation SwapReverseRegister(register : Qubit[]) : Unit is Adj + Ctl {
    let length = Length(register);
    for i in 0..length / 2 - 1 {
        SWAP(register[i], register[(length - i) - 1]);
    }
}

/// # Summary
/// Applies a bitwise-XOR operation between a classical integer and an
/// integer represented by a register of qubits.
///
/// # Description
/// Applies `X` operations to qubits in a little-endian register based on
/// 1 bits in an integer.
///
/// Let us denote `value` by a and let y be an unsigned integer encoded in `target`,
/// then `ApplyXorInPlace` performs an operation given by the following map:
/// |y⟩ ↦ |y ⊕ a⟩, where ⊕ is the bitwise exclusive OR operator.
operation ApplyXorInPlace(value : Int, target : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Fact(value >= 0, "`value` must be non-negative.");
        mutable runningValue = value;
        for q in target {
            if (runningValue &&& 1) != 0 {
                X(q);
            }
            set runningValue >>>= 1;
        }
        Fact(runningValue == 0, "value is too large");
    }
    adjoint self;
}

/// # Summary
/// Applies a bitwise-XOR operation between a classical integer and an
/// integer represented by a register of qubits.
///
/// # Description
/// Applies `X` operations to qubits in a little-endian register based on
/// 1 bits in an integer.
///
/// Let us denote `value` by a and let y be an unsigned integer encoded in `target`,
/// then `ApplyXorInPlace` performs an operation given by the following map:
/// |y⟩ ↦ |y ⊕ a⟩, where ⊕ is the bitwise exclusive OR operator.
operation ApplyXorInPlaceL(value : BigInt, target : Qubit[]) : Unit is Adj + Ctl {
    body (...) {
        Fact(value >= 0L, "`value` must be non-negative.");
        mutable runningValue = value;
        for q in target {
            if (runningValue &&& 1L) != 0L {
                X(q);
            }
            set runningValue >>>= 1;
        }
        Fact(runningValue == 0L, "`value` is too large.");
    }
    adjoint self;
}

/// # Summary
/// Applies operation `op` to the `target` `power` times.
/// If `power` is negative, the adjoint of `op` is used.
/// If `power` is 0, the operation `op` is not applied.
operation ApplyOperationPowerA<'T>(
    power : Int,
    op : 'T => Unit is Adj,
    target : 'T
) : Unit is Adj {
    let u = if power >= 0 { op } else { Adjoint op };
    for _ in 1..AbsI(power) {
        u(target);
    }
}

/// # Summary
/// Applies operation `op` to the `target` `power` times.
/// If `power` is negative, the adjoint of `op` is used.
/// If `power` is 0, the operation `op` is not applied.
operation ApplyOperationPowerCA<'T>(
    power : Int,
    op : 'T => Unit is Ctl + Adj,
    target : 'T
) : Unit is Ctl + Adj {
    let u = if power >= 0 { op } else { Adjoint op };
    for _ in 1..AbsI(power) {
        u(target);
    }
}

/// # Summary
/// Performs the quantum phase estimation algorithm for a unitary U represented
/// by `applyPowerOfU`, and a `targetState`. Returns the phase estimation
/// in the range of [0, 2π) as a fraction of 2π in the little-endian register `phase`.
///
/// # Input
/// ## applyPowerOfU
/// An oracle implementing Uᵐ for a unitary U and a given integer power m.
/// `ApplyOperationPowerCA(_, U, _)` can be used to implement this oracle,
/// if no better performing implementation is available.
/// ## targetState
/// A quantum register acted on by U. When this `targetState` is the eigenstate
/// of the operator U, the corresponding eigenvalue is estimated.
/// ## phase
/// A little-endian quantum register containing the phase estimation result. The phase is
/// a fraction of 2π represented in binary with q[0] containing 2π/2=π bit, q[1] containing 2π/4=π/2 bit,
/// q[2] containing 2π/8=π/4 bit, and so on. The length of the register indicates the desired precision.
/// The phase register is assumed to be in the state |0...0⟩ when the operation is invoked.
///
/// # Remarks
/// All eigenvalues of a unitary operator are of magnitude 1 and therefore can be represented as
/// $e^{i\phi}$ for some $\phi \in [0, 2\pi)$. Finding the phase of an eigenvalue is therefore
/// equivalent to finding the eigenvalue itself. Passing the eigenvector as `targetState`
/// to the phase estimation operation allows finding the eigenvalue to the desired precision
/// determined by the length of the `phase` register.
///
/// # Reference
/// - [Quantum phase estimation algorithm](https://wikipedia.org/wiki/Quantum_phase_estimation_algorithm)
///
/// # See Also
/// - [Std.Canon.ApplyOperationPowerCA](xref:Qdk.Std.Canon.ApplyOperationPowerCA)
operation ApplyQPE(
    applyPowerOfU : (Int, Qubit[]) => Unit is Adj + Ctl,
    targetState : Qubit[],
    phase : Qubit[]
) : Unit is Adj + Ctl {

    let nQubits = Length(phase);
    ApplyToEachCA(H, phase);

    for i in 0..nQubits - 1 {
        let power = 2^((nQubits - i) - 1);
        Controlled applyPowerOfU(
            phase[i..i],
            (power, targetState)
        );
    }

    Adjoint ApplyQFT(phase);
}

/// # Summary
/// Relabels the qubits in the `current` array with the qubits in the `updated` array. The `updated` array
/// must be a valid permutation of the `current` array.
///
/// # Input
/// ## current
/// Array of qubits to be relabeled.
/// ## updated
/// Array of qubits with which to relabel the `current` array.
///
/// # Remarks
/// This operation is useful when you need to relabel qubits in a way that does not incur any quantum operations.
/// Statically allocated qubits can also be relabeled in measurement-dependent branches and loops.
/// Subsequent uses of any alias of a relabeled qubit refer to its updated physical identity.
/// Note that when compiling for execution on hardware with limited qubit connectivity, this operation
/// may not result in any changes to qubit adjacency and one or more `SWAP` gates may still be required.
///
/// # Example
/// The following example demonstrates how to relabel qubits in a register:
/// ```qsharp
/// use qubits = Qubit[3];
/// let newOrder = [qubits[2], qubits[0], qubits[1]];
/// Relabel(qubits, newOrder);
/// ```
/// After this operation, any use of `qubits[0]` will refer to the qubit that was originally `qubits[2]`, and so on.
/// To exchange the labels on two qubits, the virtual equivalent of a `SWAP` gate, you can use the following code:
/// ```qsharp
/// use (q0, q1) = (Qubit(), Qubit());
/// Relabel([q0, q1], [q1, q0]);
/// ```
/// Note that the adjoint of this operation effectively changes the order of arguments, such that
/// `Adjoint Relabel(qubits, newOrder)` is equivalent to `Relabel(newOrder, qubits)`.
operation Relabel(current : Qubit[], updated : Qubit[]) : Unit is Adj {
    body ... {
        PermuteLabels(current, updated);
    }
    adjoint ... {
        PermuteLabels(updated, current);
    }
}

operation PermuteLabels(current : Qubit[], updated : Qubit[]) : Unit {
    body intrinsic;
}

export
    ApplyToEach,
    ApplyToEachA,
    ApplyToEachC,
    ApplyToEachCA,
    CX,
    CY,
    CZ,
    Fst,
    Snd,
    ApplyCNOTChain,
    ApplyP,
    ApplyPauli,
    ApplyPauliFromBitString,
    ApplyPauliFromInt,
    MapPauliAxis,
    ApplyControlledOnInt,
    ApplyControlledOnBitString,
    ApplyQFT,
    ApplyQPE,
    SwapReverseRegister,
    ApplyXorInPlace,
    ApplyXorInPlaceL,
    ApplyOperationPowerA,
    ApplyOperationPowerCA,
    Relabel;
