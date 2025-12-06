export TransversalCNOT, PreparePlus, PrepareZero;

import Microsoft.Quantum.Arrays.IndexRange;

operation TransversalCNOT(block0 : Qubit[], block1 : Qubit[]) : Unit {
    for i in IndexRange(block0) {
        CNOT(block0[i], block1[i]);
    }
}

operation PrepareZero(q : Qubit) : Unit {
    // Reset(q);
}

operation PreparePlus(q : Qubit) : Unit {
    // Reset(q);
    H(q);
}
