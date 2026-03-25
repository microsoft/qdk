namespace Test {

    import Std.Intrinsic.*;

    // Demonstrates use of double comparisons.
    // Expected output: (10.0, true, false, true, true, false)
    @EntryPoint()
    operation Main() : (Double, Bool, Bool, Bool, Bool, Bool, Int, Double) {
        mutable count = 0.0;
        use q = Qubit();
        for _ in 1..10 {
            X(q);
            if M(q) == One {
                X(q);
                set count += 1.0;
                set count *= 1.0;
                set count -= 1.0;
                set count /= 1.0;
                set count += 1.0;
            }
        }
        Reset(q);
        let countInteger = Std.Math.Truncate(count);
        let countIntegerAsDouble = Std.Convert.IntAsDouble(countInteger);
        return (count, count > 5.0, count < 5.0, count >= 10.0, count == 10.0, count != 10.0, countInteger, countIntegerAsDouble);
    }
}
