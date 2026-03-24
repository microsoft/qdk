// # Sample
// Qubit
//
// # Description
// Q# uses the `Qubit` primitive type to represent quantum state. A qubit represents
// the smallest addressable physical unit in a quantum computer. Qubits are long-lived,
// and accumulates transformations to quantum states. A Q# program has no ability to
// introspect into the state of a qubit, and thus is entirely agnostic about what a
// quantum state is or on how it is realized. Rather, a program can call operations
// such as Measure to learn information about the quantum state of the computation.

import Std.Diagnostics.DumpMachine;

/// In the below code, all variables have type annotations to showcase their type.
operation Main() : Unit {
    // The following statement allocates three qubits.
    use qs = Qubit[3];

    // Allocate a single qubit
    use qubit = Qubit();

    // Apply an H gate to the first qubit.
    H(qs[0]);

    // Apply an H gate to the second qubit.
    H(qs[1]);

    // Apply a Y gate to the third qubit.
    Y(qs[2]);

    // Apply the SWAP gate to qubits 3 and 1.
    SWAP(qs[2], qs[0]);

    // Display the quantum state of the qubits.
    DumpMachine();

    // Reset the allocated qubits.
    ResetAll(qs);

    // Reset the single qubit.
    Reset(qubit);

}
