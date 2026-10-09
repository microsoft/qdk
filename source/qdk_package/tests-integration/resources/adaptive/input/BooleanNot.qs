namespace Test {
    operation Main() : Bool {
        use q = Qubit();
        X(q);
        let c = MResetZ(q) == One;
        boolNot(c)
    }

    function boolNot(b : Bool) : Bool {
        not b
    }
}
