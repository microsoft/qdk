// A set of custom measurements exposed from a hardware
// provider using Majorana Qubits.

@Measurement()
@SimulatableIntrinsic()
operation __quantum__qis__custom__mx__body(q : Qubit) : Result {
    // TODO(pranavjain): this is the right fix for now, but when we get to joint measurements
    // later we might consider reworking this whole example file. It serves two purposes right
    // now: introducing @Measurement() for defining custom measurements and introducing Measurement-Based-Quantum-Computing.
    // I think we can port it over to the full set of new intrinsics when it is ready and still use it to explain MBQC, just ditch the first purpose.
    H(q);
    M(q)
}

@Measurement()
@SimulatableIntrinsic()
operation __quantum__qis__mz__body(q : Qubit) : Result {
    M(q)
}

@Measurement()
@SimulatableIntrinsic()
operation __quantum__qis__mxx__body(q1 : Qubit, q2 : Qubit) : Result {
    Std.Intrinsic.Measure([PauliX, PauliX], [q1, q2])
}

@Measurement()
@SimulatableIntrinsic()
operation __quantum__qis__mzz__body(q1 : Qubit, q2 : Qubit) : Result {
    Std.Intrinsic.Measure([PauliZ, PauliZ], [q1, q2])
}
