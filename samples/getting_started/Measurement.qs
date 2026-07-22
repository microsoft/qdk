// # Summary
// Measurement sample
//
// # Description
// This Q# program demonstrates how to perform measurements in Z basis.
//
// # Remarks
// Quantum measurement is an irreversible operation in which a quantum system
// is manipulated to yield a numerical result. Measuring a quantum system
// generally changes the quantum state that describes that system.
operation Main() : (Result, Result[]) {
    // Allocate a qubit. Qubit is in |0〉 state after allocation.
    use q = Qubit();
    // Flip the state. Qubit is in |1〉 state now.
    X(q);

    // The `MResetZ` operation performs a measurement of a single qubit in the
    // computational basis, also known as the Pauli Z basis. Then it resets
    // the qubit to |0〉 state. `MResetZ` may be more efficient than measuring
    // a qubit and resetting it using two separate operations.
    let result = MResetZ(q);

    // Allocate a two-qubit array (or register).
    use qs = Qubit[2];

    // The `MResetEachZ` operation measures each qubit in an array in the
    // computational basis and resets each qubit to |0〉 state. It returns
    // an array of `Result` values.
    let results = MResetEachZ(qs);

    // Return all results. In Q#, the result of a measurement is a value
    // of the type `Result`, that is, `One` or `Zero`.
    (result, results)
}
