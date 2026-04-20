// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


import Std.Convert.*;
import Std.Core.*;
import Std.Diagnostics.*;
import Std.Math.*;
open QIR.Intrinsic;
import Std.InternalHelpers.*;

/// # Summary
/// Applies the AND gate that is more efficient for use with decomposition of multi-controlled operations.
/// Note that target qubit must be in |0⟩ state.
///
/// # Input
/// ## control1
/// First control qubit for the AND gate.
/// ## control2
/// Second control qubit for the AND gate.
/// ## target
/// Target qubit for the AND gate.
///
/// # Remarks
/// Use the Adjoint only for uncomputation purposes.
@Config(Adaptive)
operation AND(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
    body ... {
        __quantum__qis__ccx__body(control1, control2, target);
    }
    adjoint ... {
        __quantum__qis__h__body(target);
        if MResetZ(target) == One {
            __quantum__qis__cz__body(control1, control2);
        }
    }
}

/// # Summary
/// Applies the AND gate that is more efficient for use with decomposition of multi-controlled operations.
/// Note that target qubit must be in |0⟩ state.
///
/// # Input
/// ## control1
/// First control qubit for the AND gate.
/// ## control2
/// Second control qubit for the AND gate.
/// ## target
/// Target qubit for the AND gate.
///
/// # Remarks
/// Use the Adjoint only for uncomputation purposes.
@Config(not Adaptive)
operation AND(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
    PhaseCCX(control1, control2, target);
}

/// # Summary
/// Applies the doubly controlled–NOT (CCNOT) gate to three qubits.
///
/// # Input
/// ## control1
/// First control qubit for the CCNOT gate.
/// ## control2
/// Second control qubit for the CCNOT gate.
/// ## target
/// Target qubit for the CCNOT gate.
///
/// # Remarks
/// Equivalent to:
/// ```qsharp
/// Controlled X([control1, control2], target);
/// ```
operation CCNOT(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__ccx__body(control1, control2, target);
    }
    controlled (ctls, ...) {
        Controlled X(ctls + [control1, control2], target);
    }
    adjoint self;
}

/// # Summary
/// Applies the controlled-NOT (CNOT) gate to a pair of qubits.
///
/// # Input
/// ## control
/// Control qubit for the CNOT gate.
/// ## target
/// Target qubit for the CNOT gate.
///
/// # Remarks
/// $$
/// \begin{align}
///     \operatorname{CNOT} \mathrel{:=}
///     \begin{bmatrix}
///         1 & 0 & 0 & 0 \\\\
///         0 & 1 & 0 & 0 \\\\
///         0 & 0 & 0 & 1 \\\\
///         0 & 0 & 1 & 0
///     \end{bmatrix},
/// \end{align}
/// $$
///
/// where rows and columns are ordered as in the quantum concepts guide.
///
/// Equivalent to:
/// ```qsharp
/// Controlled X([control], target);
/// ```
operation CNOT(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__cx__body(control, target);
    }
    controlled (ctls, ...) {
        Controlled X(ctls + [control], target);
    }
    adjoint self;
}

