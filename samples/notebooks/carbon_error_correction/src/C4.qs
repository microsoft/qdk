// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

export PrepareXX, PrepareZZ, DetectOn, DetectZ;

import Utils.PrepareZero, Utils.PreparePlus, Utils.TransversalCNOT;

operation PrepareXX(block : Qubit[], plus_ancilla : Qubit) : Unit {
    PrepareZero(block[3]);
    PreparePlus(block[0]);
    CNOT(plus_ancilla, block[3]);
    CNOT(block[0], block[3]);
    PreparePlus(block[1]);
    CNOT(block[1], block[3]);
    PreparePlus(block[2]);
    CNOT(block[2], block[3]);
    CNOT(plus_ancilla, block[3]);
}

operation PrepareZZ(block : Qubit[], zero_ancilla : Qubit) : Unit {
    PreparePlus(block[3]);
    PrepareZero(block[0]);
    CNOT(block[3], zero_ancilla);
    CNOT(block[3], block[0]);
    PrepareZero(block[1]);
    CNOT(block[3], block[1]);
    PrepareZero(block[2]);
    CNOT(block[3], block[2]);
    CNOT(block[3], zero_ancilla);
}

operation DetectOn(block : Qubit[], plus_ancilla : Qubit, zero_ancilla : Qubit) : Result[] {
    CNOT(plus_ancilla, block[0]);
    CNOT(block[1], zero_ancilla);
    CNOT(plus_ancilla, block[1]);
    CNOT(block[0], zero_ancilla);
    CNOT(plus_ancilla, block[2]);
    CNOT(block[3], zero_ancilla);
    CNOT(plus_ancilla, block[3]);
    CNOT(block[2], zero_ancilla);
    [MResetX(plus_ancilla), MResetZ(zero_ancilla)]
}

operation DetectZ(block : Qubit[], plus_ancilla : Qubit, zero_ancilla : Qubit) : Result[] {
    CNOT(plus_ancilla, zero_ancilla);
    CNOT(block[0], zero_ancilla);
    CNOT(block[1], plus_ancilla);
    CNOT(block[2], zero_ancilla);
    CNOT(block[3], plus_ancilla);
    CNOT(plus_ancilla, zero_ancilla);
    [MResetX(plus_ancilla), MResetZ(zero_ancilla)]
}
