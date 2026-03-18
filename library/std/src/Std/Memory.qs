// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

operation Allocate() : QMem {
    body intrinsic;
}

operation Free(qmem : QMem) : Unit {
    body intrinsic;
}

operation Store(qubit : Qubit, qmem : QMem) : Unit {
    body intrinsic;
}

operation Load(qmem : QMem, qubit : Qubit) : Unit {
    body intrinsic;
}

export Allocate, Free, Store, Load;