/// # Summary
/// Applies the exponential of a multi-qubit Pauli operator.
///
/// # Input
/// ## paulis
/// Array of single-qubit Pauli values indicating the tensor product
/// factors on each qubit.
/// ## theta
/// Angle about the given multi-qubit Pauli operator by which the
/// target register is to be rotated.
/// ## qubits
/// Register to apply the given rotation to.
///
/// # Remarks
/// $$
/// \begin{align}
///     e^{i \theta [P_0 \otimes P_1 \cdots P_{N-1}]},
/// \end{align}
/// $$
/// where $P_i$ is the $i$-th element of `paulis`, and where
/// $N = $`Length(paulis)`.
operation Exp(paulis : Pauli[], theta : Double, qubits : Qubit[]) : Unit is Adj + Ctl {
    body ... {
        Fact(Length(paulis) == Length(qubits), "Arrays 'pauli' and 'qubits' must have the same length");
        let (paulis, qubits) = RemovePauliI(paulis, qubits);
        let angle = -2.0 * theta;
        let len = Length(paulis);

        if len == 0 {
            ApplyGlobalPhase(theta);
        } elif len == 1 {
            R(paulis[0], angle, qubits[0]);
        } elif len == 2 {
            within {
                MapPauliAxis(paulis[0], paulis[1], qubits[1]);
            } apply {
                if (paulis[0] == PauliX) {
                    Rxx(angle, qubits[0], qubits[1]);
                } elif (paulis[0] == PauliY) {
                    Ryy(angle, qubits[0], qubits[1]);
                } elif (paulis[0] == PauliZ) {
                    Rzz(angle, qubits[0], qubits[1]);
                }
            }
        } else {
            // len > 2
            within {
                for i in 0..Length(paulis) - 1 {
                    MapPauliAxis(PauliZ, paulis[i], qubits[i]);
                }
            } apply {
                within {
                    SpreadZ(qubits[1], qubits[2..Length(qubits) - 1]);
                } apply {
                    Rzz(angle, qubits[0], qubits[1]);
                }
            }
        }
    }
    adjoint ... {
        Exp(paulis, -theta, qubits);
    }
}

/// # Summary
/// Applies the Hadamard transformation to a single qubit.
///
/// # Input
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     H \mathrel{:=}
///     \frac{1}{\sqrt{2}}
///     \begin{bmatrix}
///         1 & 1 \\\\
///         1 & -1
///     \end{bmatrix}
/// \end{align}
/// $$
operation H(qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__h__body(qubit);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__h__body(qubit);
        } elif Length(ctls) == 1 {
            CH(ctls[0], qubit);
        } elif Length(ctls) == 2 {
            CCH(ctls[0], ctls[1], qubit);
        } else {
            use aux = Qubit[Length(ctls) - 1 - (Length(ctls) % 2)];
            within {
                CollectControls(ctls, aux, 0);
            } apply {
                if Length(ctls) % 2 != 0 {
                    CCH(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                } else {
                    CCH(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                }
            }
        }
    }
    adjoint self;
}

/// # Summary
/// Performs the identity operation (no-op) on a single qubit.
///
/// # Remarks
/// This is a no-op. It is provided for completeness and because
/// sometimes it is useful to call the identity in an algorithm or to pass it as a parameter.
operation I(target : Qubit) : Unit is Adj + Ctl {
    body ... {}
    adjoint self;
}

/// # Summary
/// Performs a measurement of a single qubit in the
/// Pauli _Z_ basis.
///
/// # Input
/// ## qubit
/// Qubit to be measured.
///
/// # Output
/// `Zero` if the +1 eigenvalue is observed, and `One` if
/// the -1 eigenvalue is observed.
///
/// # Remarks
/// The output result is given by
/// the distribution
/// $$
/// \begin{align}
///     \Pr(\texttt{Zero} | \ket{\psi}) =
///         \braket{\psi | 0} \braket{0 | \psi}.
/// \end{align}
/// $$
///
/// Equivalent to:
/// ```qsharp
/// Measure([PauliZ], [qubit]);
/// ```
operation M(qubit : Qubit) : Result {
    Measure([PauliZ], [qubit])
}

/// # Summary
/// Performs a joint measurement of one or more qubits in the
/// specified Pauli bases.
///
/// If the basis array and qubit array are different lengths, then the
/// operation will fail.
///
/// # Input
/// ## bases
/// Array of single-qubit Pauli values indicating the tensor product
/// factors on each qubit.
/// ## qubits
/// Register of qubits to be measured.
///
/// # Output
/// `Zero` if the +1 eigenvalue is observed, and `One` if
/// the -1 eigenvalue is observed.
///
/// # Remarks
/// The probability of getting `Zero` is
/// $\bra{\psi} \frac{I + P_0 \otimes \ldots \otimes P_{N-1}}{2} \ket{\psi}$
/// where $P_i$ is the $i$-th element of `bases`, and where
/// $N$ is the `Length(bases)`.
/// That is, measurement returns a `Result` $d$ such that the eigenvalue of the
/// observed measurement effect is $(-1)^d$.
operation Measure(bases : Pauli[], qubits : Qubit[]) : Result {
    if Length(bases) != Length(qubits) {
        fail "Arrays 'bases' and 'qubits' must be of the same length.";
    }
    if Length(bases) == 1 {
        within {
            MapPauliAxis(PauliZ, bases[0], qubits[0]);
        } apply {
            __quantum__qis__m__body(qubits[0])
        }
    } else {
        use aux = Qubit();
        within {
            H(aux);
        } apply {
            for i in 0..Length(bases) - 1 {
                EntangleForJointMeasure(bases[i], aux, qubits[i]);
            }
        }
        __quantum__qis__mresetz__body(aux)
    }
}

