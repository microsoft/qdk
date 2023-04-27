// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


namespace Microsoft.Quantum.Samples.Teleportation {
    open Microsoft.Quantum.Canon;
    open Microsoft.Quantum.Intrinsic;
    open Microsoft.Quantum.Random; 

    //////////////////////////////////////////////////////////////////////////
    // Introduction //////////////////////////////////////////////////////////
    //////////////////////////////////////////////////////////////////////////

    // Quantum teleportation provides a way of moving a quantum state from one
    // location to another without having to move physical particle(s) along
    // with it. This is done with the help of previously shared quantum
    // entanglement between the sending and the receiving locations and
    // classical communication.

    //////////////////////////////////////////////////////////////////////////
    // Teleportation /////////////////////////////////////////////////////////
    //////////////////////////////////////////////////////////////////////////

    /// # Summary
    /// Sends the state of one qubit to a target qubit by using
    /// teleportation.
    ///
    /// Notice that after calling Teleport, the state of `msg` is
    /// collapsed.
    ///
    /// # Input
    /// ## msg
    /// A qubit whose state we wish to send.
    /// ## target
    /// A qubit initially in the |0〉 state that we want to send
    /// the state of msg to.
    operation Teleport (msg : Qubit, target : Qubit) : Unit {
        use register = Qubit();
        // Create some entanglement that we can use to send our message.
        H(register);
        CNOT(register, target);

        // Encode the message into the entangled pair.
        CNOT(msg, register);
        H(msg);

        // Measure the qubits to extract the classical data we need to
        // decode the message by applying the corrections on
        // the target qubit accordingly.
        if M(msg) == One { Z(target); }
        // Correction step
        if M(register) == One {
            X(target);
            // Reset register to Zero state before releasing
            X(register);
        }
    }

    // One can use quantum teleportation circuit to send an unobserved
    // (unknown) classical message from source qubit to target qubit
    // by sending specific (known) classical information from source
    // to target.

    /// # Summary
    /// Uses teleportation to send a classical message from one qubit
    /// to another.
    ///
    /// # Input
    /// ## message
    /// If `true`, the source qubit (`here`) is prepared in the
    /// |1〉 state, otherwise the source qubit is prepared in |0〉.
    ///
    /// ## Output
    /// The result of a Z-basis measurement on the teleported qubit,
    /// represented as a Bool.
    operation TeleportClassicalMessage (message : Bool) : Bool {
        // Ask for some qubits that we can use to teleport.
        use (msg, target) = (Qubit(), Qubit());

        // Encode the message we want to send.
        if message {
            X(msg);
        }

        // Use the operation we defined above.
        Teleport(msg, target);

        // Check what message was received.
        let result = (M(target) == One);
        
        // Reset qubits to Zero state before releasing
        Reset(msg);
        Reset(target);

        return result;
    }

    /// # Summary
    /// Sets the qubit's state to |+⟩.
    operation SetToPlus(q: Qubit) : Unit {
        Reset(q);
        H(q);
    }

    /// # Summary
    /// Sets the qubit's state to |−⟩.
    operation SetToMinus(q: Qubit) : Unit {
        Reset(q);
        X(q);
        H(q);
    }

    /// # Summary
    /// Returns true if qubit is |+⟩ (assumes qubit is either |+⟩ or |−⟩)
    operation MeasureIsPlus(q: Qubit) : Bool {
        Measure([PauliX], [q]) == Zero
    }

    /// # Summary
    /// Returns true if qubit is |−⟩ (assumes qubit is either |+> or |−⟩)
    operation MeasureIsMinus(q: Qubit) : Bool {
        Measure([PauliX], [q]) == One
    }

    /// # Summary
    /// Randomly prepares the qubit into |+⟩ or |−⟩
    operation PrepareRandomMessage(q: Qubit) : Unit {        
        let choice = DrawRandomInt(0, 1) == 1;

        if choice {
            Message("Sending |->");
            SetToMinus(q);
        } else {
            Message("Sending |+>");
            SetToPlus(q);
        }
    }

    // One can also use quantum teleportation to send any quantum state
    // without losing any information. The following sample shows
    // how a randomly picked non-trivial state (|-> or |+>)
    // gets moved from one qubit to another.

    /// # Summary
    /// Uses teleportation to send a randomly picked |-> or |+> state
    /// to another.
    operation TeleportRandomMessage () : Unit {
        // Ask for some qubits that we can use to teleport.
        use (msg, target) = (Qubit(), Qubit());
        PrepareRandomMessage(msg);

        // Use the operation we defined above.
        Teleport(msg, target);

        // Report message received:
        if MeasureIsPlus(target) { Message("Received |+>"); }
        if MeasureIsMinus(target) { Message("Received |->"); }

        // Reset all of the qubits that we used before releasing
        // them.
        Reset(msg);
        Reset(target);
    }

    @EntryPoint()
    operation Main () : Unit {
        for idxRun in 1 .. 10 {
            let sent = DrawRandomInt(0, 1) == 1;
            let received = TeleportClassicalMessage(sent);
            Message(
                "Round " + AsString(idxRun) +
                ": Sent " + AsString(sent) +
                ", got " + AsString(received) + ".");
            Message(sent == received ? "Teleportation successful!" | "");
        }
        for idxRun in 1 .. 10 {
            TeleportRandomMessage();
        }

    }
}

// ////////////////////////////////////////////////////////////////////////
// Other teleportation scenarios not illustrated here
// ////////////////////////////////////////////////////////////////////////

// ● Teleport a rotation. Rotate a basis state by a certain angle φ ∈ [0, 2π),
// for example by preparing Rₓ(φ) |0〉, and teleport the rotated state to the target qubit.
// When successful, the target qubit captures the angle φ [although, on course one does
// not have classical access to its value].
// ● "Super dense coding".  Given an EPR state |β〉 shared between the source and target
// qubits, the source can encode two classical bits a,b by applying Z^b X^a to its half
// of |β〉. Both bits can be recovered on the target by measurement in the Bell basis.
// For details refer to discussion and code in Unit Testing Sample, in file SuperdenseCoding.qs.
// ////////////////////////////////////////////////////////////////////////