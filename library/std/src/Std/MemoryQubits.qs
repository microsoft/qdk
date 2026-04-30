// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import QIR.Intrinsic.*;
import QIR.Runtime.*;

// General notes on `MemoryQubit` usage:
// - `MemoryQubit` is a primitive type representing a logical qubit in memory.
// - Quantum gates and measurements cannot be applied directly to `MemoryQubit`.
// - Move state from memory to compute with `Load`, and from compute back to
//   memory with `Store`.
// - Allocate memory qubits with `use`, just like regular qubits:
//     use m = MemoryQubit();
//     use ms = MemoryQubit[4];
// - Memory qubits are automatically released at the end of scope (similarly to
//   qubits). They must be in the zero state before release.
// - Memory and compute qubits are distinct resources. A memory qubit does not
//   change type to a compute qubit, and vice versa.

/// # Summary
/// Loads the state of a memory qubit into a compute qubit.
///
/// # Description
/// This operation transfers the state from `memory_qubit` to `qubit`.
/// After `Load(memory_qubit, qubit)`, `qubit` holds the previous state of
/// `memory_qubit`, and `memory_qubit` is returned to $\ket{0}$.
///
/// # Input
/// ## memory_qubit
/// The memory qubit to read from.
/// ## qubit
/// The compute qubit that receives the loaded state.
operation Load(memory_qubit : MemoryQubit, qubit : Qubit) : Unit {
    __quantum__qis__memory_qubit_load(memory_qubit, qubit);
}

/// # Summary
/// Stores the state of a compute qubit into a memory qubit.
///
/// # Description
/// This operation transfers the state from `qubit` to `memory_qubit`.
/// After `Store(qubit, memory_qubit)`, `memory_qubit` holds the previous state of
/// `qubit`, and `qubit` is returned to $\ket{0}$.
///
/// # Input
/// ## qubit
/// The compute qubit to write from.
/// ## memory_qubit
/// The memory qubit that receives the stored state.
operation Store(qubit : Qubit, memory_qubit : MemoryQubit) : Unit {
    __quantum__qis__memory_qubit_store(qubit, memory_qubit);
}

/// # Summary
/// Loads a memory register into a compute register.
///
/// # Input
/// ## source
/// Source memory register.
/// ## target
/// Target compute register.
///
/// # Remarks
/// Fails if the source and target registers have different lengths.
operation LoadArray(source : MemoryQubit[], target : Qubit[]) : Unit {
    let n = Length(source);
    if (n != Length(target)) { fail ("Registers have different sizes."); }
    for i in 0..n-1 {
        Load(source[i], target[i]);
    }
}

/// # Summary
/// Stores a compute register into a memory register.
///
/// # Input
/// ## source
/// Source compute register.
/// ## target
/// Target memory register.
///
/// # Remarks
/// Fails if the source and target registers have different lengths.
operation StoreArray(source : Qubit[], target : MemoryQubit[]) : Unit {
    let n = Length(source);
    if (n != Length(target)) { fail ("Registers have different sizes."); }
    for i in 0..n-1 {
        Store(source[i], target[i]);
    }
}

/// # Summary
/// Runs a computation on memory qubits by using a temporary compute buffer.
///
/// # Description
/// This operation allocates a temporary `Qubit[]` buffer, loads `mem_qs` into that
/// buffer, applies `op`, and stores the resulting state back into `mem_qs`.
///
/// # Input
/// ## mem_qs
/// Memory register to transform.
/// ## op
/// Operation to apply to the temporary compute buffer.
operation DoComputation(mem_qs : MemoryQubit[], op : Qubit[] => Unit) : Unit {
    use buffer = Qubit[Length(mem_qs)];
    LoadArray(mem_qs, buffer);
    op(buffer);
    StoreArray(buffer, mem_qs);
}

export Load, Store, LoadArray, StoreArray, DoComputation;
