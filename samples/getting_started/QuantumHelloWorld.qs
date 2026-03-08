/// # Summary
/// Quantum Hello World sample
///
/// # Description
/// This is one of the simplest Q# programs that contains quantum part.
/// This code prints the message then allocates a qubit and immediately measures it.
/// Since the qubit starts in |0〉 state such measurement will always yield `Zero`.
@EntryPoint()
operation Main() : Result {
    // Print the message (when running on a simulator).
    Message("Hello world!");

    // Allocate a qubit. Qubit is in |0〉 state after allocation.
    use qubit = Qubit();

    // Measure then reset the qubit. Last statement returns result from `Main`.
    // Measurement result is `Zero` as the qubit is in |0〉 state.
    return MResetZ(qubit)

    // Note, that `qubit` is automatically deallocated at the end of the block.
}
