namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Reusing a qubit after `M` should work on supported platforms, be replaced by entanglement with auxiliary on others.
    // Reusing a qubit after `Reset` should work on supported platforms, be replaced by newly allocated qubit on others.
    // Expected output: (0, 1, 1, 0, 0)
    @EntryPoint()
    operation Main() : (Result, Result, Result, Result, Result) {
        use q = Qubit();
        let r1 = M(q);
        X(q);
        let r2 = M(q);
        H(q);
        let r3 = MResetX(q);
        let r4 = MResetZ(q);
        H(q);
        Adjoint S(q);
        H(q);
        let r5 = MResetY(q);
        return (r1, r2, r3, r4, r5);
    }
}
