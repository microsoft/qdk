/// # Summary
/// Simple quantum teleportation sample
///
/// # Description
/// This Q# program demonstrates how to teleport quantum state
/// by communicating two classical bits and using previously entangled qubits.
/// This code teleports one specific state, but any state can be teleported.
operation Main() : Bool {
    // Allocate `qAlice`, `qBob` qubits
    use (qAlice, qBob) = (Qubit(), Qubit());

    // Entangle `qAlice`, `qBob` qubits
    H(qAlice);
    CNOT(qAlice, qBob);

    // From now on qubits `qAlice` and `qBob` will not interact directly.

    // Allocate `qToTeleport` qubit and prepare it to be |𝜓⟩≈0.9394|0⟩−0.3429𝑖|1⟩
    use qToTeleport = Qubit();
    Rx(0.7, qToTeleport);

    // Prepare the message by entangling `qToTeleport` and `qAlice` qubits
    CNOT(qToTeleport, qAlice);
    H(qToTeleport);

    // Obtain classical measurement results b1 and b2 at Alice's site.
    let b1 = M(qToTeleport) == One;
    let b2 = M(qAlice) == One;

    // At this point classical bits b1 and b2 are "sent" to the Bob's site.

    // Decode the message by applying adjustments based on classical data b1 and b2.
    if b1 {
        Z(qBob);
    }
    if b2 {
        X(qBob);
    }

    // Make sure that the obtained message is |𝜓⟩≈0.9394|0⟩−0.3429𝑖|1⟩
    Rx(-0.7, qBob);
    // This state dump should show that the state of `qBob` is |0⟩ state, which means that the teleportation was successful.
    Std.Diagnostics.DumpMachine();
    // We can further verify the teleport by measuring `qBob`, which should give us |0⟩ state with certainty.
    // Note that verifying via measurement might require multiple runs or "shots" to investigate the distribution of outcomes.
    let correct = M(qBob) == Zero;
    Message($"Teleportation successful: {correct}.");

    // Reset all qubits to |0⟩ state.
    ResetAll([qAlice, qBob, qToTeleport]);

    // Return indication if the measurement of the state was correct
    correct
}
