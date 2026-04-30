namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Verifies the use of the CCNOT quantum gate from Q#'s Microsoft.Quantum.Intrinsic namespace.
    // Expected simulation output: ([0, 0, 0], [1, 0, 0], [1, 1, 1]).
    @EntryPoint()
    operation Main() : (Result[], Result[], Result[]) {
        use registerA = Qubit[3];                           // |000⟩
        CCNOT(registerA[0], registerA[1], registerA[2]);    // |000⟩
        let resultsA = MeasureEachZ(registerA);
        ResetAll(registerA);

        use registerB = Qubit[3];                           // |000⟩
        X(registerB[0]);                                    // |100⟩
        CCNOT(registerB[0], registerB[1], registerB[2]);    // |100⟩
        let resultsB = MeasureEachZ(registerB);
        ResetAll(registerB);

        use registerC = Qubit[3];                           // |000⟩
        X(registerC[0]);                                    // |100⟩
        X(registerC[1]);                                    // |110⟩
        CCNOT(registerC[0], registerC[1], registerC[2]);    // |111⟩
        let resultsC = MeasureEachZ(registerC);
        ResetAll(registerC);

        return (resultsA, resultsB, resultsC);
    }
}
