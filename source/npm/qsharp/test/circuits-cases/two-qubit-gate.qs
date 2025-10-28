operation Main() : (Result, Result) {
    use (q1, q2, q3) = (Qubit(), Qubit(), Qubit());
    H(q2);
    Rzz(1.2345, q1, q3);
    (MResetZ(q1), MResetZ(q2))
}
