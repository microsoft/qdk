/// # Sample
/// Simplified Sample of a Variational Quantum Eigensolver
///
/// # Description
/// This is an example of a Variational Quantum Eigensolver (VQE).
/// This example includes:
///   1. Simple classical optimization to find minimum of a multi-variable function
///      in order to find an approximation to the minimum eigenvalue of a Hamiltonian
///   2. Finding Hamiltonian expectation value as a weighted sum of terms.
///   3. Finding one term expectation value by performing multiple shots.
///   4. Ansatz state preparation similar to the circuit in the referenced paper.
/// To keep this sample simple Hamiltonian terms are generated randomly.
///
/// # Reference
/// Ground-state energy estimation of the water molecule on a trapped ion quantum
/// computer by Yunseong Nam et al., 2019. https://arxiv.org/abs/1902.10171

import Std.Arrays.IsEmpty;
import Std.Arrays.IndexRange;
import Std.Convert.IntAsDouble;
import Std.Diagnostics.Fact;
import Std.Math.AbsD;
import Std.Math.PI;

/// # Summary
/// Find the approximation to the minimum eigenvalue of a Hamiltonian by applying VQE
operation Main() : Double {

    // Find the approximation to the minimum eigenvalue of a Hamiltonian
    // by varying ansatz parameters to minimize its expectation value.
    SimpleDescent(
        // Use a number of shots when estimating Hamiltonian terms
        // Actual VQE implementations may require very large number of shots.
        FindHamiltonianExpectationValue(_, 100),
        // Start from these angles for ansatz state preparation
        [1.0, 1.0],
        // Initial step to search for minimum
        0.5,
        // Stop optimization if step is 0
        0.0,
        // Stop optimization after several attempts.
        // Actual VQE would need to make enough iterations
        // to find energy with sufficient chemical accuracy.
        50
    )
}

/// # Summary
/// Find expectation value of a Hamiltonian given parameters for the
/// ansatz state and number of shots to evaluate each term.
/// Different VQE applications will have different measurements and
/// coefficients depending on the Hamiltonian being evaluated.
operation FindHamiltonianExpectationValue(thetas : Double[], shots : Int) : Double {
    let terms = [
        ([PauliZ, PauliI, PauliI, PauliI], 0.16),
        ([PauliI, PauliI, PauliZ, PauliI], -0.25),
        ([PauliZ, PauliZ, PauliI, PauliI], 0.17),
        ([PauliI, PauliI, PauliZ, PauliZ], 0.45),
        ([PauliX, PauliX, PauliX, PauliX], 0.2),
        ([PauliY, PauliY, PauliY, PauliY], 0.1),
        ([PauliY, PauliX, PauliX, PauliY], -0.02),
        ([PauliX, PauliY, PauliY, PauliX], -0.22),
    ];
    mutable value = 0.0;
    for (basis, coefficient) in terms {
        value += coefficient * FindTermExpectationValue(thetas, basis, shots);
    }
    value
}

/// # Summary
/// Find expectation value of a Hamiltonian term given parameters for the
/// ansatz state, measurement basis and number of shots to evaluate each term.
operation FindTermExpectationValue(
    thetas : Double[],
    pauliBasis : Pauli[],
    shots : Int
) : Double {

    mutable zeroCount = 0;
    for _ in 1..shots {
        use qs = Qubit[4];
        PrepareAnsatzState(qs, thetas);
        if Measure(pauliBasis, qs) == Zero {
            zeroCount += 1;
        }
        ResetAll(qs);
    }
    IntAsDouble(zeroCount) / IntAsDouble(shots)
}

/// # Summary
/// Prepare the ansatz state for given parameters on a qubit register
/// This is an example of ansatz state preparation similar to the
/// unitary couple clustered method used in the referenced paper.
/// Actual VQE application will have different ansatz preparation operations.
operation PrepareAnsatzState(qs : Qubit[], thetas : Double[]) : Unit {
    BosonicExitationTerm(thetas[0], qs[0], qs[2]);
    CNOT(qs[0], qs[1]);
    NonBosonicExitataionTerm(thetas[1], qs[0], qs[1], qs[2], qs[3]);
}

/// # Summary
/// Bosonic excitation circuit from the referenced paper.
operation BosonicExitationTerm(
    theta : Double,
    moX : Qubit,
    moY : Qubit
) : Unit {
    X(moX);
    Adjoint S(moX);
    Rxx(theta, moX, moY);
    S(moX);
    Adjoint S(moY);
    Rxx(-theta, moX, moY);
    S(moY);
}

/// # Summary
/// Non-bosonic excitation circuit from the referenced paper.
operation NonBosonicExitataionTerm(
    theta : Double,
    moXsoX : Qubit,
    moXsoY : Qubit,
    moYsoX : Qubit,
    moYsoY : Qubit
) : Unit {
    Adjoint S(moXsoX);
    within {
        CNOT(moXsoX, moYsoY);
        CNOT(moXsoX, moYsoX);
        CNOT(moXsoX, moXsoY);
        H(moXsoX);
        Rz(theta, moXsoX);
        CNOT(moXsoY, moXsoX);
        Rz(theta, moXsoX);
        CNOT(moYsoY, moXsoX);
        Rz(-theta, moXsoX);
        CNOT(moXsoY, moXsoX);
        Rz(-theta, moXsoX);
    } apply {
        Adjoint S(moYsoX);
        CNOT(moYsoX, moXsoX);
    }
    S(moYsoX);
}

/// # Summary
/// Simple classical optimizer. A descent to a local minimum of function `f`.
/// Tries to takes steps in all directions and proceeds if the new point is better.
/// If no moves result in function value improvement the step size is halved.
/// Actual VQE implementations use more elaborate optimizers.
operation SimpleDescent(
    f : Double[] => Double,
    initialPoint : Double[],
    initialStep : Double,
    minimalStep : Double,
    attemptLimit : Int
) : Double {
    Fact(not IsEmpty(initialPoint), "Argument array must contain elements.");
    Fact(initialStep > 0.0, "Initial step must be positive.");
    Fact(minimalStep >= 0.0, "Minimal step must be non-negative.");

    mutable bestPoint = initialPoint;
    mutable bestValue = f(bestPoint);
    mutable currentStep = initialStep;
    mutable currentAttempt = 0;

    Message($"Beginning descent from value {bestValue}.");

    while (currentAttempt < attemptLimit) and (currentStep > minimalStep) {
        mutable hadImprovement = false;
        for i in IndexRange(initialPoint) {
            let nextPoint = bestPoint w/ i <- bestPoint[i] + currentStep;
            let nextValue = f(nextPoint); // Evaluate quantum part
            currentAttempt = currentAttempt + 1;
            if nextValue < bestValue {
                hadImprovement = true;
                bestValue = nextValue;
                bestPoint = nextPoint;
                Message($"Value improved to {bestValue}.");
            }
            let nextPoint = bestPoint w/ i <- bestPoint[i] - currentStep;
            let nextValue = f(nextPoint); // Evaluate quantum part
            currentAttempt = currentAttempt + 1;
            if nextValue < bestValue {
                hadImprovement = true;
                bestValue = nextValue;
                bestPoint = nextPoint;
                Message($"Value improved to {bestValue}.");
            }
        }

        if not hadImprovement {
            currentStep = currentStep / 2.0;
        }
    }
    Message($"Descent done. Attempts: {currentAttempt}, Step: {currentStep}, Arguments: {bestPoint}, Value: {bestValue}.");
    bestValue
}
