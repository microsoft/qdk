operation Main() : (Result, Result) {
    use (q1, q2) = (Qubit(), Qubit());
    PrepareBellPair(q1, q2);
    (MResetZ(q1), MResetZ(q2))
}

operation PrepareBellPair(q1 : Qubit, q2 : Qubit) : Unit {
    H(q1);
    CNOT(q1, q2);
}
