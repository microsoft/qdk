namespace Test {
    operation Main() : Int[] {
        use q = Qubit();
        X(q);
        mutable n = 0;
        if MResetZ(q) == One {
            n = 2;
        }
        mutable a = [0, 0] w/ 0 <- n;
        n = 3;
        if MResetZ(q) == One {
            n = 7;
        }
        a[1] = n;
        n = 5;
        a
    }
}
