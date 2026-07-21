// A set of custom measurements exposed from a hardware
// provider using Majorana Qubits.

@Measurement()
@SimulatableIntrinsic()
operation __quantum__qis__mx__body(q : Qubit) : Result {
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
