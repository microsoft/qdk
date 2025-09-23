operation Main() : Unit {
    use qs = Qubit[2];

    H(qs[0]);
    H(qs[1]);

    if (M(qs[0]) == One) {
        let r1 = M(qs[1]);
    }
    ResetAll(qs)

}
