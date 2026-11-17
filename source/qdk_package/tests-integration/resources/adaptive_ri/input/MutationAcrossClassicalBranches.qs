namespace Test {
    operation Main() : Int {
        use q = Qubit();
        mutable n = 0;
        if true {
            set n = 5;
        }
        if MResetZ(q) == One {
            set n = 1;
        }
        n
    }
}
