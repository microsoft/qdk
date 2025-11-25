operation Main() : Result[] {
    use q = Qubit();
    Foo(q);
    Adjoint Foo(q);
    [MResetZ(q)]
}

operation Foo(q : Qubit) : Unit is Adj + Ctl {

    body (...) {
        X(q);
    }

    adjoint (...) {
        Y(q);
    }

    controlled (cs, ...) {}
}
