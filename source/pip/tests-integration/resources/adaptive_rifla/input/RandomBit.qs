namespace Test {

    import Std.Intrinsic.*;

    @EntryPoint()
    operation Main() : Result {
        use q = Qubit();
        H(q);
        let r = M(q);
        Reset(q);
        return r;
    }
}
