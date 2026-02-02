import Std.Arrays.*;
import Std.Math.*;
import Std.Convert.*;

/// # Summary
/// Implements Grover's algorithm, which searches all possible inputs to an
/// operation to find a particular marked state. This is the same implementation
/// as found in the Grover's Search sample in the QDK samples repository.
operation GroverSearch(
    nQubits : Int,
    iterations : Int,
    phaseOracle : Qubit[] => Unit
) : Result[] {

    use qubits = Qubit[nQubits];

    // Initialize a uniform superposition over all possible inputs.
    PrepareUniform(qubits);

    // The search itself consists of repeatedly reflecting about the marked
    // state and our start state, which we can write out in Q# as a for loop.
    for _ in 1..iterations {
        phaseOracle(qubits);
        ReflectAboutUniform(qubits);
    }

    // Measure and return the answer.
    return MResetEachZ(qubits);
}

/// # Summary
/// Given a register in the all-zeros state, prepares a uniform
/// superposition over all basis states.
operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
    ApplyToEachCA(H, inputQubits);
}

/// # Summary
/// Reflects about the uniform superposition state.
operation ReflectAboutUniform(inputQubits : Qubit[]) : Unit {
    within {
        // Transform the uniform superposition to all-zero.
        Adjoint PrepareUniform(inputQubits);
        // Transform the all-zero state to all-ones
        ApplyToEachA(X, inputQubits);
    } apply {
        // Now that we've transformed the uniform superposition to the
        // all-ones state, reflect about the all-ones state, then let the
        // within/apply block transform us back.
        ReflectAboutAllOnes(inputQubits);
    }
}

/// # Summary
/// Reflects about the all-ones state.
operation ReflectAboutAllOnes(inputQubits : Qubit[]) : Unit {
    Controlled Z(Most(inputQubits), Tail(inputQubits));
}

/// # Summary
/// Returns the optimal number of Grover iterations needed to find a marked
/// item, given the number of qubits in a register. Setting the number of
/// iterations to a different number may undershoot or overshoot the marked state.
function IterationsToMarked(nQubits : Int) : Int {
    if nQubits > 126 {
        fail "This sample supports at most 126 qubits.";
    }

    let nItems = 2.0^IntAsDouble(nQubits);
    let angle = ArcSin(1. / Sqrt(nItems));
    let iterations = Round(0.25 * PI() / angle - 0.5);
    iterations
}
