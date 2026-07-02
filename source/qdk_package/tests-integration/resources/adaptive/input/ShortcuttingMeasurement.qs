namespace Test {

    import Std.Intrinsic.*;

    // Demonstrates shortcutting of measurement ops in conditionals.
    // Expected output: (0, 0)
    @EntryPoint()
    operation Main() : (Result, Result) {
        use (q0, q1) = (Qubit(), Qubit());
        X(q0);
        CNOT(q0, q1);
        if M(q0) != Zero or M(q1) != Zero {
            X(q0);
            X(q1);
        }
        let (r0, r1) = (M(q0), M(q1));
        Reset(q0);
        Reset(q1);
        return (r0, r1);
    }

}
