/// # Sample
/// Quantum Teleportation
///
/// # Description
/// Quantum teleportation provides a way of moving a quantum state from one
/// location to another without having to move physical particle(s) along with
/// it. This is done with the help of previously shared quantum entanglement
/// between the sending and the receiving locations, and classical
/// communication.
///
/// This Q# program implements quantum teleportation.
import Std.Diagnostics.*;
import Std.Intrinsic.*;
import Std.Measurement.*;


operation Main() : Result[] {
    // Use the `Teleport` operation to send different quantum states.
    let stateInitializerBasisTuples = [
        ("|0〉", I, PauliZ),
        ("|1〉", X, PauliZ),
        ("|+〉", SetToPlus, PauliX),
        ("|-〉", SetToMinus, PauliX)
    ];

    mutable results = [];
    for (state, initializer, basis) in stateInitializerBasisTuples {
        // Allocate the message and target qubits.
        use (message, target) = (Qubit(), Qubit());

        // Initialize the message and show its state using the `DumpMachine`
        // function.
        initializer(message);
        Message($"Teleporting state {state}");
        DumpRegister([message]);

        // Teleport the message and show the quantum state after
        // teleportation.
        Teleport(message, target);
        Message($"Received state {state}");
        DumpRegister([target]);

        // Measure target in the corresponding basis and reset the qubits to
        // continue teleporting more messages.
        let result = Measure([basis], [target]);
        results += [result];
        ResetAll([message, target]);
    }

    return results;
}

/// # Summary
/// Sends the state of one qubit to a target qubit by using teleportation.
///
/// Notice that after calling Teleport, the state of `message` is collapsed.
///
/// # Input
/// ## message
/// A qubit whose state we wish to send.
/// ## target
/// A qubit initially in the |0〉 state that we want to send
/// the state of message to.
operation Teleport(message : Qubit, target : Qubit) : Unit {
    // Allocate an auxiliary qubit.
    use auxiliary = Qubit();

    // Create some entanglement that we can use to send our message.
    H(auxiliary);
    CNOT(auxiliary, target);

    // Encode the message into the entangled pair.
    CNOT(message, auxiliary);
    H(message);

    // Measure the qubits to extract the classical data we need to decode
    // the message by applying the corrections on the target qubit
    // accordingly.
    if M(auxiliary) == One {
        X(target);
    }

    if M(message) == One {
        Z(target);
    }

    // Reset auxiliary qubit before releasing.
    Reset(auxiliary);
}

/// # Summary
/// Sets a qubit in state |0⟩ to |+⟩.
operation SetToPlus(q : Qubit) : Unit is Adj + Ctl {
    H(q);
}

/// # Summary
/// Sets a qubit in state |0⟩ to |−⟩.
operation SetToMinus(q : Qubit) : Unit is Adj + Ctl {
    X(q);
    H(q);
}
