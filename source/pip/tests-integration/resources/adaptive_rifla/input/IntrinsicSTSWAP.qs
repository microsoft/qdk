namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Verifies the use of the S, SWAP and T operations from Q#'s Microsoft.Quantum.Intrinsic namespace.
    // Expected simulation output: (1, 1, [1, 0]).
    @EntryPoint()
    operation Main() : (Result, Result, Result[]) {
        // Exercise S operation.
        // N.B. The S operation is equivalent to sqrt(Z).
        use sTarget = Qubit();  // |0⟩
        H(sTarget);             // |+⟩
        S(sTarget);             // sqrt(Z)|+⟩
        S(sTarget);             // sqrt(Z)^2|+⟩ ≡ Z|+⟩ ≡ |-⟩
        H(sTarget);             // |1⟩
        let sResult = MResetZ(sTarget);

        // Exercise T operation.
        // N.B. The T operation is equivalent to sqrt(S).
        use tTarget = Qubit();  // |0⟩
        H(tTarget);             // |+⟩
        T(tTarget);             // sqrt(S)|+⟩
        T(tTarget);             // sqrt(S)^2|+⟩
        T(tTarget);             // sqrt(S)^3|+⟩
        T(tTarget);             // sqrt(S)^4|+⟩ ≡ S^2|+⟩ ≡ sqrt(Z)^2|+⟩ ≡ Z|+⟩ ≡ |-⟩
        H(tTarget);             // |1⟩
        let tResult = MResetZ(tTarget);

        // Exercise SWAP operation.
        use swapRegister = Qubit[2];
        X(swapRegister[1]);
        SWAP(swapRegister[0], swapRegister[1]);
        let swapResults = [MResetZ(swapRegister[0]), MResetZ(swapRegister[1])];
        return (sResult, tResult, swapResults);
    }
}
