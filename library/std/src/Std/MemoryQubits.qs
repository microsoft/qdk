// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import QIR.Intrinsic.*;
import QIR.Runtime.*;

operation Allocate() : MemoryQubit {
    return __quantum__rt__memory_qubit_allocate();
}

operation Free(memory_qubit : MemoryQubit) : Unit {
    __quantum__rt__memory_qubit_release(memory_qubit);
}

operation Load(memory_qubit : MemoryQubit, qubit : Qubit) : Unit {
    __quantum__qis__memory_qubit_load(memory_qubit, qubit);
}

operation Store(qubit : Qubit, memory_qubit : MemoryQubit) : Unit {
    __quantum__qis__memory_qubit_store(qubit, memory_qubit);
}

export Allocate, Free, Store, Load;