/// # Summary
/// Applies a rotation about the given Pauli axis.
///
/// # Input
/// ## pauli
/// Pauli operator (μ) to be exponentiated to form the rotation.
/// ## theta
/// Angle in radians about which the qubit is to be rotated.
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_{\mu}(\theta) \mathrel{:=}
///     e^{-i \theta \sigma_{\mu} / 2},
/// \end{align}
/// $$
/// where $\mu \in \{I, X, Y, Z\}$.
///
/// When called with `pauli = PauliI`, this operation applies
/// a *global phase*. This phase can be significant
/// when used with the `Controlled` functor.
operation R(pauli : Pauli, theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
    if (pauli == PauliX) {
        Rx(theta, qubit);
    } elif (pauli == PauliY) {
        Ry(theta, qubit);
    } elif (pauli == PauliZ) {
        Rz(theta, qubit);
    } else {
        // PauliI
        ApplyGlobalPhase(-theta / 2.0);
    }
}

/// # Summary
/// Applies a rotation about the |1⟩ state by a given angle.
///
/// # Input
/// ## theta
/// Angle about which the qubit is to be rotated.
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_1(\theta) \mathrel{:=}
///     \operatorname{diag}(1, e^{i\theta}).
/// \end{align}
/// $$
///
/// Equivalent to:
/// ```qsharp
/// R(PauliZ, theta, qubit);
/// R(PauliI, -theta, qubit);
/// ```
operation R1(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
    Rz(theta, qubit);
    R(PauliI, -theta, qubit);
}

/// # Summary
/// Applies a rotation about the |1⟩ state by an angle specified
/// as a dyadic fraction.
///
/// WARNING:
/// This operation uses the **opposite** sign convention from
/// Std.Intrinsic.R.
///
/// # Input
/// ## numerator
/// Numerator in the dyadic fraction representation of the angle
/// by which the qubit is to be rotated. This angle is expressed in radians.
/// ## power
/// Power of two specifying the denominator of the angle by which
/// the qubit is to be rotated. This angle is expressed in radians.
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_1(n, k) \mathrel{:=}
///     \operatorname{diag}(1, e^{i \pi n / 2^k}).
/// \end{align}
/// $$
///
/// Equivalent to:
/// ```qsharp
/// RFrac(PauliZ, -numerator, denominator + 1, qubit);
/// RFrac(PauliI, numerator, denominator + 1, qubit);
/// ```
operation R1Frac(numerator : Int, power : Int, qubit : Qubit) : Unit is Adj + Ctl {
    RFrac(PauliZ, -numerator, power + 1, qubit);
    RFrac(PauliI, numerator, power + 1, qubit);
}

/// # Summary
/// Given a single qubit, measures it and ensures it is in the |0⟩ state
/// such that it can be safely released.
///
/// # Input
/// ## qubit
/// The qubit whose state is to be reset to |0⟩.
operation Reset(qubit : Qubit) : Unit {
    __quantum__qis__reset__body(qubit);
}

/// # Summary
/// Given an array of qubits, measure them and ensure they are in the |0⟩ state
/// such that they can be safely released.
///
/// # Input
/// ## qubits
/// An array of qubits whose states are to be reset to |0⟩.
operation ResetAll(qubits : Qubit[]) : Unit {
    for q in qubits {
        Reset(q);
    }
}

