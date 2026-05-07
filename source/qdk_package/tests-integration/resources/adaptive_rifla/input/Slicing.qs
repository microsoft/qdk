namespace Test {

    import Std.Canon.*;
    import Std.Intrinsic.*;
    import Std.Measurement.*;

    // Verifies loop over subset of index range, constant folding division of array length, array slicing, qubit reindexing, reverse iteration.
    // Expected output: [1, 1, 1, 1, 1].
    @EntryPoint()
    operation Main() : Result[] {
        use qs = Qubit[10];
        for i in (Length(qs) - 1).. -1..(Length(qs) / 2) {
            X(qs[i]);
        }
        let results = MeasureEachZ(qs[Length(qs) / 2...]);
        ResetAll(qs);
        return results;
    }

}
