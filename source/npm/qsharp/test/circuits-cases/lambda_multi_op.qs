operation Main() : Result[] {
    use q = Qubit();
    let lambda = (q => { H(q); S(q); });
    lambda(q);
    [MResetZ(q)]
}
