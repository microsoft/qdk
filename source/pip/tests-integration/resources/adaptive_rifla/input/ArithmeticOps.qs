namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Demonstrates use of arithmetic operations on integers at runtime.
    // Expected output: (5, 25, 0, 243)
    @EntryPoint()
    operation Main() : (Int, Int, Int, Int) {
        mutable count = 0;
        mutable countPos = 0;
        mutable countNeg = 10;
        mutable countMul = 1;
        use qs = Qubit[5];
        for q in qs {
            X(q);
        }
        for r in MeasureEachZ(qs) {
            if r == One {
                // Note that addition of a 1 will get optimized into a zext on the bool.
                set count += 1;

                set countPos += 5;

                // Note that subtraction of 2 turns into add of -2... problem for providers without negative numbers?
                set countNeg -= 2;

                set countMul *= 3;
            }
        }
        ResetAll(qs);
        return (count, countPos, countNeg, countMul);
    }
}
