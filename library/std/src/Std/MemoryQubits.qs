// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import QIR.Intrinsic.*;
import QIR.Runtime.*;

operation Load(memory_qubit : MemoryQubit, qubit : Qubit) : Unit {
    __quantum__qis__memory_qubit_load(memory_qubit, qubit);
}

operation Store(qubit : Qubit, memory_qubit : MemoryQubit) : Unit {
    __quantum__qis__memory_qubit_store(qubit, memory_qubit);
}

operation LoadArray(source : MemoryQubit[], target : Qubit[]) : Unit {
    let n = Length(source);
    if (n != Length(target)) { fail ("Registers have different sizes."); }
    for i in 0..n-1 {
        Load(source[i], target[i]);
    }
}

operation StoreArray(source : Qubit[], target : MemoryQubit[]) : Unit {
    let n = Length(source);
    if (n != Length(target)) { fail ("Registers have different sizes."); }
    for i in 0..n-1 {
        Store(source[i], target[i]);
    }
}

/// Performs computation on a MemoryQubit register by loading it to a temporary Qubit
/// register, performing the operation `op` and storing result back to `mem_qs`.
operation DoComputation(mem_qs : MemoryQubit[], op : Qubit[] => Unit) : Unit {
    use buffer = Qubit[Length(mem_qs)];
    LoadArray(mem_qs, buffer);
    op(buffer);
    StoreArray(buffer, mem_qs);
}

export Load, Store, LoadArray, StoreArray, DoComputation;
