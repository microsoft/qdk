namespace Test {

    import Std.Arrays.*;
    import Std.Intrinsic.*;

    // Demonstrates use of measurement comparisons, including ternary.
    // Expected output: (true, false, true, true)
    @EntryPoint()
    operation Main() : (Bool, Bool, Bool, Bool) {
        use (q0, q1) = (Qubit(), Qubit());
        X(q0);
        CNOT(q0, q1);
        let (r0, r1) = (M(q0), M(q1));
        Reset(q0);
        Reset(q1);
        return (r0 == One, r1 == Zero, r0 == r1, r0 == Zero ? false | true);
    }
}
