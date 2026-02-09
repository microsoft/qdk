// # Sample
// Quantum Memory
//
// # Description
// The primary quantum feature of Q# is its representation of qubits and qubit
// memory.
//
// Q# supports allocation of qubits with the `use` keyword.
// Allocated qubits start in the |0⟩ state.
//
// Qubits are automatically released at the end of their scope.
// Before a qubit is released, it must be returned to |0⟩ (for example, by
// calling `Reset` after temporarily using it).

operation Main() : Unit {
    // Allocate a single qubit and an array of qubits.
    use q = Qubit();
    use qs = Qubit[3];

    // If you change a qubit's state, reset it before leaving scope.
    X(q);
    Reset(q);

    // The same rule applies to arrays of qubits.
    for qi in qs {
        X(qi);
        Reset(qi);
    }
}
