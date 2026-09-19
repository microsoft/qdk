operation Ising2DTrotter() : Result[] {
    use qs = Qubit[3];
    Rx(0.7, qs[0]);
    Rx(0.7, qs[2]);
    Rzz(0.41, qs[0], qs[2]);
    Rx(-0.3, qs[0]);
    Rzz(0.41, qs[2], qs[0]);
    Rx(0.29, qs[2]);
    return Std.Measurement.MResetEachZ(qs);
}