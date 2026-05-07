namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Verifies the use of the CNOT quantum gate from Q#'s Microsoft.Quantum.Intrinsic namespace.
    // Expected simulation output: ([0, 0], [1, 1]).
    @EntryPoint()
    operation Main() : (Result[], Result[]) {
        use registerA = Qubit[2];           // |00⟩
        CNOT(registerA[0], registerA[1]);   // |00⟩
        let resultsA = MeasureEachZ(registerA);
        ResetAll(registerA);

        use registerB = Qubit[2];           // |00⟩
        X(registerB[0]);                    // |10⟩
        CNOT(registerB[0], registerB[1]);   // |11⟩
        let resultsB = MeasureEachZ(registerB);
        ResetAll(registerB);

        return (resultsA, resultsB);
    }
}
