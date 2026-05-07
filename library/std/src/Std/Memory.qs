// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.


/// Loads a qubit from "memory/cold" qubit to "compute/hot" qubit.
/// Does nothing if qubit is already "hot".
function MemoryQubitLoad(q : Qubit) : Unit {
    body intrinsic;
}

/// Stores a qubit from "compute/hot" qubit to "memory/cold" qubit.
/// Does nothing if qubit is already "cold".
function MemoryQubitStore(q : Qubit) : Unit {
    body intrinsic;
}

export MemoryQubitLoad, MemoryQubitStore;