/// # Summary
/// Applies a rotation about the given Pauli axis by an angle specified
/// as a dyadic fraction.
///
/// WARNING:
/// This operation uses the **opposite** sign convention from
/// Std.Intrinsic.R.
///
/// # Input
/// ## pauli
/// Pauli operator to be exponentiated to form the rotation.
/// ## numerator
/// Numerator in the dyadic fraction representation of the angle
/// by which the qubit is to be rotated. This angle is expressed in radians.
/// ## power
/// Power of two specifying the denominator of the angle by which
/// the qubit is to be rotated. This angle is expressed in radians.
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_{\mu}(n, k) \mathrel{:=}
///     e^{i \pi n \sigma_{\mu} / 2^k},
/// \end{align}
/// $$
/// where $\mu \in \{I, X, Y, Z\}$.
///
/// Equivalent to:
/// ```qsharp
/// // PI() is a Q# function that returns an approximation of π.
/// R(pauli, -2.0 * PI() * IntAsDouble(numerator) / IntAsDouble(2 ^ (power - 1)), qubit);
/// ```
operation RFrac(pauli : Pauli, numerator : Int, power : Int, qubit : Qubit) : Unit is Adj + Ctl {
    // Note that power must be converted to a double and used with 2.0 instead of 2 to allow for
    // negative exponents that result in a fractional denominator.
    let angle = ((-2.0 * PI()) * IntAsDouble(numerator)) / (2.0^IntAsDouble(power));
    R(pauli, angle, qubit);
}

/// # Summary
/// Applies a rotation about the _x_-axis by a given angle.
///
/// # Input
/// ## theta
/// Angle about which the qubit is to be rotated.
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_x(\theta) \mathrel{:=}
///     e^{-i \theta \sigma_x / 2} =
///     \begin{bmatrix}
///         \cos \frac{\theta}{2} & -i\sin \frac{\theta}{2}  \\\\
///         -i\sin \frac{\theta}{2} & \cos \frac{\theta}{2}
///     \end{bmatrix}.
/// \end{align}
/// $$
///
/// Equivalent to:
/// ```qsharp
/// R(PauliX, theta, qubit);
/// ```
operation Rx(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__rx__body(theta, qubit);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__rx__body(theta, qubit);
        } else {
            within {
                MapPauliAxis(PauliZ, PauliX, qubit);
            } apply {
                Controlled Rz(ctls, (theta, qubit));
            }
        }
    }
    adjoint ... {
        Rx(-theta, qubit);
    }
}

/// # Summary
/// Applies the two qubit Ising _XX_ rotation gate.
///
/// # Input
/// ## theta
/// The angle about which the qubits are rotated.
/// ## qubit0
/// The first qubit input to the gate.
/// ## qubit1
/// The second qubit input to the gate.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_{xx}(\theta) \mathrel{:=}
///     \begin{bmatrix}
///         \cos \theta & 0 & 0 & -i\sin \theta  \\\\
///         0 & \cos \theta & -i\sin \theta & 0  \\\\
///         0 & -i\sin \theta & \cos \theta & 0  \\\\
///         -i\sin \theta & 0 & 0 & \cos \theta
///     \end{bmatrix}.
/// \end{align}
/// $$
operation Rxx(theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__rxx__body(theta, qubit0, qubit1);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__rxx__body(theta, qubit0, qubit1);
        } elif Length(ctls) == 1 {
            CRxx(ctls[0], theta, qubit0, qubit1);
        } else {
            use aux = Qubit[Length(ctls) - 1];
            within {
                CollectControls(ctls, aux, 0);
                AdjustForSingleControl(ctls, aux);
            } apply {
                CRxx(aux[Length(ctls) - 2], theta, qubit0, qubit1);
            }
        }
    }
    adjoint ... {
        Rxx(-theta, qubit0, qubit1);
    }
}

