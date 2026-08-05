// # Summary
// Greenberger–Horne–Zeilinger state sample
//
// # Description
// This Q# program shows how to prepare a generalized Greenberger–Horne–Zeilinger
// state (aka Cat state) in a register of 5 qubits.
//
// # Remarks
// The Greenberger–Horne–Zeilinger state, or GHZ state, aka Cat state,
// is a state defined as: |GHZ〉 = (|0...0〉 + |1...1〉)/√2.
//
// The GHZ state is said to be a maximally entangled state, a multi-qubit
// state where the state of any one qubit is not separable from the state
// of any of the other qubits.
//
// # References
// - [Greenberger–Horne–Zeilinger state](https://en.wikipedia.org/wiki/Greenberger%E2%80%93Horne%E2%80%93Zeilinger_state)
// - [Cat state](https://en.wikipedia.org/wiki/Cat_state)
operation Main() : Result[] {
    // Allocate 5 qubits for Cat₅ state.
    use cat5 = Qubit[5];
    // Prepare a Cat₅ state in the register.
    PrepareGHZState(cat5);
    // Show the Cat₅ state.
    Std.Diagnostics.DumpMachine();
    // Measure and reset qubits. Return results.
    MResetEachZ(cat5)
}

/// # Summary
/// Prepares GHZ state (|0...0〉 + |1...1〉) / √2 (aka Cat state) in `qs` register.
/// All qubits are assumed to be in |0〉 state on input.
operation PrepareGHZState(qs : Qubit[]) : Unit {
    Std.Diagnostics.Fact(Length(qs) > 0, "Qubit register must not be empty.");

    // Set the first qubit into a (|0〉 + |1〉) / √2 superposition.
    H(qs[0]);

    // Then apply a CNOT to the remaining qubits using the first qubit as control.
    // This entangles first qubit with all the other qubits in the register.
    for q in qs[1...] {
        CNOT(qs[0], q);
    }
}
