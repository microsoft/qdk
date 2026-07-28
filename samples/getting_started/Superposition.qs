// # Summary
// Superposition sample
//
// # Description
// This Q# program sets a qubit in a superposition of the computational basis
// states |0〉 and |1〉 by applying a Hadamard transformation.
operation Main() : Result {
    // Allocate a qubit. Qubit is in |0〉 state after allocation.
    use qubit = Qubit();
    // Qubits are only accessible for the duration of the scope where they
    // are allocated and are automatically released at the end of the scope.

    // Set the qubit in superposition by applying a Hadamard transformation.
    H(qubit);

    // Show the quantum state (when running on a simulator).
    Std.Diagnostics.DumpMachine();

    // Measure then reset the qubit. Return the result.
    // There is a 50% probability of measuring either `Zero` or `One`.
    MResetZ(qubit)
}