/// # Summary
/// Applies a rotation about the _y_-axis by a given angle.
///
/// # Input
/// ## theta
/// Angle about which the qubit is to be rotated.
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_y(\theta) \mathrel{:=}
///     e^{-i \theta \sigma_y / 2} =
///     \begin{bmatrix}
///         \cos \frac{\theta}{2} & -\sin \frac{\theta}{2}  \\\\
///         \sin \frac{\theta}{2} & \cos \frac{\theta}{2}
///     \end{bmatrix}.
/// \end{align}
/// $$
///
/// Equivalent to:
/// ```qsharp
/// R(PauliY, theta, qubit);
/// ```
operation Ry(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__ry__body(theta, qubit);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__ry__body(theta, qubit);
        } else {
            within {
                MapPauliAxis(PauliZ, PauliY, qubit);
            } apply {
                Controlled Rz(ctls, (theta, qubit));
            }
        }
    }
    adjoint ... {
        Ry(-theta, qubit);
    }
}

/// # Summary
/// Applies the two qubit Ising _YY_ rotation gate.
///
/// # Input
/// ## theta
/// The angle about which the qubits are rotated.
/// ## qubit0
/// The first qubit input to the gate.
/// ## qubit1
/// The second qubit input to the gate.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_{yy}(\theta) \mathrel{:=}
///     \begin{bmatrix}
///         \cos \theta & 0 & 0 & i\sin \theta  \\\\
///         0 & \cos \theta & -i\sin \theta & 0  \\\\
///         0 & -i\sin \theta & \cos \theta & 0  \\\\
///         i\sin \theta & 0 & 0 & \cos \theta
///     \end{bmatrix}.
/// \end{align}
/// $$
operation Ryy(theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__ryy__body(theta, qubit0, qubit1);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__ryy__body(theta, qubit0, qubit1);
        } elif Length(ctls) == 1 {
            CRyy(ctls[0], theta, qubit0, qubit1);
        } else {
            use aux = Qubit[Length(ctls) - 1];
            within {
                CollectControls(ctls, aux, 0);
                AdjustForSingleControl(ctls, aux);
            } apply {
                CRyy(aux[Length(ctls) - 2], theta, qubit0, qubit1);
            }
        }
    }
    adjoint ... {
        Ryy(-theta, qubit0, qubit1);
    }
}

/// # Summary
/// Applies a rotation about the _z_-axis by a given angle.
///
/// # Input
/// ## theta
/// Angle about which the qubit is to be rotated.
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_z(\theta) \mathrel{:=}
///     e^{-i \theta \sigma_z / 2} =
///     \begin{bmatrix}
///         e^{-i \theta / 2} & 0 \\\\
///         0 & e^{i \theta / 2}
///     \end{bmatrix}.
/// \end{align}
/// $$
///
/// Equivalent to:
/// ```qsharp
/// R(PauliZ, theta, qubit);
/// ```
operation Rz(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__rz__body(theta, qubit);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__rz__body(theta, qubit);
        } elif Length(ctls) == 1 {
            CRz(ctls[0], theta, qubit);
        } else {
            use aux = Qubit[Length(ctls) - 1];
            within {
                CollectControls(ctls, aux, 0);
                AdjustForSingleControl(ctls, aux);
            } apply {
                CRz(aux[Length(ctls) - 2], theta, qubit);
            }
        }
    }
    adjoint ... {
        Rz(-theta, qubit);
    }
}

/// # Summary
/// Applies the two qubit Ising _ZZ_ rotation gate.
///
/// # Input
/// ## theta
/// The angle about which the qubits are rotated.
/// ## qubit0
/// The first qubit input to the gate.
/// ## qubit1
/// The second qubit input to the gate.
///
/// # Remarks
/// $$
/// \begin{align}
///     R_{zz}(\theta) \mathrel{:=}
///     \begin{bmatrix}
///         e^{-i \theta / 2} & 0 & 0 & 0 \\\\
///         0 & e^{i \theta / 2} & 0 & 0 \\\\
///         0 & 0 & e^{i \theta / 2} & 0 \\\\
///         0 & 0 & 0 & e^{-i \theta / 2}
///     \end{bmatrix}.
/// \end{align}
/// $$
operation Rzz(theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__rzz__body(theta, qubit0, qubit1);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__rzz__body(theta, qubit0, qubit1);
        } elif Length(ctls) == 1 {
            CRzz(ctls[0], theta, qubit0, qubit1);
        } else {
            use aux = Qubit[Length(ctls) - 1];
            within {
                CollectControls(ctls, aux, 0);
                AdjustForSingleControl(ctls, aux);
            } apply {
                CRzz(aux[Length(ctls) - 2], theta, qubit0, qubit1);
            }
        }
    }
    adjoint ... {
        Rzz(-theta, qubit0, qubit1);
    }
}

