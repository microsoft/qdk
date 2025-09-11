// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


import Std.Arrays.*;
import Std.Core.*;
import Std.Math.*;
import Std.Intrinsic.*;
import QIR.Intrinsic.*;

internal operation CH(control : Qubit, target : Qubit) : Unit is Adj {
    within {
        S(target);
        H(target);
        T(target);
    } apply {
        CNOT(control, target);
    }
}

internal operation CCH(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
    within {
        S(target);
        H(target);
        T(target);
    } apply {
        CCNOT(control1, control2, target);
    }
}

internal operation ApplyGlobalPhase(theta : Double) : Unit is Ctl + Adj {
    body ... {
        ControllableGlobalPhase(theta);
    }
    adjoint ... {
        ControllableGlobalPhase(-theta);
    }
}

// Global phase is not relevant for physical systems, but controlled global phase is physical. We use
// the Rz gate to implement controlled global phase physically, and then correct for the extra global phase it
// introduces in simulation using additional calls to the simulation-only global phase intrinsic.
// We use a separate operation for this controlled case to avoid recursive calls to the same operation
// that can interfere with runtime capabilities analysis.
internal operation ControllableGlobalPhase(theta : Double) : Unit is Ctl {
    body ... {
        GlobalPhase([], theta);
    }
    controlled (ctls, ...) {
        if Length(ctls) == 0 {
            GlobalPhase([], theta);
        } else {
            Controlled Rz(ctls[1...], (theta, ctls[0]));
            GlobalPhase(ctls[1...], theta / 2.0);
        }
    }
}

// Global phase intrinsic, which only has affect in simulation and is a no-op otherwise.
internal operation GlobalPhase(ctls : Qubit[], theta : Double) : Unit {
    body intrinsic;
}

internal operation CRz(control : Qubit, theta : Double, target : Qubit) : Unit is Adj {
    Rz(theta / 2.0, target);
    CNOT(control, target);
    Rz(-theta / 2.0, target);
    CNOT(control, target);
}

internal operation CS(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
    T(control);
    T(target);
    CNOT(control, target);
    Adjoint T(target);
    CNOT(control, target);
}

internal operation CT(control : Qubit, target : Qubit) : Unit is Adj {
    let angle = PI() / 8.0;
    Rz(angle, control);
    Rz(angle, target);
    CNOT(control, target);
    Adjoint Rz(angle, target);
    CNOT(control, target);
    // This decomposition for controlled-T introduces a global phase (due to the unmatched call to Rz from above).
    // We correct for this global phase in simulation, which is a no-op on hardware.
    ApplyGlobalPhase(angle / 2.0);
}

internal operation EntangleForJointMeasure(basis : Pauli, aux : Qubit, qubit : Qubit) : Unit {
    if basis == PauliX {
        __quantum__qis__cx__body(aux, qubit);
    } elif basis == PauliZ {
        __quantum__qis__cz__body(aux, qubit);
    } elif basis == PauliY {
        __quantum__qis__cy__body(aux, qubit);
    }
}

/// Collects the given list of control qubits into one or two of the given auxiliary qubits, using
/// all but the last qubits in the auxiliary list as scratch qubits. The auxiliary list must be
/// big enough to accommodate the data, so it is usually smaller than controls list by number of
/// qubits needed for the eventual controlled unitary application. The passed adjustment value is
/// used to ensure the right number of auxiliary qubits are processed.
///
/// For example, if the controls list is 6 qubits, the auxiliary list must be 5 qubits, and the
/// state from the 6 control qubits will be collected into the last qubit of the auxiliary array.
internal operation CollectControls(ctls : Qubit[], aux : Qubit[], adjustment : Int) : Unit is Adj {
    // First collect the controls into the first part of the auxiliary list.
    for i in 0..2..(Length(ctls) - 2) {
        AND(ctls[i], ctls[i + 1], aux[i / 2]);
    }
    // Then collect the auxiliary qubits in the first part of the list forward into the last
    // qubit of the auxiliary list. The adjustment is used to allow the caller to reduce or increase
    // the number of times this is run based on the eventual number of control qubits needed.
    for i in 0..((Length(ctls) / 2) - 2 - adjustment) {
        AND(aux[i * 2], aux[(i * 2) + 1], aux[i + Length(ctls) / 2]);
    }
}

/// When collecting controls, if there is an uneven number of original control qubits then the
/// last control and the second to last auxiliary will be collected into the last auxiliary.
internal operation AdjustForSingleControl(ctls : Qubit[], aux : Qubit[]) : Unit is Adj {
    if Length(ctls) % 2 != 0 {
        AND(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], aux[Length(ctls) - 2]);
    }
}

internal operation PhaseCCX(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
    // https://arxiv.org/pdf/1210.0974.pdf#page=2
    H(target);
    CNOT(target, control1);
    CNOT(control1, control2);
    T(control2);
    Adjoint T(control1);
    T(target);
    CNOT(target, control1);
    CNOT(control1, control2);
    Adjoint T(control2);
    CNOT(target, control2);
    H(target);
}

internal operation CCZ(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
    within {
        MapPauliAxis(PauliX, PauliZ, target);
    } apply {
        CCNOT(control1, control2, target);
    }
}

internal operation CCY(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
    within {
        MapPauliAxis(PauliX, PauliY, target);
    } apply {
        CCNOT(control1, control2, target);
    }
}

internal operation CRxx(control : Qubit, theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit {
    within {
        MapPauliAxis(PauliZ, PauliX, qubit0);
        MapPauliAxis(PauliZ, PauliX, qubit1);
    } apply {
        CRzz(control, theta, qubit0, qubit1);
    }
}

internal operation CRyy(control : Qubit, theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit {
    within {
        MapPauliAxis(PauliZ, PauliY, qubit0);
        MapPauliAxis(PauliZ, PauliY, qubit1);
    } apply {
        CRzz(control, theta, qubit0, qubit1);
    }
}

internal operation CRzz(control : Qubit, theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit {
    within {
        CNOT(qubit1, qubit0);
    } apply {
        Controlled Rz([control], (theta, qubit0));
    }
}

internal function IndicesOfNonIdentity(paulies : Pauli[]) : Int[] {
    mutable indices = [];
    for i in 0..Length(paulies) - 1 {
        if (paulies[i] != PauliI) {
            set indices += [i];
        }
    }
    indices
}

internal function RemovePauliI(paulis : Pauli[], qubits : Qubit[]) : (Pauli[], Qubit[]) {
    let indices = IndicesOfNonIdentity(paulis);
    let newPaulis = Subarray(indices, paulis);
    let newQubits = Subarray(indices, qubits);
    return (newPaulis, newQubits);
}

internal operation SpreadZ(from : Qubit, to : Qubit[]) : Unit is Adj {
    let targets = GetSpread(from, to);
    for (ctl, tgt) in targets {
        CNOT(ctl, tgt);
    }
}

internal function GetSpread(from : Qubit, to : Qubit[]) : (Qubit, Qubit)[] {
    mutable queue = [(from, to)];
    mutable targets = [];
    while Length(queue) > 0 {
        mutable (next, rest) = (queue[0], queue[1...]);
        set queue = rest;
        let (next_from, next_to) = next;
        if Length(next_to) > 0 {
            set targets = [(next_to[0], next_from)] + targets;
            if Length(next_to) > 1 {
                let half = Length(next_to) / 2;
                set queue = [(next_from, next_to[1..half]), (next_to[0], next_to[(half + 1)...])] + rest;
            }
        }
    }

    targets
}
