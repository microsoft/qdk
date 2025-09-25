/// # Sample
/// Simple Quantum Phase Estimation
///
/// # Description
/// This sample demonstrates how to use ApplyQPE to perform
/// Quantum Phase Estimation of a simple unitary operation.
/// Rz gate is used as the unitary operation,
/// and the eigenvector |1⟩ is chosen for the phase estimation.
/// Run this sample and check the histogram of results.
/// This sample is suitable for base profile.

/// # Summary
/// Estimate the phase of the eigenvalue of the unitary gate U
/// using Quantum Phase Estimation (QPE) algorithm.
/// The result is returned as an array of `Result` values
/// representing the binary fraction of the phase in big-endian order.
/// This is used to display the histogram of the results.
operation Main() : Result[] {
    // Allocate qubits to be used in phase estimation
    use state = Qubit();
    use phase = Qubit[6];

    // Prepare Rz eigenvector |1⟩
    X(state);

    // ApplyOperationPowerCA is used to apply the unitary operation U multiple times
    // as required by the Quantum Phase Estimation algorithm. In general, unitary U may
    // operate on multiple qubits, but in this case it operates on a single qubit qs[0].
    let oracle = ApplyOperationPowerCA(_, qs => U(qs[0]), _);

    // Estimate eigenvalue of Rz gate corresponding eigenvector |1⟩
    // In general, unitary U may operate on multiple qubits, but in this case
    // it operates on a single qubit state. So a single element array [state] is passed
    // as the target state.
    ApplyQPE(oracle, [state], phase);

    // Measure each qubit in the phase register
    let results = MeasureEachZ(phase);

    // Reset the qubits
    Reset(state);
    ResetAll(phase);

    // For the gate Rz(π/3) and the eigenvector |1⟩, the eigenvalue should be exp(i * π/6).
    // Its phase π/6 is 1/12 as a frction of 2π, or ~0.0001010101 in binary.
    // Estimation is done with the precision of 6 binary digits so the expected histogram
    // should peak at [Zero, Zero, Zero, One, Zero, One] (big-endian order),
    // with the smaller bar at the next value [Zero, Zero, Zero, One, One, Zero]

    // Reverse the order of the results to present in big-endian
    // and return one data point for histogram display
    Std.Arrays.Reversed(results)
}

// U is the unitary gate for which we want to perform phase estimation.
operation U(q : Qubit) : Unit is Ctl + Adj {
    // Using the Rz gate with the angle of π/3
    // The eigenvalues of this gate are exp(-i * π/6) for |0⟩ and exp(i * π/6) for |1⟩.
    Rz(Std.Math.PI() / 3.0, q);
}

