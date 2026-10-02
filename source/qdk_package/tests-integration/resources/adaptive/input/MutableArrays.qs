namespace Test {
    import Std.Convert.ResultArrayAsBoolArray;
    operation Main() : Bool[] {
        use qs = Qubit[3];
        ApplyToEach(X, qs);
        mutable arr = [false, size = Length(qs) + 1];
        let res = ResultArrayAsBoolArray(MResetEachZ(qs));
        for i in 0..Length(res)-1 {
            arr[i + 1] = res[i];
        }
        arr[1..2...]
    }
}