/// # Summary
/// Applies the π/4 phase gate to a single qubit.
///
/// # Input
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     S \mathrel{:=}
///     \begin{bmatrix}
///         1 & 0 \\\\
///         0 & i
///     \end{bmatrix}.
/// \end{align}
/// $$
operation S(qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__s__body(qubit);
    }
    adjoint ... {
        __quantum__qis__s__adj(qubit);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__s__body(qubit);
        } elif Length(ctls) == 1 {
            CS(ctls[0], qubit);
        } elif Length(ctls) == 2 {
            Controlled CS([ctls[0]], (ctls[1], qubit));
        } else {
            use aux = Qubit[Length(ctls) - 2];
            within {
                CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
            } apply {
                if Length(ctls) % 2 != 0 {
                    Controlled CS([ctls[Length(ctls) - 1]], (aux[Length(ctls) - 3], qubit));
                } else {
                    Controlled CS([aux[Length(ctls) - 3]], (aux[Length(ctls) - 4], qubit));
                }
            }
        }
    }
    controlled adjoint (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__s__adj(qubit);
        } elif Length(ctls) == 1 {
            Adjoint CS(ctls[0], qubit);
        } elif Length(ctls) == 2 {
            Controlled Adjoint CS([ctls[0]], (ctls[1], qubit));
        } else {
            use aux = Qubit[Length(ctls) - 2];
            within {
                CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
            } apply {
                if Length(ctls) % 2 != 0 {
                    Controlled Adjoint CS([ctls[Length(ctls) - 1]], (aux[Length(ctls) - 3], qubit));
                } else {
                    Controlled Adjoint CS([aux[Length(ctls) - 3]], (aux[Length(ctls) - 4], qubit));
                }
            }
        }
    }
}

/// # Summary
/// Applies the square root of X gate to a single qubit.
///
/// # Input
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     SX \mathrel{:=}
///     \begin{bmatrix}
///         \frac{1}{2} + \frac{i}{2} & \frac{1}{2} - \frac{i}{2} \\\\
///         \frac{1}{2} - \frac{i}{2} & \frac{1}{2} + \frac{i}{2}
///     \end{bmatrix}.
/// \end{align}
/// $$
operation SX(qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__sx__body(qubit);
    }
    adjoint ... {
        __quantum__qis__x__body(qubit);
        __quantum__qis__sx__body(qubit);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__sx__body(qubit);
        } elif Length(ctls) == 1 {
            within {
                H(qubit);
            } apply {
                CS(ctls[0], qubit);
            }
        } elif Length(ctls) == 2 {
            within {
                H(qubit);
            } apply {
                Controlled CS([ctls[0]], (ctls[1], qubit));
            }
        } else {
            use aux = Qubit[Length(ctls) - 2];
            within {
                CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                H(qubit);
            } apply {
                if Length(ctls) % 2 != 0 {
                    Controlled CS([ctls[Length(ctls) - 1]], (aux[Length(ctls) - 3], qubit));
                } else {
                    Controlled CS([aux[Length(ctls) - 3]], (aux[Length(ctls) - 4], qubit));
                }
            }
        }
    }
    controlled adjoint (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__x__body(qubit);
            __quantum__qis__sx__body(qubit);
        } elif Length(ctls) == 1 {
            within {
                H(qubit);
            } apply {
                Adjoint CS(ctls[0], qubit);
            }
        } elif Length(ctls) == 2 {
            within {
                H(qubit);
            } apply {
                Controlled Adjoint CS([ctls[0]], (ctls[1], qubit));
            }
        } else {
            use aux = Qubit[Length(ctls) - 2];
            within {
                CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                H(qubit);
            } apply {
                if Length(ctls) % 2 != 0 {
                    Controlled Adjoint CS([ctls[Length(ctls) - 1]], (aux[Length(ctls) - 3], qubit));
                } else {
                    Controlled Adjoint CS([aux[Length(ctls) - 3]], (aux[Length(ctls) - 4], qubit));
                }
            }
        }
    }
}

