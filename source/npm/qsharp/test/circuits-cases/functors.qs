operation Main() : Result[] {
    use q = Qubit();
    Foo(q);
    Adjoint Foo(q);
    use controls = Qubit[2];
    Controlled Foo(controls, q);
    Controlled Adjoint Foo(controls, q);
    [MResetZ(q)]
}

operation Foo(q : Qubit) : Unit is Adj + Ctl {

    body (...) {
        X(q);
    }

    adjoint (...) {
        X(q);
    }

    controlled (cs, ...) {
        Controlled X(cs, q);
    }
}
