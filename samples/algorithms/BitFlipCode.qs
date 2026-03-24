/// # Sample
/// Bit-Flip Code
///
/// # Description
/// This sample demonstrates the three-qubit bit-flip code. This code is a
/// simple quantum error correction strategy for protecting against a single
/// bit-flip error by encoding a logical qubit into three physical qubits. A
/// single bit-flip error is when one of the three physical qubits has its
/// state changed erroneously in a way that is equivalent to applying the X
/// gate to it.
///
/// The bit-flip correction code works by checking the parity of the physical
/// qubits. By measuring only their parity, the quantum superposition of the
/// qubits is preserved. Because all the physical qubits are supposed to have
/// the same state, when the parity checks detect a difference in state, the
/// erroneous qubit can be identified and corrected.
///
/// This Q# program prepares a logical qubit encoded as three physical qubits
/// with one of the qubits being bit-flipped. It then identifies and corrects
/// the flipped qubit.
import Std.Math.*;
import Std.Diagnostics.*;

operation Main() : Result {
    use logicalQubit = Qubit[3];

    // Set the initial state of the first physical qubit.
    SetSampleState(logicalQubit[0]);

    // Using two additional qubits, encode the first physical qubit into a
    // logical qubit.
    EncodeAsLogicalQubit(logicalQubit[0], logicalQubit[1...]);

    // Induce a bit-flip error on the second qubit.
    X(logicalQubit[1]);

    // Show the logical qubit with the error state.
    DumpMachine();

    // Find and correct the bit-flip error.
    CorrectError(logicalQubit);

    // Show the logical qubit with the corrected state.
    DumpMachine();

    // Decode the logical qubit back into a single physical qubit.
    Adjoint EncodeAsLogicalQubit(logicalQubit[0], logicalQubit[1...]);

    // Measure and reset the physical qubit before releasing it.
    let result = M(logicalQubit[0]);
    Reset(logicalQubit[0]);
    return result;
}

/// # Summary
/// This operation sets the state of the given qubit such that
/// it will have a 20% likelihood of resulting in a `Zero` and
/// 80% likelihood of resulting in a `One` when measured in the
/// computational basis. The input qubit is expected to be in
/// the |0〉 state.
///
/// # Input
/// ## q
/// The given qubit to be put into superposition. It is assumed that this
/// qubit is in its default |0〉 state.
operation SetSampleState(q : Qubit) : Unit {
    let alpha = 0.20;
    Ry(2.0 * ArcCos(Sqrt(alpha)), q);
}

/// # Summary
/// This operation takes the given `physicalQubit` state,
/// (α|0〉 + β|1〉) / √2, and encodes it in the `aux` qubits. This
/// encodes all the qubits into a single logical qubit whose state reflects
/// the state of the given `physicalQubit`: (α|000〉 + β|111〉) / √2.
///
/// # Input
/// ## physicalQubit
/// The qubit whose state, (α|0〉 + β|1〉) / √2, is to be encoded in the
/// logical qubit.
///
/// ## aux
/// The auxiliary qubits that will be used as part of the encoding. These
/// should be grouped with the `physicalQubit` to form the logical qubit.
operation EncodeAsLogicalQubit(physicalQubit : Qubit, aux : Qubit[]) : Unit is Adj {
    ApplyToEachA(CNOT(physicalQubit, _), aux);
}

/// # Summary
/// This operation detects and corrects a single bit-flip error for a logical
/// qubit encoded as three physical qubits. When finished, the given register
/// of qubits will be in the state: (α|000〉 + β|111〉) / √2.
///
/// # Input
/// ## logicalQubit
/// The given register of three physical qubits representing a single logical qubit
/// having superposition (α|0〉 + β|1〉) / √2.
/// This logical qubit can have up to one bit-flip error that will be corrected.
operation CorrectError(logicalQubit : Qubit[]) : Unit {
    Fact(Length(logicalQubit) == 3, "`logicalQubit` must be length 3");

    // Entangle the parity of the physical qubits into two auxiliary qubits.
    use aux = Qubit[2];
    CNOT(logicalQubit[0], aux[0]);
    CNOT(logicalQubit[1], aux[0]);
    CNOT(logicalQubit[1], aux[1]);
    CNOT(logicalQubit[2], aux[1]);

    // Measure the parity information from the auxiliary qubits.
    let (parity01, parity12) = (M(aux[0]), M(aux[1]));
    ResetAll(aux);

    // Determine which of the three qubits has the error based on the
    // parity measurements.
    if parity01 == One {
        if parity12 == One {
            X(logicalQubit[1]);
        } else {
            X(logicalQubit[0]);
        }
    } else {
        if parity12 == One {
            X(logicalQubit[2]);
        } else {
            // No error was detected.
        }
    }
}
