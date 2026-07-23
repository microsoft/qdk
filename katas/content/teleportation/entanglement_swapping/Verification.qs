namespace Kata.Verification {
    import Std.Diagnostics.*;
    import KatasUtils.*;

    operation TeleportEntanglementSwappingTestLoop(
        entanglementSwapping : ((Qubit, Qubit) => Int, (Qubit, Int) => Unit)
    ) : Bool {

        for i in 1..15 {
            use (qAlice1, qAlice2) = (Qubit(), Qubit());
            EntangleWrapper_Reference([qAlice1, qAlice2]);

            use (qBob1, qBob2) = (Qubit(), Qubit());
            EntangleWrapper_Reference([qBob1, qBob2]);

            let (teleportOp, adjustOp) = entanglementSwapping;
            // Apply the operations returned by the solution:
            // first Charlie's side, then Bob's side.
            let result = teleportOp(qAlice1, qBob1);
            adjustOp(qBob2, result);

            // Apply adjoint of the operation that prepares the |Φ⁺⟩ state:
            // if the state of Alice's and Bob's qubits was |Φ⁺⟩,
            // their state should become |00⟩ now.
            Adjoint EntangleWrapper_Reference([qAlice2, qBob2]);

            // Assert that Alice's and Bob's qubits end up in |0⟩ state.
            if not CheckAllZero([qAlice2, qBob2]) {
                Message($"Incorrect.");
                Message($"Entanglement swapping was not successful, as qubits qAlice2 and qBob2 didn't end up in the state |Φ⁺⟩ = 1/sqrt(2) (|00⟩ + |11⟩)");
                EntangleWrapper_Reference([qAlice2, qBob2]);
                Message("The state of the qubits [qAlice1, qAlice2, qBob1, qBob2] after teleportation:");
                DumpMachine();
                ResetAll([qAlice1, qAlice2, qBob1, qBob2]);
                return false;
            }

            ResetAll([qAlice1, qAlice2, qBob1, qBob2]);
        }

        Message("Correct.");
        return true;
    }

    @EntryPoint()
    operation CheckSolution() : Bool {
        return TeleportEntanglementSwappingTestLoop(Kata.EntanglementSwapping());
    }
}
