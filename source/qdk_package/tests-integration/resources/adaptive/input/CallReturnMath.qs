namespace Test {
    operation B(q : Qubit) : Int {
        X(q);
        let r = MResetZ(q);
        return r == One ? 7 | 3;
    }

    operation A(q1 : Qubit, q2 : Qubit) : Int {
        let fromB = B(q2);
        X(q1);
        let r = MResetZ(q1);
        let local = r == One ? 5 | 2;
        return local * fromB + 1;
    }

    @EntryPoint()
    operation Main() : Int {
        use (q1, q2) = (Qubit(), Qubit());
        let result = A(q1, q2);
        return result;
    }
}
