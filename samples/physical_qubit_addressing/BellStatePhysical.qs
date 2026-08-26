// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Physical qubit addressing in Q#.
//
// This program treats QIR qubit IDs as fixed physical addresses on a target machine. The machine
// size is specific to the hardware you target; this example assumes 256 qubits, so size the pool
// to the qubit count of the machine you target. The pattern relies on three facts:
//   1. The source allocates the entire machine-sized qubit pool exactly once, so its array
//      indices map directly onto QIR qubit IDs.
//   2. Selecting pool elements by constant index names the intended physical addresses.
//   3. A provider target must contractually preserve those IDs. Q# indices and QIR IDs alone do
//      not force a hardware layout; confirm the target's contract before relying on it.
//
// See README.md for the full physical-addressing guidance and the constraints below.

// Prepares four Bell states on separate address pairs, then measures them at the end.
operation Main() : (Result, Result)[] {
    // This is the only allocation. Array indices are physical addresses.
    use machine = Qubit[256];

    // Distinct pairs keep every physical qubit measured exactly once.
    let phiPlus = (machine[12], machine[173]);
    let phiMinus = (machine[40], machine[190]);
    let psiPlus = (machine[71], machine[205]);
    let psiMinus = (machine[99], machine[233]);

    // Prepare every state before measuring any qubit.
    PreparePhiPlus(phiPlus);
    PreparePhiMinus(phiMinus);
    PreparePsiPlus(psiPlus);
    PreparePsiMinus(psiMinus);

    // Base Profile requires measurements at the end of the program.
    [
        MeasurePair(phiPlus),
        MeasurePair(phiMinus),
        MeasurePair(psiPlus),
        MeasurePair(psiMinus)
    ]
}

// Measures each physical qubit independently.
operation MeasurePair(alice : Qubit, bob : Qubit) : (Result, Result) {
    (MResetZ(alice), MResetZ(bob))
}

// These helpers operate only on qubits from the fixed pool and allocate no ancillas.

// |Phi+> = (|00> + |11>) / sqrt(2)
operation PreparePhiPlus(alice : Qubit, bob : Qubit) : Unit {
    H(alice);
    CNOT(alice, bob);
}

// |Phi-> = (|00> - |11>) / sqrt(2)
operation PreparePhiMinus(alice : Qubit, bob : Qubit) : Unit {
    H(alice);
    Z(alice);
    CNOT(alice, bob);
}

// |Psi+> = (|01> + |10>) / sqrt(2)
operation PreparePsiPlus(alice : Qubit, bob : Qubit) : Unit {
    H(alice);
    X(bob);
    CNOT(alice, bob);
}

// |Psi-> = (|01> - |10>) / sqrt(2)
operation PreparePsiMinus(alice : Qubit, bob : Qubit) : Unit {
    H(alice);
    Z(alice);
    X(bob);
    CNOT(alice, bob);
}
