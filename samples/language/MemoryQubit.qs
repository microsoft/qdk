// # Sample
// MemoryQubit
//
// # Description
// Prepares a compute qubit in |1>, stores it in a memory qubit, then loads
// it back and measures. The result should be `One`.

import Std.MemoryQubits.*;

operation Main() : Result {
    use (q, mem) = (Qubit(), MemoryQubit());

    X(q);
    Store(q, mem);
    Load(mem, q);

    return MResetZ(q);
}
