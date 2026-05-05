/// # Sample: Bell State
/// Creates a Bell state (maximally entangled pair) and measures both qubits,
/// demonstrating that their outcomes are always correlated.

import Std.Diagnostics.*;

operation Main() : (Result, Result) {
    use (q1, q2) = (Qubit(), Qubit());

    // Create Bell state |Φ+⟩ = (|00⟩ + |11⟩) / √2
    H(q1);
    CNOT(q1, q2);
    DumpMachine();

    let (r1, r2) = (M(q1), M(q2));
    ResetAll([q1, q2]);
    Message($"Results: ({r1}, {r2})");
    (r1, r2)
}
