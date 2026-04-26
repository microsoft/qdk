// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.



// Controlled Gates

operation __quantum__qis__ccx__body(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__cx__body(control : Qubit, target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__cy__body(control : Qubit, target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__cz__body(control : Qubit, target : Qubit) : Unit {
    body intrinsic;
}

// Rotation Gates

operation __quantum__qis__rx__body(angle : Double, target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__rxx__body(angle : Double, target1 : Qubit, target2 : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__ry__body(angle : Double, target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__ryy__body(angle : Double, target1 : Qubit, target2 : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__rz__body(angle : Double, target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__rzz__body(angle : Double, target1 : Qubit, target2 : Qubit) : Unit {
    body intrinsic;
}

// Single-Qubit Gates

operation __quantum__qis__h__body(target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__s__body(target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__s__adj(target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__sx__body(target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__t__body(target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__t__adj(target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__x__body(target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__y__body(target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__z__body(target : Qubit) : Unit {
    body intrinsic;
}

// Two-Qubit Gates

operation __quantum__qis__swap__body(target1 : Qubit, target2 : Qubit) : Unit {
    body intrinsic;
}

// Quantum Measurement

operation __quantum__qis__m__body(target : Qubit) : Result {
    body intrinsic;
}

operation __quantum__qis__reset__body(target : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__mresetz__body(target : Qubit) : Result {
    body intrinsic;
}

// MemoryQubit operations.
operation __quantum__qis__memory_qubit_load(qmem : QMem, qubit : Qubit) : Unit {
    body intrinsic;
}

operation __quantum__qis__memory_qubit_store(qubit : Qubit, qmem : QMem) : Unit {
    body intrinsic;
}

export
    __quantum__qis__ccx__body,
    __quantum__qis__cx__body,
    __quantum__qis__cy__body,
    __quantum__qis__cz__body,
    __quantum__qis__rx__body,
    __quantum__qis__rxx__body,
    __quantum__qis__ry__body,
    __quantum__qis__ryy__body,
    __quantum__qis__rz__body,
    __quantum__qis__rzz__body,
    __quantum__qis__h__body,
    __quantum__qis__s__body,
    __quantum__qis__s__adj,
    __quantum__qis__sx__body,
    __quantum__qis__t__body,
    __quantum__qis__t__adj,
    __quantum__qis__x__body,
    __quantum__qis__y__body,
    __quantum__qis__z__body,
    __quantum__qis__swap__body,
    __quantum__qis__m__body,
    __quantum__qis__reset__body,
    __quantum__qis__mresetz__body,
    __quantum__qis__memory_qubit_load,
    __quantum__qis__memory_qubit_store;
