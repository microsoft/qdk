// A set of gates built upon the custom measurements
// provided by the hardware provider.
//
// Source:
//  [1] Surface code compilation via edge-disjoint paths
//      https://arxiv.org/pdf/2110.11493

/// Apply a CNOT gate to the given qubits.
/// Source: [1] Figure 3.
operation CNOT(control : Qubit, target : Qubit) : Unit {
    // Prepare an ancilla qubit in the |+⟩ state.
    use ancilla = Qubit();
    PrepareX(ancilla);

    let a = Mzz(control, ancilla);
    let b = Mxx(ancilla, target);
    let c = Mz(ancilla);
    Reset(ancilla);

    if b == One {
        Z(control);
    }

    if a != c {
        X(target);
    }
}


/// Prepare a qubit in the |+⟩ state.
operation PrepareX(q : Qubit) : Unit {
    if Mx(q) == One {
        Z(q);
    }
}

/// Prepare a qubit in the |0⟩ state.
operation PrepareZ(q : Qubit) : Unit {
    if Mz(q) == One {
        X(q);
    }
}

/// Prepare a Bell Pair.
/// Source: [1] Figure 18a.
operation BellPair(q1 : Qubit, q2 : Qubit) : Unit {
    // Collapse the qubits onto the Pauli-Z basis.
    Mz(q1);
    Mz(q2);

    // If they are not aligned in the Pauli-X basis, phase flip one of them.
    if Mxx(q1, q2) == One {
        Z(q2);
    }
}

/// Measure a Bell Pair.
/// Source: [1] Figure 18b.
/// Below is a map showing how the Bell states map to the Result pairs:
///   |𝚽⁺⟩ -> (Zero, Zero)
///   |𝚿⁺⟩ -> (Zero, One)
///   |𝚽⁻⟩ -> (One, Zero)
///   |𝚿⁻⟩ -> (One, One)
operation BellMeasurement(q1 : Qubit, q2 : Qubit) : (Result, Result) {
    let z = Mzz(q1, q2);
    let x = Mxx(q1, q2);
    (x, z)
}

/// User friendly wrapper around the Mx hardware gate.
operation Mx(q : Qubit) : Result {
    HardwareIntrinsics.__quantum__qis__mx__body(q)
}

/// User friendly wrapper around the Mz hardware gate.
operation Mz(q : Qubit) : Result {
    HardwareIntrinsics.__quantum__qis__mz__body(q)
}

/// User friendly wrapper around the Mxx hardware gate.
operation Mxx(q1 : Qubit, q2 : Qubit) : Result {
    HardwareIntrinsics.__quantum__qis__mxx__body(q1, q2)
}

/// User friendly wrapper around the Mzz hardware gate.
operation Mzz(q1 : Qubit, q2 : Qubit) : Result {
    HardwareIntrinsics.__quantum__qis__mzz__body(q1, q2)
}
