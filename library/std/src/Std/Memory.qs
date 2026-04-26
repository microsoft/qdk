// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import QIR.Intrinsic.*;
import QIR.Runtime.*;

operation Allocate() : QMem {
    return __quantum__rt__memory_qubit_allocate();
}

operation Free(qmem : QMem) : Unit {
    __quantum__rt__memory_qubit_release(qmem);
}

operation Load(qmem : QMem, qubit : Qubit) : Unit {
    __quantum__qis__memory_qubit_load(qmem, qubit);
}

operation Store(qubit : Qubit, qmem : QMem) : Unit {
    __quantum__qis__memory_qubit_store(qubit, qmem);
}

export Allocate, Free, Store, Load;
