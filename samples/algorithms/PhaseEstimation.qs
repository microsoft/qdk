/// # Sample
/// Quantum Phase Estimation
///
/// # Description
/// This sample demonstrates how to use Quantum Phase Estimation (QPE) algorithm
/// to estimate the eigenvalue of a specific unitary operation.
/// QPE algorithm is performed by calling `ApplyQPE` library operation.
/// Run this sample and check the histogram of results.

import Std.Math.*;

/// # Summary
/// Estimate the the eigenvalue of a specific unitary U for its eigenvector |010⟩
/// using `ApplyQPE`. The result is returned as a complex polar value.
operation Main() : ComplexPolar {
    // Allocate qubits to be used in phase estimation
    use state = Qubit[3];

    // Prepare U eigenvector |010⟩
    X(state[1]);

    // Estimate eigenvalue of U corresponding eigenvector |010⟩
    let eigenvalue = EstimateEigenvalue(U, state, 6);

    // Reset the state qubits
    ResetAll(state);

    // The eigenvalue represented as ComplexPolar should be (1.0, π/3.0)
    // or approximately (1.0, 1.04719755).
    eigenvalue
}

/// # Summary
/// Estimate the eigenvalue of a unitary operation `U` with certain `precision`
/// for a given eigenvector using Quantum Phase Estimation (QPE) algorithm.
operation EstimateEigenvalue(
    U : Qubit[] => Unit is Adj + Ctl,
    eigenstate : Qubit[],
    precision : Int
) : ComplexPolar {
    // Allocate qubits to be used in phase estimation
    use phase = Qubit[precision];

    // Estimate eigenvalue of Rz gate corresponding eigenvector |1⟩
    ApplyQPE(ApplyOperationPowerCA(_, U, _), eigenstate, phase);

    // Measure qubit register `phase` as a binary fraction
    let phaseEstimation = MeasureBinaryFractionLE(phase);

    // Reset the qubits
    ResetAll(phase);

    // Return one data point for histogram display
    new ComplexPolar {
        Magnitude = 1.0,
        Argument = phaseEstimation * 2.0 * Std.Math.PI()
    }
}

/// # Summary
/// Given a qubit register `qs` measure each qubit in the register
/// assume results represent a binary fraction in little-endian order
/// and return the fraction as a `Double` value.
operation MeasureBinaryFractionLE(qs : Qubit[]) : Double {
    mutable result = 0.0;
    mutable power = 1.0;
    for i in Length(qs)-1..-1..0 {
        power /= 2.0;
        if (MResetZ(qs[i]) == One) {
            result += power;
        }
    }
    return result;
}

// U is the unitary gate for which we want to perform phase estimation.
// This operation operates on three qubits and can be represented as an 8x8 matrix.
// As number of qubits gets larger, the matrix representation becomes impractical,
// and finding exact eigenvalues classicaly becomes computationally prohibitive.
// But representation as a quantum circuit can still be concise and efficient.
operation U(qs : Qubit[]) : Unit is Ctl + Adj {
    // Using the Rzz gate with the angle of π/3
    // The eigenvalue of this gate is exp(i * π/3) for |010⟩
    Rzz(Std.Math.PI() / 3.0, qs[0], qs[1]);
    Rzz(Std.Math.PI() / 3.0, qs[1], qs[2]);
}
