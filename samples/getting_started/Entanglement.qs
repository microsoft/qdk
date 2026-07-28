// # Summary
// Entanglement sample
//
// # Description
// This Q# program entangles two qubits and measures them.
//
// # Remarks
// Qubits are said to be entangled when the state of each one of them
// cannot be described independently from the state of the others.
operation Main() : Result[] {
    // Allocate the two qubits that will be entangled.
    use q1 = Qubit();
    use q2 = Qubit();

    // Set the first qubit in superposition by calling the `H` operation,
    // which applies a Hadamard transformation to the qubit.
    H(q1);
    // Entangle the two qubits using the `CNOT` operation.
    CNOT(q1, q2);

    // Show the entangled state using the `DumpMachine` function.
    // Note that the state is a superposition of |00〉 and |11〉,
    // but not |01〉 and |10〉.
    Std.Diagnostics.DumpMachine();

    // Create an array (register) out of the two qubits, measure each qubit,
    // reset each qubit, return the array of measurement results.
    MResetEachZ([q1, q2])
}
