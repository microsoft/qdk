import Std.Diagnostics.*;

// # Summary
// Joint Measurement sample
//
// # Description
// This Q# program demonstrates how to use Joint measurements.
// Joint measurement, also known as Pauli measurements, are a generalization
// of 2-outcome measurements to multiple qubits and other bases.
operation Main() : (Result, Result[]) {
    // Prepare an entangled state.
    use qs = Qubit[2];  // |00〉
    H(qs[0]);           // (|00〉 + |10〉)/sqrt(2)
    CNOT(qs[0], qs[1]); // (|00〉 + |11〉)/sqrt(2)

    // Show the quantum state before performing the joint measurement.
    DumpMachine();

    // The below code uses a joint measurement as a way to check the parity
    // of the two qubits. In this case, the parity measurement result
    // will always be `Zero`.
    // Notice how the state was not collapsed by the joint measurement
    // because this state is the eigenvector of the Z⊗Z operator.
    let parityResult = Measure([PauliZ, PauliZ], qs);
    DumpMachine();

    // However, if we perform a measurement just on the first qubit, we can
    // see how the state collapses.
    let firstQubitResult = M(qs[0]);
    DumpMachine();

    // Measuring the last qubit does not change the quantum state
    // since the state of the second qubit collapsed when the first qubit
    // was measured because they were entangled.
    let secondQubitResult = M(qs[1]);
    DumpMachine();

    // Reset qubits before they are released.
    ResetAll(qs);

    // Return results
    (parityResult, [firstQubitResult, secondQubitResult])
}
