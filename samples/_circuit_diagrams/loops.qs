operation Main() : Unit {
    Foo();
    use qs = Qubit[2];
    for iteration in 1..10 {
        H(qs[0]);
        X(qs[0]);
        CNOT(qs[0], qs[1]);
        Message("hi");
    }
}

operation Foo() : Unit {
    use qs = Qubit[2];
    for iteration in 1..10 {
        H(qs[0]);
        X(qs[0]);
        CNOT(qs[0], qs[1]);
        Message("hi");
    }
}
