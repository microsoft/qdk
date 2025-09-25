operation Main() : Unit {
    use qs = Qubit[2];

    PrepareSomething(qs);
    DoSomethingElse(qs);
    DoSomethingElse(qs);
    DoSomethingDifferent(qs);

    MResetEachZ(qs);

    ResetAll(qs);
}

operation PrepareSomething(qs : Qubit[]) : Unit {
    for iteration in 1..5 {
        H(qs[0]);
        X(qs[0]);
        CNOT(qs[0], qs[1]);
    }
}

operation DoSomethingElse(qs : Qubit[]) : Unit {
    for iteration in 1..5 {
        H(qs[1]);
        X(qs[0]);
        X(qs[1]);
        CNOT(qs[1], qs[0]);
    }
}

operation DoSomethingDifferent(qs : Qubit[]) : Unit {
    for iteration in 1..5 {
        H(qs[0]);
        Z(qs[0]);
        CNOT(qs[0], qs[1]);
    }
}
