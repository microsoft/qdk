operation Main() : Result[] {
    use q = Qubit();
    let lambda = (q => H(q));
    lambda(q);
    let lambda2 = (q => { H(q); S(q); });
    lambda2(q);
    [MResetZ(q)]
}
