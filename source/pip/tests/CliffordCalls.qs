@EntryPoint(Base)
operation Main() : Unit {
    let gates = [H, X, Y, Z, S, Adjoint S, SX];
    let NUM_QUBITS = 1224;
    let MAX_CALLS = 1_000_000;
    use qubits = Qubit[NUM_QUBITS];
    mutable calls = 0;
    while calls < MAX_CALLS {
        for qubit in qubits {
            for gate in gates {
                if calls < MAX_CALLS {
                    gate(qubit);
                    calls += 1;
                }
            }
        }
        for i in 0..2..NUM_QUBITS-1 {
            if calls < MAX_CALLS {
                CZ(qubits[i], qubits[i + 1]);
                calls += 1;
            }
        }
    }
}
