namespace Test {
    operation Main() : Int {
        use q = Qubit();
        X(q);
        mutable n = 0;
        if MResetZ(q) == One {
            n = 2;
        }
        let a = [n];
        n = 7;
        a[0]
    }
}
