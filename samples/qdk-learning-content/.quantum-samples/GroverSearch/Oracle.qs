/// Auxiliary oracle for Grover's search.
/// Marks the |11⟩ state by flipping its phase.

operation MarkTarget(register : Qubit[]) : Unit {
    Controlled Z([register[0]], register[1]);
}
