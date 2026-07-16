// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// # Summary
/// Loads a qubit from memory.
///
/// # Description
/// Loads a qubit from memory, turning it from "memory" to "compute".
/// The qubit must be in "memory" before calling this operation.
/// Currently only takes effect for resource estimation with memory-compute architecture
/// enabled in Manual mode.
operation Load(q : Qubit) : Unit {
    body intrinsic;
}

/// # Summary
/// Stores a qubit into memory.
///
/// # Description
/// Stores a qubit into memory, turning it from "compute" to "memory".
/// The qubit must be in "compute" before calling this operation.
/// Currently only takes effect for resource estimation with memory-compute architecture
/// enabled in Manual mode.
operation Store(q : Qubit) : Unit {
    body intrinsic;
}

export Load, Store;
