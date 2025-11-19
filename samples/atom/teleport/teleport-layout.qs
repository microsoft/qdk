operation Teleport(msg : Qubit, alice : Qubit, bob : Qubit) : Unit {
    // Create some entanglement that we can use to send our message.
    H(alice);
    CNOT(alice, bob);

    // Encode the message into the entangled pair.
    CNOT(msg, alice);
    H(msg);

    CNOT(alice, bob);
    Controlled Z([msg], bob);
}

@Test()
operation TestTeleportZero() : Unit {
    use qubits = Qubit[3];

    // Message qubit is already in the |0⟩ state
    Teleport(qubits[0], qubits[1], qubits[2]);

    let result = MResetZ(qubits[2]);
    ResetAll([qubits[0], qubits[1]]);

    if result != Zero {
        fail "Teleport failed to transmit |0⟩ state";
    }
}

@Test()
operation TestTeleportSuperposition() : Unit {
    use qubits = Qubit[3];

    // Prepare a test state on the message qubit
    SX(qubits[0]);

    Teleport(qubits[0], qubits[1], qubits[2]);

    // Reverse the state prep on Bob's qubit
    Adjoint SX(qubits[2]);
    let result = MResetZ(qubits[2]);
    ResetAll([qubits[0], qubits[1]]);

    if result != Zero {
        fail "Teleport failed to transmit superposition state";
    }
}

@Test()
operation TestTeleportRotation() : Unit {
    use qubits = Qubit[3];

    // Prepare a test state on the message qubit
    Ry(0.5, qubits[0]);

    Teleport(qubits[0], qubits[1], qubits[2]);

    // Reverse the state prep on Bob's qubit
    Rz(-0.5, qubits[2]);
    let result = MResetZ(qubits[2]);
    ResetAll([qubits[0], qubits[1]]);

    if result != Zero {
        fail "Teleport failed to transmit arbitrary rotation state";
    }
}


operation Layout(instances : Int) : Result[] {
    // Partitions the teleport instances across rows and columns on the machine
    let cols = if instances >= 12 { 36 } else { instances * 3 };
    let rows = (instances + 11) / 12;  // 1 to 12 = 1, 13 to 24 = 2, etc.

    use qubits = Qubit[instances * 3];
    mutable results : Result[] = [];

    for i in 0..instances-1 {
        let rowId = i / 12;
        let colId = (i % 12) * 3;
        let idx = colId + (rowId * 36);

        // Prep state on msg qubit
        if i % 4 == 1 {
            X(qubits[idx]);
        } elif i % 4 == 2 {
            H(qubits[idx]);
        } elif i % 4 == 3 {
            SX(qubits[idx]);
        }

        Teleport(qubits[idx], qubits[idx + 1], qubits[idx + 2]);

        // Reverse state prep on Bob's qubit
        if i % 4 == 1 {
            X(qubits[idx + 2]);
        } elif i % 4 == 2 {
            H(qubits[idx + 2]);
        } elif i % 4 == 3 {
            X(qubits[idx + 2]);
            SX(qubits[idx + 2]);
        }
        results += [MResetZ(qubits[idx + 2])];
        // let _ = MResetEachZ([qubits[idx], qubits[idx + 1]]);
        ResetAll([qubits[idx], qubits[idx + 1]]);
    }

    return results;
}
