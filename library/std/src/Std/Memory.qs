// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

operation Allocate() : QMem {
    body intrinsic;
}

operation Free(qmem : QMem) : Unit {
    body intrinsic;
}

operation Clear(qmem : QMem) : Unit {
    body intrinsic;
}

operation Exchange(qmem : QMem, qubit : Qubit) : Unit {
    body intrinsic;
}

operation Store(qubit : Qubit, qmem : QMem) : Unit {
    Clear(qmem);
    Exchange(qmem, qubit);
}

operation Load(qmem : QMem, qubit : Qubit) : Unit {
    Exchange(qmem, qubit);
    Clear(qmem);
}

export Allocate, Free, Clear, Exchange, Store, Load;
