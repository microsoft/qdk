/// # Sample: Superposition
/// Demonstrates creating a uniform superposition over all basis states.

import Std.Diagnostics.*;

operation Main() : Result {
    use q = Qubit();
    H(q);
    DumpMachine();
    let result = M(q);
    Reset(q);
    Message($"Measured: {result}");
    result
}