/// # Summary
/// Applies the SWAP gate to a pair of qubits.
///
/// # Input
/// ## qubit1
/// First qubit to be swapped.
/// ## qubit2
/// Second qubit to be swapped.
///
/// # Remarks
/// $$
/// \begin{align}
///     \operatorname{SWAP} \mathrel{:=}
///     \begin{bmatrix}
///         1 & 0 & 0 & 0 \\\\
///         0 & 0 & 1 & 0 \\\\
///         0 & 1 & 0 & 0 \\\\
///         0 & 0 & 0 & 1
///     \end{bmatrix},
/// \end{align}
/// $$
///
/// where rows and columns are ordered as in the quantum concepts guide.
///
/// Equivalent to:
/// ```qsharp
/// CNOT(qubit1, qubit2);
/// CNOT(qubit2, qubit1);
/// CNOT(qubit1, qubit2);
/// ```
operation SWAP(qubit1 : Qubit, qubit2 : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__swap__body(qubit1, qubit2);
    }
    adjoint self;
    controlled (ctls, ...) {
        if (Length(ctls) == 0) {
            __quantum__qis__swap__body(qubit1, qubit2);
        } else {
            within {
                CNOT(qubit1, qubit2);
            } apply {
                Controlled CNOT(ctls, (qubit2, qubit1));
            }
        }
    }
}

/// # Summary
/// Applies the π/8 gate to a single qubit.
///
/// # Input
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     T \mathrel{:=}
///     \begin{bmatrix}
///         1 & 0 \\\\
///         0 & e^{i \pi / 4}
///     \end{bmatrix}.
/// \end{align}
/// $$
operation T(qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__t__body(qubit);
    }
    adjoint ... {
        __quantum__qis__t__adj(qubit);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__t__body(qubit);
        } elif Length(ctls) == 1 {
            CT(ctls[0], qubit);
        } else {
            use aux = Qubit[Length(ctls) - 1];
            within {
                CollectControls(ctls, aux, 0);
                AdjustForSingleControl(ctls, aux);
            } apply {
                CT(aux[Length(ctls) - 2], qubit);
            }
        }
    }
    controlled adjoint (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__t__adj(qubit);
        } elif Length(ctls) == 1 {
            Adjoint CT(ctls[0], qubit);
        } else {
            use aux = Qubit[Length(ctls) - 1];
            within {
                CollectControls(ctls, aux, 0);
                AdjustForSingleControl(ctls, aux);
            } apply {
                Adjoint CT(aux[Length(ctls) - 2], qubit);
            }
        }
    }
}

/// # Summary
/// Applies the Pauli _X_ gate.
///
/// # Input
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     \sigma_x \mathrel{:=}
///     \begin{bmatrix}
///         0 & 1 \\\\
///         1 & 0
///     \end{bmatrix}.
/// \end{align}
/// $$
operation X(qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__x__body(qubit);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__x__body(qubit);
        } elif Length(ctls) == 1 {
            __quantum__qis__cx__body(ctls[0], qubit);
        } elif Length(ctls) == 2 {
            __quantum__qis__ccx__body(ctls[0], ctls[1], qubit);
        } else {
            use aux = Qubit[Length(ctls) - 2];
            within {
                CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
            } apply {
                if Length(ctls) % 2 != 0 {
                    __quantum__qis__ccx__body(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                } else {
                    __quantum__qis__ccx__body(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                }
            }
        }
    }
    adjoint self;
}

