namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Verifies the use of the M gate from Q#'s Microsoft.Quantum.Intrinsic namespace.
    // Expected simulation output: (0, 1).
    @EntryPoint()
    operation Main() : (Result, Result) {
        use qubitA = Qubit();
        let resultA = M(qubitA);
        use qubitB = Qubit();
        X(qubitB);
        let resultB = M(qubitB);
        Reset(qubitA);
        Reset(qubitB);
        return (resultA, resultB);
    }
}
