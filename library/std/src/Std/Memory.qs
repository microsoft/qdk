// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


/// Loads a qubit from memory, turning it from "memory" to "compute".
/// Does nothing if qubit is already "compute".
/// Currently only takes effect for resource estimation with memory-compute architecture
/// enabled in Manual mode.
operation Load(q : Qubit) : Unit {
    body intrinsic;
}

/// Stores a qubit into memory, turning it from "compute" to "memory".
/// Does nothing if qubit is already "memory".
/// Currently only takes effect for resource estimation with memory-compute architecture
/// enabled in Manual mode.
operation Store(q : Qubit) : Unit {
    body intrinsic;
}

export Load, Store;