/// # Summary
/// Applies the Pauli _Y_ gate.
///
/// # Input
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     \sigma_y \mathrel{:=}
///     \begin{bmatrix}
///         0 & -i \\\\
///         i & 0
///     \end{bmatrix}.
/// \end{align}
/// $$
operation Y(qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__y__body(qubit);
    }
    controlled (ctls, ...) {
        if (Length(ctls) == 0) {
            __quantum__qis__y__body(qubit);
        } elif (Length(ctls) == 1) {
            __quantum__qis__cy__body(ctls[0], qubit);
        } elif (Length(ctls) == 2) {
            CCY(ctls[0], ctls[1], qubit);
        } else {
            use aux = Qubit[Length(ctls) - 2];
            within {
                CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
            } apply {
                if Length(ctls) % 2 != 0 {
                    CCY(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                } else {
                    CCY(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                }
            }
        }
    }
    adjoint self;
}

/// # Summary
/// Applies the Pauli _Z_ gate.
///
/// # Input
/// ## qubit
/// Qubit to which the gate should be applied.
///
/// # Remarks
/// $$
/// \begin{align}
///     \sigma_z \mathrel{:=}
///     \begin{bmatrix}
///         1 & 0 \\\\
///         0 & -1
///     \end{bmatrix}.
/// \end{align}
/// $$
operation Z(qubit : Qubit) : Unit is Adj + Ctl {
    body ... {
        __quantum__qis__z__body(qubit);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            __quantum__qis__z__body(qubit);
        } elif Length(ctls) == 1 {
            __quantum__qis__cz__body(ctls[0], qubit);
        } elif Length(ctls) == 2 {
            CCZ(ctls[0], ctls[1], qubit);
        } else {
            use aux = Qubit[Length(ctls) - 2];
            within {
                CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
            } apply {
                if Length(ctls) % 2 != 0 {
                    CCZ(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                } else {
                    CCZ(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                }
            }
        }
    }
    adjoint self;
}

/// # Summary
/// Applies the given unitary matrix to the given qubits. The matrix is checked at runtime to ensure it's shape is square and that the matrix dimensions are `2 ^ Length(qubits)`.
/// This operation is simulator-only and is not supported on hardware.
///
/// # Input
/// ## matrix
/// The unitary matrix to apply.
/// ## qubits
/// The qubits to which the unitary matrix should be applied.
///
/// # Example
/// This performs a two qubit CNOT using the unitary matrix representation:
/// ```qsharp
/// import Std.Math.Complex;
/// use qs = Qubit[2];
/// let one = new Complex { Real = 1.0, Imag = 0.0 };
/// let zero = new Complex { Real = 0.0, Imag = 0.0 };
/// ApplyUnitary(
///     [
///         [one, zero, zero, zero],
///         [zero, one, zero, zero],
///         [zero, zero, zero, one],
///         [zero, zero, one, zero]
///     ],
///     qs
/// );
/// ```
@Config(Unrestricted)
operation ApplyUnitary(matrix : Complex[][], qubits : Qubit[]) : Unit {
    let num_rows = Length(matrix);
    for col in matrix {
        if Length(col) != num_rows {
            fail "matrix passed to ApplyUnitary must be square.";
        }
    }

    let num_qubits = Length(qubits);
    if num_rows != 1 <<< num_qubits {
        fail "matrix passed to ApplyUnitary must have dimensions 2^Length(qubits).";
    }

    Apply(matrix, qubits);
}

@Config(Unrestricted)
operation Apply(matrix : Complex[][], qubits : Qubit[]) : Unit {
    body intrinsic;
}

// Applies unitary matrix on given qubits that is a permutation matrix for
// given permutation.
@Config(Unrestricted)
operation ApplyPermutationUnitary(permutation : Int[], qubits : Qubit[]) : Unit {
    body intrinsic;
}

/// # Summary
/// Logs a message.
///
/// # Input
/// ## msg
/// The message to be reported.
///
/// # Remarks
/// The specific behavior of this function is simulator-dependent,
/// but in most cases the given message will be written to the console.
/// ```
function Message(msg : String) : Unit {
    body intrinsic;
}

export AND, CCNOT, CNOT, Exp, H, I, M, Measure, R, R1, R1Frac, Reset, ResetAll, RFrac, Rx, Rxx, Ry, Ryy, Rz, Rzz, S, SWAP, SX, T, X, Y, Z, ApplyUnitary, Message;
