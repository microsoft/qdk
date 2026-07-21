// # Sample
// Using pre-calculated logical counts for resource estimation
//
// # Description
// This sample demonstrates how to use the `AccountForEstimates` function to
// estimate the resources required to run a factoring program from pre-calculated
// logical counts. The logical counts used in this sample are the ones obtained
// for a 2048-bit integer factoring application based on the implementation
// described in [[Quantum 5, 433 (2021)](https://quantum-journal.org/papers/q-2021-04-15-433/)].
// Our implementation incorporates all techniques described in the paper, except for
// carry runways.
import Std.ResourceEstimation.*;

operation Main() : Unit {
    use qubits = Qubit[12581];

    AccountForEstimates(
        [TCount(12), RotationCount(12), RotationDepth(12), CczCount(3731607428), MeasurementCount(1078154040)],
        PSSPCLayout(),
        qubits
    );
}
