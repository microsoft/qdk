// # Sample
// Quantum Memory
//
// # Description
// The primary quantum feature of Q# is its representation of qubits and qubit
// memory.
//
// Q# supports allocation of qubits with the `use` keyword.
// Allocated qubits start in the |0⟩ state.

operation Main() : Unit {
    // Allocates a single qubit.
    use q = Qubit();

    // Allocates an array of qubits.
    use qs = Qubit[3];

    // Allocates multiple qubits at once via tuple destructuring.
    use (control, target) = (Qubit(), Qubit());

    // Mixed allocation patterns are also possible.
    use (q2, qs2) = (Qubit(), Qubit[3]);
}
