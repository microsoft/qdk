namespace Test {
    operation Main() : (Int, Int) {
        use q = Qubit();
        mutable a = 1;
        mutable r = 0;
        for i in 0..0 {
            X(q);
            let t = a;
            if i == 7 { X(q); }
            a = 5;
            r = t;
        }
        mutable a = 1;
        mutable r2 = 0;
        for i in 0..1 {
            X(q);
            let t = a;
            if i == 7 { X(q); }
            a = 5;
            r2 = t;
        }
        Reset(q);
        (r, r2)
    }
}
