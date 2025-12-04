operation Main() : Result[] {
    use q = Qubit();
    let lambda = (q => H(q));
    lambda(q);
    [MResetZ(q)]
}
