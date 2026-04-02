namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;
    import Std.Math.*;

    // Demonstrates using a computed integer to do a branch that gets turned into a switch instruction
    // (should get transformed back into nested branches).
    // Expected output: 1
    @EntryPoint()
    operation Main() : Result {
        use qs = Qubit[2];
        for q in qs {
            X(q);
        }
        mutable rand = 0;
        for r in MeasureEachZ(qs) {
            set rand <<<= 1;
            if r == One {
                set rand += 1;
            }
        }
        ResetAll(qs);

        use q = Qubit();
        if rand == 0 {
            R(PauliI, PI(), q);
        } elif rand == 1 {
            R(PauliY, PI(), q);
        } elif rand == 2 {
            R(PauliZ, PI(), q);
        } else {
            R(PauliX, PI(), q);
        }
        return MResetZ(q);
    }
}
