operation Main() : Result[] {
    use (q1, q2) = (Qubit(), Qubit());
    H(q1);
    Rz(Std.Math.PI() / 4.0, q1);
    Rx(2.0 * Std.Math.PI() / 3.0, q2);
    Ry(-Std.Math.PI() / 2.0, q2);
    Rz(0.4321, q1);
    MResetEachZ([q1, q2])
}
