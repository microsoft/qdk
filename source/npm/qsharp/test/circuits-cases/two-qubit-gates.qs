operation Main() : Result[] {
    use (q1, q2, q3) = (Qubit(), Qubit(), Qubit());
    H(q2);
    Rzz(1.2345, q1, q3);
    CNOT(q2, q3);
    SWAP(q1, q2);
    MResetEachZ([q3])
}
