/// # Summary
/// Bell Pair sample
///
/// # Description
/// Bell pairs are specific quantum states of two qubits that represent
/// the simplest (and maximal) examples of quantum entanglement. This sample
/// prepares |Φ⁺⟩ = (|00⟩+|11⟩)/√2. For other Bell states see BellStates.qs
///
/// # References
/// - [Bell state](https://en.wikipedia.org/wiki/Bell_state)
operation Main() : (Result, Result) {
    // Allocate the two qubits that will be used to create a Bell pair.
    use (q1, q2) = (Qubit(), Qubit());

    // Create Bell pair by calling `PrepareBellPair` operation defined below.
    PrepareBellPair(q1, q2);

    // Show the state of qubits using the `DumpMachine` function.
    Std.Diagnostics.DumpMachine();

    // Measure the two qubits and reset them. Return measurement results.
    (MResetZ(q1), MResetZ(q2))
}

/// # Summary
/// Prepare Bell pair |Φ⁺⟩ = (|00⟩+|11⟩)/√2 on two qubits.
/// Qubits are assumed to be in |00⟩ state.
operation PrepareBellPair(q1 : Qubit, q2 : Qubit) : Unit {
    // Set qubit `q1` in superposition of |0⟩ and |1⟩ by applying a Hadamard gate.
    H(q1);

    // Entangle the two qubits `q1` and `q2` using the `CNOT` gate.
    CNOT(q1, q2);
}
