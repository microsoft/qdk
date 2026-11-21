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

    let a = MZZ(control, ancilla);
    let b = MXX(ancilla, target);
    let c = MZ(ancilla);
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
    if MX(q) == One {
        Z(q);
    }
}

/// Prepare a qubit in the |0⟩ state.
operation PrepareZ(q : Qubit) : Unit {
    if MZ(q) == One {
        X(q);
    }
}

/// Prepare a Bell Pair.
/// Source: [1] Figure 18a.
operation BellPair(q1 : Qubit, q2 : Qubit) : Unit {
    // Collapse the qubits onto the Pauli-Z basis.
    MZ(q1);
    MZ(q2);

    // If they are not aligned in the Pauli-X basis, phase flip one of them.
    if MXX(q1, q2) == One {
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
    let z = MZZ(q1, q2);
    let x = MXX(q1, q2);
    (x, z)
}
