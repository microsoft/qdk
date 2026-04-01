namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Verifies the I, H, X, Y, and Z quantum gates from Q#'s Microsoft.Quantum.Intrinsic namespace.
    // Expected simulation output: (1, 1, 1, 1, 1, 1).
    @EntryPoint()
    operation Main() : (Result, Result, Result, Result, Result, Result) {
        // Exercise H.
        use hTarget = Qubit();  // |0⟩
        H(hTarget);             // |+⟩
        Z(hTarget);             // |-⟩
        H(hTarget);             // |1⟩
        let hResult = MResetZ(hTarget);

        // Exercise I.
        use iTarget = Qubit();  // |0⟩
        X(iTarget);             // |1⟩
        I(iTarget);             // |1⟩
        let iResult = MResetZ(iTarget);

        // Exercise X.
        use xTarget = Qubit();  // |0⟩
        X(xTarget);             // |1⟩
        let xResult = MResetZ(xTarget);

        // Exercise Y.
        use yTargetA = Qubit(); // |0⟩
        Y(yTargetA);            // i|1⟩
        let yResultA = MResetZ(yTargetA);
        use yTargetB = Qubit(); // |0⟩
        H(yTargetB);            // |+⟩
        Y(yTargetB);            // -i|-⟩
        H(yTargetB);            // -i|1⟩
        let yResultB = MResetZ(yTargetB);

        // Exercise Z.
        use zTarget = Qubit();  // |0⟩
        H(zTarget);             // |+⟩
        Z(zTarget);             // |-⟩
        H(zTarget);             // |1⟩
        let zResult = MResetZ(zTarget);

        return (hResult, iResult, xResult, yResultA, yResultB, zResult);
    }
}
