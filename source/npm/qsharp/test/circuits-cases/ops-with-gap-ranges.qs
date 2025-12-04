operation Main() : Unit {
    use qs = Qubit[10];
    Foo(qs);
    Bar(qs);
    MResetEachZ(qs);
}

operation Foo(qs : Qubit[]) : Unit {
    Rxx(1.0, qs[0], qs[2]);
    Rxx(1.0, qs[4], qs[6]);
}

operation Bar(qs : Qubit[]) : Unit {
    Rxx(1.0, qs[1], qs[3]);
    Rxx(1.0, qs[5], qs[9]);
}
