operation Main() : Unit {
    use q = Qubit();
    Foo(q);
    Foo(q);
    Bar(q);
}

operation Bar(q : Qubit) : Unit {
    Foo(q);
    for _ in 1..2 {
        X(q);
        Y(q);
    }
}

operation Foo(q : Qubit) : Unit {
    H(q);
}
