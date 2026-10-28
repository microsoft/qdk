operation Main() : (Int, Bool, Double, Result[], Unit) {
    use q = Qubit();
    X(q);
    (42, true, 0.5, [MResetZ(q)], ())
}
