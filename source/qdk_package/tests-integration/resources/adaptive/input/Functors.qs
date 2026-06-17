namespace Test {

    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Verifies use of Q# functors.
    // Expected simulation output:
    // ([0, 0], [1, 0], [0, 0]) -> 0.5
    // ([0, 0], [1, 1], [0, 0]) -> 0.5
    @EntryPoint()
    operation Main() : (Result[], Result[], Result[]) {
        use targetsA = Qubit[2];
        Unitary(targetsA);
        Adjoint Unitary(targetsA);

        use controls = Qubit[2];
        use targetsB = Qubit[2];
        within {
            for q in controls {
                X(q);
            }
        } apply {
            Controlled Unitary(controls, targetsB);
        }

        use targetsC = Qubit[2];
        within {
            for q in controls {
                X(q);
            }
        } apply {
            Controlled Unitary(controls, targetsC);
            Controlled Adjoint Unitary(controls, targetsC);
        }

        let rA = MeasureEachZ(targetsA);
        let rB = MeasureEachZ(targetsB);
        let rC = MeasureEachZ(targetsC);
        ResetAll(controls);
        ResetAll(targetsA);
        ResetAll(targetsB);
        ResetAll(targetsC);
        return (rA, rB, rC);
    }

    operation Unitary(register : Qubit[]) : Unit is Adj + Ctl {
        X(register[0]);
        H(register[1]);
        Z(register[1]);
    }
}
