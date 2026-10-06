namespace Test {
    operation Main() : (Int, (Int, Int)) {
        use q = Qubit();
        X(q);
        mutable n = 0;
        mutable b = 0;
        mutable c = 3;
        if MResetZ(q) == One {
            n = 2;
            b = 4;
        }
        let a = [n];
        (b, c) = (c, b);
        n = 7;
        (a[0], (b, c))
    }
}
