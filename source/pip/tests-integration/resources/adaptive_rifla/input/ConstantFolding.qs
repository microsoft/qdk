namespace Test {

    import Std.Intrinsic.*;
    import Std.Canon.*;
    import Std.Measurement.*;

    // Verifies constant folding, unrolling, optimizing out of invalid array access, array concatenation.
    // Expected output: [1, 1, 1, 1]
    @EntryPoint()
    operation Main() : Result[] {
        let nQubits = 3;
        let iterations = nQubits * 3;
        let secondRun = true;

        use qs = Qubit[nQubits];
        X(qs[0]);
        if nQubits > 1 {
            for _ in 1..iterations {
                for q in qs[1...] {
                    CNOT(qs[0], q);
                }
            }
        }

        let nQubits2 = 1;
        let pi = Microsoft.Quantum.Math.PI() / 2.0;
        use qs2 = Qubit[nQubits2];
        if secondRun {
            Rx(pi * 2.0, qs2[0]);
            if nQubits2 > 1 {
                for _ in 1..iterations {
                    for q in qs2[1...] {
                        CNOT(qs2[0], q);
                    }
                }
            }
        }

        let results = MeasureEachZ(qs) + MeasureEachZ(qs2);
        ResetAll(qs);
        ResetAll(qs2);
        return results;
    }
}
