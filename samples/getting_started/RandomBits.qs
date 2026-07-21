// # Summary
// Simple Quantum Random Number Generator sample
//
// # Description
// This program implements a quantum random number generator by setting qubits
// into superposition and then using the measurement results as random bits.
// This is equivalent to generating a random number in the range of 0..2ᴺ-1.
operation Main() : Result[] {
    // Generate a 5-bit random number.
    GenerateNRandomBits(5)
}

/// # Summary
/// Generates N random bits in the form of `Zero` or `One` results.
operation GenerateNRandomBits(nBits : Int) : Result[] {
    // Array for the results
    mutable results = [];
    for _ in 1..nBits {
        // Append next random result to the array
        results += [GenerateRandomBit()];
    }
    results
}

/// # Summary
/// Generates a random bit in the form of `Zero` or `One` result.
operation GenerateRandomBit() : Result {
    // Allocate a qubit
    use q = Qubit();
    // Set the qubit into uniform superposition of |0〉 and |1〉
    H(q);
    // Now the qubit has 50% chance of being measured as `One`
    // and 50% chance of being measured as `Zero`.
    // Measure and reset the qubit. Return the result.
    MResetZ(q)
}
