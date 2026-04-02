namespace Test {

    import Std.Intrinsic.*;

    // Demonstrates use of integer comparisons.
    // Expected output: (true, false, true)
    @EntryPoint()
    operation Main() : (Bool, Bool, Bool) {
        mutable count = 0;
        use q = Qubit();
        for _ in 1..10 {
            X(q);
            if M(q) == One {
                X(q);
                set count += 1;
            }
        }
        Reset(q);
        return (count > 5, count < 5, count == 10);
    }
}
