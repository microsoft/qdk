namespace Test {
    import Std.Math.PI;
    operation Main() : Result {
        use q = Qubit();
        let arr = [2.0 * PI(), PI(), 2.0 * PI()];
        for a in arr {
            Rx(a, q);
        }
        let arrays = [[PI(), PI(), PI()], arr, [2.0 * PI()]];
        for arr in arrays {
            for a in arr {
                Rx(a, q);
            }
        }
        MResetZ(q)
    }
}
