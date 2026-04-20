namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Verifies use of Q# within apply construct.
    // Expected simulation output: [0, 0, 1]
    @EntryPoint()
    operation Main() : Result[] {
        use target = Qubit();
        use controls = Qubit[2];
        within {
            for q in controls {
                X(q);
            }
        } apply {
            Controlled X(controls, target);
        }

        let results = MeasureEachZ(controls + [target]);
        ResetAll(controls);
        Reset(target);
        return results;
    }
}
