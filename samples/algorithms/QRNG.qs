/// # Sample
/// Quantum Random Number Generator
///
/// # Description
/// This program implements a quantum random number generator by setting qubits
/// in superposition and then using the measurement results as random bits.
import Std.Convert.*;

operation Main() : Int {
    // This sample generates a random, positive 64-bit integer by using a qubit
    // to produce 63 random bits, and then converting the resulting bit array into an integer.
    mutable bits = [];
    for idxBit in 1..63 {
        bits += [GenerateRandomBit()];
    }
    let sample = ResultArrayAsInt(bits);

    return sample;
}

/// # Summary
/// Generates a random bit.
operation GenerateRandomBit() : Result {
    // Allocate a qubit.
    use q = Qubit();

    // Set the qubit into superposition of 0 and 1 using the Hadamard
    // operation `H`.
    H(q);

    // At this point the qubit `q` has 50% chance of being measured in the
    // |0〉 state and 50% chance of being measured in the |1〉 state.
    // Measure the qubit value using the `M` operation, and store the
    // measurement value in the `result` variable.
    let result = M(q);

    // Reset qubit to the |0〉 state.
    // Qubits must be in the |0〉 state by the time they are released.
    Reset(q);

    // Return the result of the measurement.
    return result;

    // Note that Qubit `q` is automatically released at the end of the block.
}
