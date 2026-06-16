namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Demonstrates copy and update expressions.
    // Expected output: ([1], [0], [1, 1, 1])
    @EntryPoint()
    operation Main() : (Result[], Result[], Result[]) {
        use qubitA = Qubit();
        X(qubitA);
        let resultsA = [Zero] w/ 0 <- MResetZ(qubitA);

        // Simple concatenated copy and update expressions.
        use qubitB = Qubit();
        let resultsB = [Zero]
            w/ 0 <- One
            w/ 0 <- MResetZ(qubitB);

        // Copy and update expression that make use of ranges.
        use registerC = Qubit[3];
        X(registerC[0]);
        mutable resultsC = MeasureEachZ(registerC);
        ApplyToEachCA(X, registerC[1..2]);
        set resultsC w/= 1..2 <- MeasureEachZ(registerC[1..2]);
        return (
            resultsA,
            resultsB,
            resultsC
        );
    }
}
