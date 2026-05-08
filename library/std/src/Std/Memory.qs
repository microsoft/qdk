// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


/// Loads a qubit from memory, turning it from "memory"/"cold" to "compute"/"hot".
/// Does nothing if qubit is already "compute".
/// Currently only takes effect for resource estimation with memory-compute architecture
/// enabled in Manual mode.
function MemoryQubitLoad(q : Qubit) : Unit {
    body intrinsic;
}

/// Stores a qubit from memory, turning it from "compute"/"hot" "memory"/"cold".
/// Does nothing if qubit is already "memory".
/// Currently only takes effect for resource estimation with memory-compute architecture
/// enabled in Manual mode.
function MemoryQubitStore(q : Qubit) : Unit {
    body intrinsic;
}

export MemoryQubitLoad, MemoryQubitStore;
