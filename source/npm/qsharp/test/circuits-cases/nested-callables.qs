operation Main() : Unit {
    use q1 = Qubit();
    use q2 = Qubit();
    Bar(q1);
    Bar(q2);
    Foo(q1);
    Bar(q1);
    Foo(q1);
    Foo(q2);
    Bar(q2);
    Foo(q2);
}
operation Foo(q : Qubit) : Unit {
    Bar(q);
    MResetZ(q);
}
operation Bar(q : Qubit) : Unit {
    X(q);
    Y(q);
}
