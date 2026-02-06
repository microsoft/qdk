// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

namespace QIR.Runtime {
    operation __quantum__rt__qubit_allocate() : Qubit {
        body intrinsic;
    }

    operation __quantum__rt__qubit_borrow() : Qubit {
        body intrinsic;
    }

    operation __quantum__rt__qubit_release(q : Qubit) : Unit {
        body intrinsic;
    }

    operation AllocateQubitArray(size : Int) : Qubit[] {
        if size < 0 {
            fail "Cannot allocate qubit array with a negative length";
        }
        mutable qs = [];
        for _ in 0..size - 1 {
            set qs += [__quantum__rt__qubit_allocate()];
        }
        qs
    }

    operation BorrowQubitArray(size : Int) : Qubit[] {
        if size < 0 {
            fail "Cannot borrow qubit array with a negative length";
        }
        mutable qs = [];
        for _ in 0..size - 1 {
            set qs += [__quantum__rt__qubit_borrow()];
        }
        qs
    }

    operation ReleaseQubitArray(qs : Qubit[]) : Unit {
        for q in qs {
            __quantum__rt__qubit_release(q);
        }
    }

    function __quantum__rt__read_loss(r : Result) : Bool {
        body intrinsic;
    }

    export __quantum__rt__qubit_allocate, __quantum__rt__qubit_borrow, __quantum__rt__qubit_release, AllocateQubitArray, BorrowQubitArray, ReleaseQubitArray, __quantum__rt__read_loss;
}
