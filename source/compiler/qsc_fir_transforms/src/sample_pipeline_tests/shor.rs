// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::PipelineStage;
use crate::pretty::write_reachable_qsharp_parseable;
use crate::test_utils::compile_and_run_pipeline_to;

const SHOR_SOURCE: &str = include_str!("../../../../../samples/algorithms/Shor.qs");

#[test]
#[allow(clippy::too_many_lines)]
fn shor_sample_full_pipeline_reachable_items() {
    // `DrawRandomInt` is a simulation-only intrinsic with no QIR lowering, so
    // the test pins a deterministic generator. The rest of Shor's algorithm
    // (period finding, modular arithmetic, continued fractions) is unchanged,
    // which keeps a large cross-package reachable graph for the transforms.
    let source = SHOR_SOURCE.replace(
        "let generator = DrawRandomInt(1, number - 1);",
        "let generator = 2;//DrawRandomInt(1, number - 1);",
    );
    let (store, pkg_id) = compile_and_run_pipeline_to(&source, PipelineStage::Full);
    let rendered = write_reachable_qsharp_parseable(&store, pkg_id);
    expect![[r#"
        // package 0
        operation __quantum__rt__qubit_allocate() : Qubit {
            body intrinsic;
        }
        operation __quantum__rt__qubit_release(q : Qubit) : Unit {
            body intrinsic;
        }
        operation AllocateQubitArray(size : Int) : Qubit[] {
            if size < 0 {
                fail $"Cannot allocate qubit array with a negative length";
            }

            mutable qs : Qubit[] = [];
            {
                let _range_id_0 : Range = 0..size - 1;
                mutable _index_id_1 : Int = _range_id_0.Start;
                let _step_id_2 : Int = _range_id_0.Step;
                let _end_id_3 : Int = _range_id_0.End;
                while ((_step_id_2 > 0) and (_index_id_1 <= _end_id_3)) or ((_step_id_2 < 0) and (_index_id_1 >= _end_id_3)) {
                    let _ : Int = _index_id_1;
                    qs += [__quantum__rt__qubit_allocate()];
                    _index_id_1 += _step_id_2;
                }

            }

            qs
        }
        operation ReleaseQubitArray(qs : Qubit[]) : Unit {
            {
                let _array_id_4 : Qubit[] = qs;
                let _len_id_5 : Int = Length(_array_id_4);
                mutable _index_id_6 : Int = 0;
                while _index_id_6 < _len_id_5 {
                    let q : Qubit = _array_id_4[_index_id_6];
                    __quantum__rt__qubit_release(q);
                    _index_id_6 += 1;
                }

            }

        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        // package 1
        operation MapPauliAxis(from : Pauli, to : Pauli, q : Qubit) : Unit is Adj + Ctl {
            body ... {
                if from == to {} else if ((from == PauliZ) and (to == PauliX)) or ((from == PauliX) and (to == PauliZ)) {
                    H(q);
                } else if (from == PauliZ) and (to == PauliY) {
                    Adjoint S(q);
                    H(q);
                } else if (from == PauliY) and (to == PauliZ) {
                    H(q);
                    S(q);
                } else if (from == PauliY) and (to == PauliX) {
                    S(q);
                } else if (from == PauliX) and (to == PauliY) {
                    Adjoint S(q);
                } else {
                    fail $"Unsupported mapping of Pauli axes.";
                }

            }
            adjoint ... {
                if from == to {} else if ((from == PauliZ) and (to == PauliX)) or ((from == PauliX) and (to == PauliZ)) {
                    Adjoint H(q);
                } else if (from == PauliZ) and (to == PauliY) {
                    Adjoint H(q);
                    Adjoint Adjoint S(q);
                } else if (from == PauliY) and (to == PauliZ) {
                    Adjoint S(q);
                    Adjoint H(q);
                } else if (from == PauliY) and (to == PauliX) {
                    Adjoint S(q);
                } else if (from == PauliX) and (to == PauliY) {
                    Adjoint Adjoint S(q);
                } else {
                    fail $"Unsupported mapping of Pauli axes.";
                }

            }
            controlled (ctls, ...) {
                if from == to {} else if ((from == PauliZ) and (to == PauliX)) or ((from == PauliX) and (to == PauliZ)) {
                    Controlled H(ctls, q);
                } else if (from == PauliZ) and (to == PauliY) {
                    Controlled Adjoint S(ctls, q);
                    Controlled H(ctls, q);
                } else if (from == PauliY) and (to == PauliZ) {
                    Controlled H(ctls, q);
                    Controlled S(ctls, q);
                } else if (from == PauliY) and (to == PauliX) {
                    Controlled S(ctls, q);
                } else if (from == PauliX) and (to == PauliY) {
                    Controlled Adjoint S(ctls, q);
                } else {
                    fail $"Unsupported mapping of Pauli axes.";
                }

            }
            controlled adjoint (ctls, ...) {
                if from == to {} else if ((from == PauliZ) and (to == PauliX)) or ((from == PauliX) and (to == PauliZ)) {
                    Controlled Adjoint H(ctls, q);
                } else if (from == PauliZ) and (to == PauliY) {
                    Controlled Adjoint H(ctls, q);
                    Controlled Adjoint Adjoint S(ctls, q);
                } else if (from == PauliY) and (to == PauliZ) {
                    Controlled Adjoint S(ctls, q);
                    Controlled Adjoint H(ctls, q);
                } else if (from == PauliY) and (to == PauliX) {
                    Controlled Adjoint S(ctls, q);
                } else if (from == PauliX) and (to == PauliY) {
                    Controlled Adjoint Adjoint S(ctls, q);
                } else {
                    fail $"Unsupported mapping of Pauli axes.";
                }

            }
        }
        operation ApplyXorInPlace(value : Int, target : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_7 : Qubit[] = target;
                    let _len_id_8 : Int = Length(_array_id_7);
                    mutable _index_id_9 : Int = 0;
                    while _index_id_9 < _len_id_8 {
                        let q : Qubit = _array_id_7[_index_id_9];
                        if (runningValue &&& 1) != 0 {
                            X(q);
                        }

                        runningValue >>>= 1;
                        _index_id_9 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            adjoint ... {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_10 : Qubit[] = target;
                    let _len_id_11 : Int = Length(_array_id_10);
                    mutable _index_id_12 : Int = 0;
                    while _index_id_12 < _len_id_11 {
                        let q : Qubit = _array_id_10[_index_id_12];
                        if (runningValue &&& 1) != 0 {
                            X(q);
                        }

                        runningValue >>>= 1;
                        _index_id_12 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            controlled (ctls, ...) {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_13 : Qubit[] = target;
                    let _len_id_14 : Int = Length(_array_id_13);
                    mutable _index_id_15 : Int = 0;
                    while _index_id_15 < _len_id_14 {
                        let q : Qubit = _array_id_13[_index_id_15];
                        if (runningValue &&& 1) != 0 {
                            Controlled X(ctls, q);
                        }

                        runningValue >>>= 1;
                        _index_id_15 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            controlled adjoint (ctls, ...) {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_16 : Qubit[] = target;
                    let _len_id_17 : Int = Length(_array_id_16);
                    mutable _index_id_18 : Int = 0;
                    while _index_id_18 < _len_id_17 {
                        let q : Qubit = _array_id_16[_index_id_18];
                        if (runningValue &&& 1) != 0 {
                            Controlled X(ctls, q);
                        }

                        runningValue >>>= 1;
                        _index_id_18 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
        }
        function IntAsDouble(number : Int) : Double {
            body intrinsic;
        }
        function IntAsBigInt(number : Int) : BigInt {
            body intrinsic;
        }
        function Fact(actual : Bool, message : String) : Unit {
            body intrinsic;
        }
        operation CH(control : Qubit, target : Qubit) : Unit is Adj {
            body ... {
                {
                    {
                        S(target);
                        H(target);
                        T(target);
                    }

                    let _apply_res : Unit = {
                        CNOT(control, target);
                    };
                    {
                        Adjoint T(target);
                        Adjoint H(target);
                        Adjoint S(target);
                    }

                    _apply_res
                }

            }
            adjoint ... {
                {
                    {
                        S(target);
                        H(target);
                        T(target);
                    }

                    let _apply_res : Unit = {
                        Adjoint CNOT(control, target);
                    };
                    {
                        Adjoint T(target);
                        Adjoint H(target);
                        Adjoint S(target);
                    }

                    _apply_res
                }

            }
        }
        operation CCH(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
            body ... {
                {
                    {
                        S(target);
                        H(target);
                        T(target);
                    }

                    let _apply_res : Unit = {
                        CCNOT(control1, control2, target);
                    };
                    {
                        Adjoint T(target);
                        Adjoint H(target);
                        Adjoint S(target);
                    }

                    _apply_res
                }

            }
            adjoint ... {
                {
                    {
                        S(target);
                        H(target);
                        T(target);
                    }

                    let _apply_res : Unit = {
                        Adjoint CCNOT(control1, control2, target);
                    };
                    {
                        Adjoint T(target);
                        Adjoint H(target);
                        Adjoint S(target);
                    }

                    _apply_res
                }

            }
        }
        operation ApplyGlobalPhase(theta : Double) : Unit is Adj + Ctl {
            body ... {
                ControllableGlobalPhase(theta);
            }
            adjoint ... {
                ControllableGlobalPhase((-theta));
            }
            controlled (ctls, ...) {
                Controlled ControllableGlobalPhase(ctls, theta);
            }
            controlled adjoint (ctls, ...) {
                Controlled ControllableGlobalPhase(ctls, (-theta));
            }
        }
        operation ControllableGlobalPhase(theta : Double) : Unit is Ctl {
            body ... {
                GlobalPhase([], theta);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                if __cond_0 {
                    GlobalPhase([], theta);
                } else {
                    Controlled Rz(ctls[1...], (theta, ctls[0]));
                    GlobalPhase(ctls[1...], theta / 2.);
                }

            }
        }
        operation GlobalPhase(ctls : Qubit[], theta : Double) : Unit {
            body intrinsic;
        }
        operation CRz(control : Qubit, theta : Double, target : Qubit) : Unit is Adj {
            body ... {
                Rz(theta / 2., target);
                CNOT(control, target);
                Rz(((-theta)) / 2., target);
                CNOT(control, target);
            }
            adjoint ... {
                Adjoint CNOT(control, target);
                Adjoint Rz(((-theta)) / 2., target);
                Adjoint CNOT(control, target);
                Adjoint Rz(theta / 2., target);
            }
        }
        operation CS(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
            body ... {
                T(control);
                T(target);
                CNOT(control, target);
                Adjoint T(target);
                CNOT(control, target);
            }
            adjoint ... {
                Adjoint CNOT(control, target);
                Adjoint Adjoint T(target);
                Adjoint CNOT(control, target);
                Adjoint T(target);
                Adjoint T(control);
            }
            controlled (ctls, ...) {
                Controlled T(ctls, control);
                Controlled T(ctls, target);
                Controlled CNOT(ctls, (control, target));
                Controlled Adjoint T(ctls, target);
                Controlled CNOT(ctls, (control, target));
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint CNOT(ctls, (control, target));
                Controlled Adjoint Adjoint T(ctls, target);
                Controlled Adjoint CNOT(ctls, (control, target));
                Controlled Adjoint T(ctls, target);
                Controlled Adjoint T(ctls, control);
            }
        }
        operation CT(control : Qubit, target : Qubit) : Unit is Adj {
            body ... {
                let angle : Double = PI() / 8.;
                Rz(angle, control);
                Rz(angle, target);
                CNOT(control, target);
                Adjoint Rz(angle, target);
                CNOT(control, target);
                ApplyGlobalPhase(angle / 2.);
            }
            adjoint ... {
                let angle : Double = PI() / 8.;
                Adjoint ApplyGlobalPhase(angle / 2.);
                Adjoint CNOT(control, target);
                Adjoint Adjoint Rz(angle, target);
                Adjoint CNOT(control, target);
                Adjoint Rz(angle, target);
                Adjoint Rz(angle, control);
            }
        }
        operation CollectControls(ctls : Qubit[], aux : Qubit[], adjustment : Int) : Unit is Adj {
            body ... {
                {
                    let _range_id_19 : Range = 0..2..Length(ctls) - 2;
                    mutable _index_id_20 : Int = _range_id_19.Start;
                    let _step_id_21 : Int = _range_id_19.Step;
                    let _end_id_22 : Int = _range_id_19.End;
                    while ((_step_id_21 > 0) and (_index_id_20 <= _end_id_22)) or ((_step_id_21 < 0) and (_index_id_20 >= _end_id_22)) {
                        let i : Int = _index_id_20;
                        CCNOT(ctls[i], ctls[i + 1], aux[i / 2]);
                        _index_id_20 += _step_id_21;
                    }

                }

                {
                    let _range_id_23 : Range = 0..((Length(ctls) / 2) - 2) - adjustment;
                    mutable _index_id_24 : Int = _range_id_23.Start;
                    let _step_id_25 : Int = _range_id_23.Step;
                    let _end_id_26 : Int = _range_id_23.End;
                    while ((_step_id_25 > 0) and (_index_id_24 <= _end_id_26)) or ((_step_id_25 < 0) and (_index_id_24 >= _end_id_26)) {
                        let i_1 : Int = _index_id_24;
                        CCNOT(aux[i_1 * 2], aux[(i_1 * 2) + 1], aux[i_1 + (Length(ctls) / 2)]);
                        _index_id_24 += _step_id_25;
                    }

                }

            }
            adjoint ... {
                {
                    let _range : Range = 0..((Length(ctls) / 2) - 2) - adjustment;
                    {
                        let _range_id_27 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                        mutable _index_id_28 : Int = _range_id_27.Start;
                        let _step_id_29 : Int = _range_id_27.Step;
                        let _end_id_30 : Int = _range_id_27.End;
                        while ((_step_id_29 > 0) and (_index_id_28 <= _end_id_30)) or ((_step_id_29 < 0) and (_index_id_28 >= _end_id_30)) {
                            let i : Int = _index_id_28;
                            Adjoint CCNOT(aux[i * 2], aux[(i * 2) + 1], aux[i + (Length(ctls) / 2)]);
                            _index_id_28 += _step_id_29;
                        }

                    }

                }

                {
                    let _range_1 : Range = 0..2..Length(ctls) - 2;
                    {
                        let _range_id_31 : Range = _range_1.Start + (((_range_1.End - _range_1.Start) / _range_1.Step) * _range_1.Step)..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_32 : Int = _range_id_31.Start;
                        let _step_id_33 : Int = _range_id_31.Step;
                        let _end_id_34 : Int = _range_id_31.End;
                        while ((_step_id_33 > 0) and (_index_id_32 <= _end_id_34)) or ((_step_id_33 < 0) and (_index_id_32 >= _end_id_34)) {
                            let i_1 : Int = _index_id_32;
                            Adjoint CCNOT(ctls[i_1], ctls[i_1 + 1], aux[i_1 / 2]);
                            _index_id_32 += _step_id_33;
                        }

                    }

                }

            }
        }
        operation AdjustForSingleControl(ctls : Qubit[], aux : Qubit[]) : Unit is Adj {
            body ... {
                let __cond_0 : Bool = (Length(ctls) % 2) != 0;
                if __cond_0 {
                    CCNOT(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], aux[Length(ctls) - 2]);
                }

            }
            adjoint ... {
                let __cond_0 : Bool = (Length(ctls) % 2) != 0;
                if __cond_0 {
                    Adjoint CCNOT(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], aux[Length(ctls) - 2]);
                }

            }
        }
        operation PhaseCCX(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
            body ... {
                H(target);
                CNOT(target, control1);
                CNOT(control1, control2);
                T(control2);
                Adjoint T(control1);
                T(target);
                CNOT(target, control1);
                CNOT(control1, control2);
                Adjoint T(control2);
                CNOT(target, control2);
                H(target);
            }
            adjoint ... {
                Adjoint H(target);
                Adjoint CNOT(target, control2);
                Adjoint Adjoint T(control2);
                Adjoint CNOT(control1, control2);
                Adjoint CNOT(target, control1);
                Adjoint T(target);
                Adjoint Adjoint T(control1);
                Adjoint T(control2);
                Adjoint CNOT(control1, control2);
                Adjoint CNOT(target, control1);
                Adjoint H(target);
            }
        }
        operation AND(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
            body ... {
                PhaseCCX(control1, control2, target);
            }
            adjoint ... {
                Adjoint PhaseCCX(control1, control2, target);
            }
        }
        operation CCNOT(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__ccx__body(control1, control2, target);
            }
            adjoint ... {
                __quantum__qis__ccx__body(control1, control2, target);
            }
            controlled (ctls, ...) {
                Controlled X(ctls + [control1, control2], target);
            }
            controlled adjoint (ctls, ...) {
                Controlled X(ctls + [control1, control2], target);
            }
        }
        operation CNOT(control : Qubit, target : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__cx__body(control, target);
            }
            adjoint ... {
                __quantum__qis__cx__body(control, target);
            }
            controlled (ctls, ...) {
                Controlled X(ctls + [control], target);
            }
            controlled adjoint (ctls, ...) {
                Controlled X(ctls + [control], target);
            }
        }
        operation H(qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__h__body(qubit);
            }
            adjoint ... {
                __quantum__qis__h__body(qubit);
            }
            controlled (ctls, ...) {
                mutable __cond_3 : Bool = false;
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                if __cond_0 {
                    __quantum__qis__h__body(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        CH(ctls[0], qubit);
                    } else {
                        __cond_2 = Length(ctls) == 2;
                        if __cond_2 {
                            CCH(ctls[0], ctls[1], qubit);
                        } else {
                            let aux : Qubit[] = AllocateQubitArray((Length(ctls) - 1) - (Length(ctls) % 2));
                            let _generated_ident_35 : Unit = {
                                {
                                    CollectControls(ctls, aux, 0);
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = (Length(ctls) % 2) != 0;
                                    if __cond_3 {
                                        CCH(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                                    } else {
                                        CCH(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 0);
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_35
                        }

                    }

                }

            }
            controlled adjoint (ctls, ...) {
                mutable __cond_3 : Bool = false;
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                if __cond_0 {
                    __quantum__qis__h__body(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        CH(ctls[0], qubit);
                    } else {
                        __cond_2 = Length(ctls) == 2;
                        if __cond_2 {
                            CCH(ctls[0], ctls[1], qubit);
                        } else {
                            let aux : Qubit[] = AllocateQubitArray((Length(ctls) - 1) - (Length(ctls) % 2));
                            let _generated_ident_36 : Unit = {
                                {
                                    CollectControls(ctls, aux, 0);
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = (Length(ctls) % 2) != 0;
                                    if __cond_3 {
                                        CCH(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                                    } else {
                                        CCH(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 0);
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_36
                        }

                    }

                }

            }
        }
        operation M(qubit : Qubit) : Result {
            __quantum__qis__m__body(qubit)
        }
        operation R(pauli : Pauli, theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                if pauli == PauliX {
                    Rx(theta, qubit);
                } else if pauli == PauliY {
                    Ry(theta, qubit);
                } else if pauli == PauliZ {
                    Rz(theta, qubit);
                } else {
                    ApplyGlobalPhase(((-theta)) / 2.);
                }

            }
            adjoint ... {
                if pauli == PauliX {
                    Adjoint Rx(theta, qubit);
                } else if pauli == PauliY {
                    Adjoint Ry(theta, qubit);
                } else if pauli == PauliZ {
                    Adjoint Rz(theta, qubit);
                } else {
                    Adjoint ApplyGlobalPhase(((-theta)) / 2.);
                }

            }
            controlled (ctls, ...) {
                if pauli == PauliX {
                    Controlled Rx(ctls, (theta, qubit));
                } else if pauli == PauliY {
                    Controlled Ry(ctls, (theta, qubit));
                } else if pauli == PauliZ {
                    Controlled Rz(ctls, (theta, qubit));
                } else {
                    Controlled ApplyGlobalPhase(ctls, ((-theta)) / 2.);
                }

            }
            controlled adjoint (ctls, ...) {
                if pauli == PauliX {
                    Controlled Adjoint Rx(ctls, (theta, qubit));
                } else if pauli == PauliY {
                    Controlled Adjoint Ry(ctls, (theta, qubit));
                } else if pauli == PauliZ {
                    Controlled Adjoint Rz(ctls, (theta, qubit));
                } else {
                    Controlled Adjoint ApplyGlobalPhase(ctls, ((-theta)) / 2.);
                }

            }
        }
        operation R1Frac(numerator : Int, power : Int, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                RFrac(PauliZ, (-numerator), power + 1, qubit);
                RFrac(PauliI, numerator, power + 1, qubit);
            }
            adjoint ... {
                Adjoint RFrac(PauliI, numerator, power + 1, qubit);
                Adjoint RFrac(PauliZ, (-numerator), power + 1, qubit);
            }
            controlled (ctls, ...) {
                Controlled RFrac(ctls, (PauliZ, (-numerator), power + 1, qubit));
                Controlled RFrac(ctls, (PauliI, numerator, power + 1, qubit));
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint RFrac(ctls, (PauliI, numerator, power + 1, qubit));
                Controlled Adjoint RFrac(ctls, (PauliZ, (-numerator), power + 1, qubit));
            }
        }
        operation Reset(qubit : Qubit) : Unit {
            __quantum__qis__reset__body(qubit);
        }
        operation ResetAll(qubits : Qubit[]) : Unit {
            {
                let _array_id_37 : Qubit[] = qubits;
                let _len_id_38 : Int = Length(_array_id_37);
                mutable _index_id_39 : Int = 0;
                while _index_id_39 < _len_id_38 {
                    let q : Qubit = _array_id_37[_index_id_39];
                    Reset(q);
                    _index_id_39 += 1;
                }

            }

        }
        operation RFrac(pauli : Pauli, numerator : Int, power : Int, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                let angle : Double = ((((-2.)) * PI()) * IntAsDouble(numerator)) / (2.^IntAsDouble(power));
                R(pauli, angle, qubit);
            }
            adjoint ... {
                let angle : Double = ((((-2.)) * PI()) * IntAsDouble(numerator)) / (2.^IntAsDouble(power));
                Adjoint R(pauli, angle, qubit);
            }
            controlled (ctls, ...) {
                let angle : Double = ((((-2.)) * PI()) * IntAsDouble(numerator)) / (2.^IntAsDouble(power));
                Controlled R(ctls, (pauli, angle, qubit));
            }
            controlled adjoint (ctls, ...) {
                let angle : Double = ((((-2.)) * PI()) * IntAsDouble(numerator)) / (2.^IntAsDouble(power));
                Controlled Adjoint R(ctls, (pauli, angle, qubit));
            }
        }
        operation Rx(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__rx__body(theta, qubit);
            }
            adjoint ... {
                Rx((-theta), qubit);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                if __cond_0 {
                    __quantum__qis__rx__body(theta, qubit);
                } else {
                    {
                        {
                            MapPauliAxis(PauliZ, PauliX, qubit);
                        }

                        let _apply_res : Unit = {
                            Controlled Rz(ctls, (theta, qubit));
                        };
                        {
                            Adjoint MapPauliAxis(PauliZ, PauliX, qubit);
                        }

                        _apply_res
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled Rx(ctls, ((-theta), qubit));
            }
        }
        operation Ry(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__ry__body(theta, qubit);
            }
            adjoint ... {
                Ry((-theta), qubit);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                if __cond_0 {
                    __quantum__qis__ry__body(theta, qubit);
                } else {
                    {
                        {
                            MapPauliAxis(PauliZ, PauliY, qubit);
                        }

                        let _apply_res : Unit = {
                            Controlled Rz(ctls, (theta, qubit));
                        };
                        {
                            Adjoint MapPauliAxis(PauliZ, PauliY, qubit);
                        }

                        _apply_res
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled Ry(ctls, ((-theta), qubit));
            }
        }
        operation Rz(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__rz__body(theta, qubit);
            }
            adjoint ... {
                Rz((-theta), qubit);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                if __cond_0 {
                    __quantum__qis__rz__body(theta, qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        CRz(ctls[0], theta, qubit);
                    } else {
                        let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 1);
                        let _generated_ident_40 : Unit = {
                            {
                                CollectControls(ctls, aux, 0);
                                AdjustForSingleControl(ctls, aux);
                            }

                            let _apply_res : Unit = {
                                CRz(aux[Length(ctls) - 2], theta, qubit);
                            };
                            {
                                Adjoint AdjustForSingleControl(ctls, aux);
                                Adjoint CollectControls(ctls, aux, 0);
                            }

                            _apply_res
                        };
                        ReleaseQubitArray(aux);
                        _generated_ident_40
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled Rz(ctls, ((-theta), qubit));
            }
        }
        operation S(qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__s__body(qubit);
            }
            adjoint ... {
                __quantum__qis__s__adj(qubit);
            }
            controlled (ctls, ...) {
                mutable __cond_3 : Bool = false;
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                if __cond_0 {
                    __quantum__qis__s__body(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        CS(ctls[0], qubit);
                    } else {
                        __cond_2 = Length(ctls) == 2;
                        if __cond_2 {
                            Controlled CS([ctls[0]], (ctls[1], qubit));
                        } else {
                            let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 2);
                            let _generated_ident_41 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = (Length(ctls) % 2) != 0;
                                    if __cond_3 {
                                        Controlled CS([ctls[Length(ctls) - 1]], (aux[Length(ctls) - 3], qubit));
                                    } else {
                                        Controlled CS([aux[Length(ctls) - 3]], (aux[Length(ctls) - 4], qubit));
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_41
                        }

                    }

                }

            }
            controlled adjoint (ctls, ...) {
                mutable __cond_3 : Bool = false;
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                if __cond_0 {
                    __quantum__qis__s__adj(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        Adjoint CS(ctls[0], qubit);
                    } else {
                        __cond_2 = Length(ctls) == 2;
                        if __cond_2 {
                            Controlled Adjoint CS([ctls[0]], (ctls[1], qubit));
                        } else {
                            let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 2);
                            let _generated_ident_42 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = (Length(ctls) % 2) != 0;
                                    if __cond_3 {
                                        Controlled Adjoint CS([ctls[Length(ctls) - 1]], (aux[Length(ctls) - 3], qubit));
                                    } else {
                                        Controlled Adjoint CS([aux[Length(ctls) - 3]], (aux[Length(ctls) - 4], qubit));
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_42
                        }

                    }

                }

            }
        }
        operation SWAP(qubit1 : Qubit, qubit2 : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__swap__body(qubit1, qubit2);
            }
            adjoint ... {
                __quantum__qis__swap__body(qubit1, qubit2);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                if __cond_0 {
                    __quantum__qis__swap__body(qubit1, qubit2);
                } else {
                    {
                        {
                            CNOT(qubit1, qubit2);
                        }

                        let _apply_res : Unit = {
                            Controlled CNOT(ctls, (qubit2, qubit1));
                        };
                        {
                            Adjoint CNOT(qubit1, qubit2);
                        }

                        _apply_res
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                if __cond_0 {
                    __quantum__qis__swap__body(qubit1, qubit2);
                } else {
                    {
                        {
                            CNOT(qubit1, qubit2);
                        }

                        let _apply_res : Unit = {
                            Controlled CNOT(ctls, (qubit2, qubit1));
                        };
                        {
                            Adjoint CNOT(qubit1, qubit2);
                        }

                        _apply_res
                    }

                }

            }
        }
        operation T(qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__t__body(qubit);
            }
            adjoint ... {
                __quantum__qis__t__adj(qubit);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                if __cond_0 {
                    __quantum__qis__t__body(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        CT(ctls[0], qubit);
                    } else {
                        let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 1);
                        let _generated_ident_43 : Unit = {
                            {
                                CollectControls(ctls, aux, 0);
                                AdjustForSingleControl(ctls, aux);
                            }

                            let _apply_res : Unit = {
                                CT(aux[Length(ctls) - 2], qubit);
                            };
                            {
                                Adjoint AdjustForSingleControl(ctls, aux);
                                Adjoint CollectControls(ctls, aux, 0);
                            }

                            _apply_res
                        };
                        ReleaseQubitArray(aux);
                        _generated_ident_43
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                if __cond_0 {
                    __quantum__qis__t__adj(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        Adjoint CT(ctls[0], qubit);
                    } else {
                        let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 1);
                        let _generated_ident_44 : Unit = {
                            {
                                CollectControls(ctls, aux, 0);
                                AdjustForSingleControl(ctls, aux);
                            }

                            let _apply_res : Unit = {
                                Adjoint CT(aux[Length(ctls) - 2], qubit);
                            };
                            {
                                Adjoint AdjustForSingleControl(ctls, aux);
                                Adjoint CollectControls(ctls, aux, 0);
                            }

                            _apply_res
                        };
                        ReleaseQubitArray(aux);
                        _generated_ident_44
                    }

                }

            }
        }
        operation X(qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__x__body(qubit);
            }
            adjoint ... {
                __quantum__qis__x__body(qubit);
            }
            controlled (ctls, ...) {
                mutable __cond_3 : Bool = false;
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                if __cond_0 {
                    __quantum__qis__x__body(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        __quantum__qis__cx__body(ctls[0], qubit);
                    } else {
                        __cond_2 = Length(ctls) == 2;
                        if __cond_2 {
                            __quantum__qis__ccx__body(ctls[0], ctls[1], qubit);
                        } else {
                            let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 2);
                            let _generated_ident_45 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = (Length(ctls) % 2) != 0;
                                    if __cond_3 {
                                        __quantum__qis__ccx__body(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                                    } else {
                                        __quantum__qis__ccx__body(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_45
                        }

                    }

                }

            }
            controlled adjoint (ctls, ...) {
                mutable __cond_3 : Bool = false;
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                if __cond_0 {
                    __quantum__qis__x__body(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        __quantum__qis__cx__body(ctls[0], qubit);
                    } else {
                        __cond_2 = Length(ctls) == 2;
                        if __cond_2 {
                            __quantum__qis__ccx__body(ctls[0], ctls[1], qubit);
                        } else {
                            let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 2);
                            let _generated_ident_46 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = (Length(ctls) % 2) != 0;
                                    if __cond_3 {
                                        __quantum__qis__ccx__body(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                                    } else {
                                        __quantum__qis__ccx__body(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_46
                        }

                    }

                }

            }
        }
        function Message(msg : String) : Unit {
            body intrinsic;
        }
        function PI() : Double {
            3.141592653589793
        }
        function SignI(a : Int) : Int {
            if a < 0 {
                (-1)
            } else if a > 0 {
                (+ 1)
            } else {
                0
            }

        }
        function AbsI(a : Int) : Int {
            if a < 0 {
                (-a)
            } else {
                a
            }
        }
        function MaxI(a : Int, b : Int) : Int {
            if a > b {
                a
            } else {
                b
            }
        }
        function ModulusI(value : Int, modulus : Int) : Int {
            Fact(modulus > 0, $"`modulus` must be positive");
            let r : Int = value % modulus;
            if r < 0 {
                r + modulus
            } else {
                r
            }
        }
        function ExpModI(expBase : Int, power : Int, modulus : Int) : Int {
            mutable __has_returned : Bool = false;
            mutable __ret_val : Int = 0;
            Fact(power >= 0, $"`power` must be non-negative");
            Fact(modulus > 0, $"`modulus` must be positive");
            Fact(expBase > 0, $"`expBase` must be positive");
            if modulus == 1 {
                {
                    __ret_val = 0;
                    __has_returned = true;
                };
            }

            mutable res : Int = if (not __has_returned) {
                1
            } else {
                0
            };
            mutable expPow2mod : Int = if (not __has_returned) {
                expBase % modulus
            } else {
                0
            };
            mutable powerBits : Int = if (not __has_returned) {
                power
            } else {
                0
            };
            if (not __has_returned) {
                while powerBits > 0 {
                    if (powerBits &&& 1) != 0 {
                        res = (res * expPow2mod) % modulus;
                    }

                    expPow2mod = (expPow2mod * expPow2mod) % modulus;
                    powerBits >>>= 1;
                }

            };
            if __has_returned {
                __ret_val
            } else {
                if (not __has_returned) {
                    res
                } else {
                    __ret_val
                }
            }

        }
        function InverseModI(a : Int, modulus : Int) : Int {
            let (u : Int, v : Int) = ExtendedGreatestCommonDivisorI(a, modulus);
            let gcd : Int = (u * a) + (v * modulus);
            Fact(gcd == 1, $"`a` and `modulus` must be co-prime");
            ModulusI(u, modulus)
        }
        function GreatestCommonDivisorI(a : Int, b : Int) : Int {
            mutable aa : Int = AbsI(a);
            mutable bb : Int = AbsI(b);
            while bb != 0 {
                let cc : Int = aa % bb;
                aa = bb;
                bb = cc;
            }

            aa
        }
        function ExtendedGreatestCommonDivisorI(a : Int, b : Int) : (Int, Int) {
            let signA : Int = SignI(a);
            let signB : Int = SignI(b);
            mutable (s1 : Int, s2 : Int) = (1, 0);
            mutable (t1 : Int, t2 : Int) = (0, 1);
            mutable (r1 : Int, r2 : Int) = (a * signA, b * signB);
            while r2 != 0 {
                let quotient : Int = r1 / r2;
                (r1, r2) = (r2, r1 - (quotient * r2));
                (s1, s2) = (s2, s1 - (quotient * s2));
                (t1, t2) = (t2, t1 - (quotient * t2));
            }

            (s1 * signA, t1 * signB)
        }
        function ContinuedFractionConvergentI(fraction_0 : Int, fraction_1 : Int, denominatorBound : Int) : (Int, Int) {
            Fact(denominatorBound > 0, $"Denominator bound must be positive");
            let a : Int = fraction_0;
            let b : Int = fraction_1;
            let signA : Int = SignI(a);
            let signB : Int = SignI(b);
            mutable (s1 : Int, s2 : Int) = (1, 0);
            mutable (t1 : Int, t2 : Int) = (0, 1);
            mutable (r1 : Int, r2 : Int) = (a * signA, b * signB);
            while (r2 != 0) and (AbsI(s2) <= denominatorBound) {
                let quotient : Int = r1 / r2;
                (r1, r2) = (r2, r1 - (quotient * r2));
                (s1, s2) = (s2, s1 - (quotient * s2));
                (t1, t2) = (t2, t1 - (quotient * t2));
            }

            if (r2 == 0) and (AbsI(s2) <= denominatorBound) {
                (((-t2)) * signB, s2 * signA)
            } else {
                (((-t1)) * signB, s1 * signA)
            }

        }
        function BitSizeI(a : Int) : Int {
            Fact(a >= 0, $"`a` must be non-negative.");
            mutable number : Int = a;
            mutable size : Int = 0;
            while number != 0 {
                size = size + 1;
                number = number >>> 1;
            }

            size
        }
        function TrailingZeroCountI(a : Int) : Int {
            Fact(a != 0, $"TrailingZeroCountI: `a` cannot be 0.");
            mutable count : Int = 0;
            mutable n : Int = a;
            while (n &&& 1) == 0 {
                count += 1;
                n >>>= 1;
            }

            count
        }
        function TrailingZeroCountL(a : BigInt) : Int {
            Fact(a != 0L, $"TrailingZeroCountL: `a` cannot be 0.");
            mutable count : Int = 0;
            mutable n : BigInt = a;
            while (n &&& 1L) == 0L {
                count += 1;
                n >>>= 1;
            }

            count
        }
        operation __quantum__qis__ccx__body(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__cx__body(control : Qubit, target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__rx__body(angle : Double, target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__ry__body(angle : Double, target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__rz__body(angle : Double, target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__h__body(target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__s__body(target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__s__adj(target : Qubit) : Unit {
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
        operation __quantum__qis__swap__body(target1 : Qubit, target2 : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__m__body(target : Qubit) : Result {
            body intrinsic;
        }
        operation __quantum__qis__reset__body(target : Qubit) : Unit {
            body intrinsic;
        }
        operation IncByI(c : Int, ys : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                IncByIUsingIncByLE_AdjCtl__RippleCarryTTKIncByLE_(c, ys);
            }
            adjoint ... {
                Adjoint IncByIUsingIncByLE_AdjCtl__RippleCarryTTKIncByLE_(c, ys);
            }
            controlled (ctls, ...) {
                Controlled IncByIUsingIncByLE_AdjCtl__RippleCarryTTKIncByLE_(ctls, (c, ys));
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint IncByIUsingIncByLE_AdjCtl__RippleCarryTTKIncByLE_(ctls, (c, ys));
            }
        }
        operation RippleCarryTTKIncByLE(xs : Qubit[], ys : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                let xsLen : Int = Length(xs);
                let ysLen : Int = Length(ys);
                Fact(ysLen >= xsLen, $"Register `ys` must be longer than register `xs`.");
                Fact(xsLen >= 1, $"Registers `xs` and `ys` must contain at least one qubit.");
                if xsLen == ysLen {
                    if xsLen > 1 {
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res : Unit = {
                                ApplyInnerTTKAdderNoCarry(xs, ys);
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res
                        }

                    }

                    CNOT(xs[0], ys[0]);
                } else if (xsLen + 1) == ysLen {
                    if xsLen > 1 {
                        CNOT(xs[xsLen - 1], ys[ysLen - 1]);
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res_1 : Unit = {
                                ApplyInnerTTKAdderWithCarry(xs, ys);
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res_1
                        }

                    } else {
                        CCNOT(xs[0], ys[0], ys[1]);
                    }

                    CNOT(xs[0], ys[0]);
                } else if (xsLen + 2) <= ysLen {
                    let padding : Qubit[] = AllocateQubitArray((ysLen - xsLen) - 1);
                    RippleCarryTTKIncByLE(xs + padding, ys);
                    ReleaseQubitArray(padding);
                }

            }
            adjoint ... {
                let xsLen : Int = Length(xs);
                let ysLen : Int = Length(ys);
                Fact(ysLen >= xsLen, $"Register `ys` must be longer than register `xs`.");
                Fact(xsLen >= 1, $"Registers `xs` and `ys` must contain at least one qubit.");
                if xsLen == ysLen {
                    Adjoint CNOT(xs[0], ys[0]);
                    if xsLen > 1 {
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res : Unit = {
                                Adjoint ApplyInnerTTKAdderNoCarry(xs, ys);
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res
                        }

                    }

                } else if (xsLen + 1) == ysLen {
                    Adjoint CNOT(xs[0], ys[0]);
                    if xsLen > 1 {
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res_1 : Unit = {
                                Adjoint ApplyInnerTTKAdderWithCarry(xs, ys);
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res_1
                        }

                        Adjoint CNOT(xs[xsLen - 1], ys[ysLen - 1]);
                    } else {
                        Adjoint CCNOT(xs[0], ys[0], ys[1]);
                    }

                } else if (xsLen + 2) <= ysLen {
                    let padding : Qubit[] = AllocateQubitArray((ysLen - xsLen) - 1);
                    Adjoint RippleCarryTTKIncByLE(xs + padding, ys);
                    ReleaseQubitArray(padding);
                }

            }
            controlled (ctls, ...) {
                let xsLen : Int = Length(xs);
                let ysLen : Int = Length(ys);
                Fact(ysLen >= xsLen, $"Register `ys` must be longer than register `xs`.");
                Fact(xsLen >= 1, $"Registers `xs` and `ys` must contain at least one qubit.");
                if xsLen == ysLen {
                    if xsLen > 1 {
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res : Unit = {
                                Controlled ApplyInnerTTKAdderNoCarry(ctls, (xs, ys));
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res
                        }

                    }

                    Controlled CNOT(ctls, (xs[0], ys[0]));
                } else if (xsLen + 1) == ysLen {
                    if xsLen > 1 {
                        Controlled CNOT(ctls, (xs[xsLen - 1], ys[ysLen - 1]));
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res_1 : Unit = {
                                Controlled ApplyInnerTTKAdderWithCarry(ctls, (xs, ys));
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res_1
                        }

                    } else {
                        Controlled CCNOT(ctls, (xs[0], ys[0], ys[1]));
                    }

                    Controlled CNOT(ctls, (xs[0], ys[0]));
                } else if (xsLen + 2) <= ysLen {
                    let padding : Qubit[] = AllocateQubitArray((ysLen - xsLen) - 1);
                    Controlled RippleCarryTTKIncByLE(ctls, (xs + padding, ys));
                    ReleaseQubitArray(padding);
                }

            }
            controlled adjoint (ctls, ...) {
                let xsLen : Int = Length(xs);
                let ysLen : Int = Length(ys);
                Fact(ysLen >= xsLen, $"Register `ys` must be longer than register `xs`.");
                Fact(xsLen >= 1, $"Registers `xs` and `ys` must contain at least one qubit.");
                if xsLen == ysLen {
                    Controlled Adjoint CNOT(ctls, (xs[0], ys[0]));
                    if xsLen > 1 {
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res : Unit = {
                                Controlled Adjoint ApplyInnerTTKAdderNoCarry(ctls, (xs, ys));
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res
                        }

                    }

                } else if (xsLen + 1) == ysLen {
                    Controlled Adjoint CNOT(ctls, (xs[0], ys[0]));
                    if xsLen > 1 {
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res_1 : Unit = {
                                Controlled Adjoint ApplyInnerTTKAdderWithCarry(ctls, (xs, ys));
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res_1
                        }

                        Controlled Adjoint CNOT(ctls, (xs[xsLen - 1], ys[ysLen - 1]));
                    } else {
                        Controlled Adjoint CCNOT(ctls, (xs[0], ys[0], ys[1]));
                    }

                } else if (xsLen + 2) <= ysLen {
                    let padding : Qubit[] = AllocateQubitArray((ysLen - xsLen) - 1);
                    Controlled Adjoint RippleCarryTTKIncByLE(ctls, (xs + padding, ys));
                    ReleaseQubitArray(padding);
                }

            }
        }
        operation ApplyOuterTTKAdder(xs : Qubit[], ys : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range_id_47 : Range = 1..Length(xs) - 1;
                    mutable _index_id_48 : Int = _range_id_47.Start;
                    let _step_id_49 : Int = _range_id_47.Step;
                    let _end_id_50 : Int = _range_id_47.End;
                    while ((_step_id_49 > 0) and (_index_id_48 <= _end_id_50)) or ((_step_id_49 < 0) and (_index_id_48 >= _end_id_50)) {
                        let i : Int = _index_id_48;
                        CNOT(xs[i], ys[i]);
                        _index_id_48 += _step_id_49;
                    }

                }

                {
                    let _range_id_51 : Range = Length(xs) - 2..(-1)..1;
                    mutable _index_id_52 : Int = _range_id_51.Start;
                    let _step_id_53 : Int = _range_id_51.Step;
                    let _end_id_54 : Int = _range_id_51.End;
                    while ((_step_id_53 > 0) and (_index_id_52 <= _end_id_54)) or ((_step_id_53 < 0) and (_index_id_52 >= _end_id_54)) {
                        let i_1 : Int = _index_id_52;
                        CNOT(xs[i_1], xs[i_1 + 1]);
                        _index_id_52 += _step_id_53;
                    }

                }

            }
            adjoint ... {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range : Range = Length(xs) - 2..(-1)..1;
                    {
                        let _range_id_55 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                        mutable _index_id_56 : Int = _range_id_55.Start;
                        let _step_id_57 : Int = _range_id_55.Step;
                        let _end_id_58 : Int = _range_id_55.End;
                        while ((_step_id_57 > 0) and (_index_id_56 <= _end_id_58)) or ((_step_id_57 < 0) and (_index_id_56 >= _end_id_58)) {
                            let i : Int = _index_id_56;
                            Adjoint CNOT(xs[i], xs[i + 1]);
                            _index_id_56 += _step_id_57;
                        }

                    }

                }

                {
                    let _range_1 : Range = 1..Length(xs) - 1;
                    {
                        let _range_id_59 : Range = _range_1.Start + (((_range_1.End - _range_1.Start) / _range_1.Step) * _range_1.Step)..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_60 : Int = _range_id_59.Start;
                        let _step_id_61 : Int = _range_id_59.Step;
                        let _end_id_62 : Int = _range_id_59.End;
                        while ((_step_id_61 > 0) and (_index_id_60 <= _end_id_62)) or ((_step_id_61 < 0) and (_index_id_60 >= _end_id_62)) {
                            let i_1 : Int = _index_id_60;
                            Adjoint CNOT(xs[i_1], ys[i_1]);
                            _index_id_60 += _step_id_61;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range_id_63 : Range = 1..Length(xs) - 1;
                    mutable _index_id_64 : Int = _range_id_63.Start;
                    let _step_id_65 : Int = _range_id_63.Step;
                    let _end_id_66 : Int = _range_id_63.End;
                    while ((_step_id_65 > 0) and (_index_id_64 <= _end_id_66)) or ((_step_id_65 < 0) and (_index_id_64 >= _end_id_66)) {
                        let i : Int = _index_id_64;
                        Controlled CNOT(ctls, (xs[i], ys[i]));
                        _index_id_64 += _step_id_65;
                    }

                }

                {
                    let _range_id_67 : Range = Length(xs) - 2..(-1)..1;
                    mutable _index_id_68 : Int = _range_id_67.Start;
                    let _step_id_69 : Int = _range_id_67.Step;
                    let _end_id_70 : Int = _range_id_67.End;
                    while ((_step_id_69 > 0) and (_index_id_68 <= _end_id_70)) or ((_step_id_69 < 0) and (_index_id_68 >= _end_id_70)) {
                        let i_1 : Int = _index_id_68;
                        Controlled CNOT(ctls, (xs[i_1], xs[i_1 + 1]));
                        _index_id_68 += _step_id_69;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range : Range = Length(xs) - 2..(-1)..1;
                    {
                        let _range_id_71 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                        mutable _index_id_72 : Int = _range_id_71.Start;
                        let _step_id_73 : Int = _range_id_71.Step;
                        let _end_id_74 : Int = _range_id_71.End;
                        while ((_step_id_73 > 0) and (_index_id_72 <= _end_id_74)) or ((_step_id_73 < 0) and (_index_id_72 >= _end_id_74)) {
                            let i : Int = _index_id_72;
                            Controlled Adjoint CNOT(ctls, (xs[i], xs[i + 1]));
                            _index_id_72 += _step_id_73;
                        }

                    }

                }

                {
                    let _range_1 : Range = 1..Length(xs) - 1;
                    {
                        let _range_id_75 : Range = _range_1.Start + (((_range_1.End - _range_1.Start) / _range_1.Step) * _range_1.Step)..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_76 : Int = _range_id_75.Start;
                        let _step_id_77 : Int = _range_id_75.Step;
                        let _end_id_78 : Int = _range_id_75.End;
                        while ((_step_id_77 > 0) and (_index_id_76 <= _end_id_78)) or ((_step_id_77 < 0) and (_index_id_76 >= _end_id_78)) {
                            let i_1 : Int = _index_id_76;
                            Controlled Adjoint CNOT(ctls, (xs[i_1], ys[i_1]));
                            _index_id_76 += _step_id_77;
                        }

                    }

                }

            }
        }
        operation ApplyInnerTTKAdderNoCarry(xs : Qubit[], ys : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                Controlled ApplyInnerTTKAdderNoCarry([], (xs, ys));
            }
            adjoint ... {
                Adjoint Controlled ApplyInnerTTKAdderNoCarry([], (xs, ys));
            }
            controlled (controls, ...) {
                Fact(Length(xs) == Length(ys), $"Input registers must have the same number of qubits.");
                {
                    let _range_id_79 : Range = 0..Length(xs) - 2;
                    mutable _index_id_80 : Int = _range_id_79.Start;
                    let _step_id_81 : Int = _range_id_79.Step;
                    let _end_id_82 : Int = _range_id_79.End;
                    while ((_step_id_81 > 0) and (_index_id_80 <= _end_id_82)) or ((_step_id_81 < 0) and (_index_id_80 >= _end_id_82)) {
                        let idx : Int = _index_id_80;
                        CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                        _index_id_80 += _step_id_81;
                    }

                }

                {
                    let _range_id_83 : Range = Length(xs) - 1..(-1)..1;
                    mutable _index_id_84 : Int = _range_id_83.Start;
                    let _step_id_85 : Int = _range_id_83.Step;
                    let _end_id_86 : Int = _range_id_83.End;
                    while ((_step_id_85 > 0) and (_index_id_84 <= _end_id_86)) or ((_step_id_85 < 0) and (_index_id_84 >= _end_id_86)) {
                        let idx_1 : Int = _index_id_84;
                        Controlled CNOT(controls, (xs[idx_1], ys[idx_1]));
                        CCNOT(xs[idx_1 - 1], ys[idx_1 - 1], xs[idx_1]);
                        _index_id_84 += _step_id_85;
                    }

                }

            }
            controlled adjoint (controls, ...) {
                Fact(Length(xs) == Length(ys), $"Input registers must have the same number of qubits.");
                {
                    let _range : Range = Length(xs) - 1..(-1)..1;
                    {
                        let _range_id_87 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                        mutable _index_id_88 : Int = _range_id_87.Start;
                        let _step_id_89 : Int = _range_id_87.Step;
                        let _end_id_90 : Int = _range_id_87.End;
                        while ((_step_id_89 > 0) and (_index_id_88 <= _end_id_90)) or ((_step_id_89 < 0) and (_index_id_88 >= _end_id_90)) {
                            let idx : Int = _index_id_88;
                            Adjoint CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                            Adjoint Controlled CNOT(controls, (xs[idx], ys[idx]));
                            _index_id_88 += _step_id_89;
                        }

                    }

                }

                {
                    let _range_1 : Range = 0..Length(xs) - 2;
                    {
                        let _range_id_91 : Range = _range_1.Start + (((_range_1.End - _range_1.Start) / _range_1.Step) * _range_1.Step)..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_92 : Int = _range_id_91.Start;
                        let _step_id_93 : Int = _range_id_91.Step;
                        let _end_id_94 : Int = _range_id_91.End;
                        while ((_step_id_93 > 0) and (_index_id_92 <= _end_id_94)) or ((_step_id_93 < 0) and (_index_id_92 >= _end_id_94)) {
                            let idx_1 : Int = _index_id_92;
                            Adjoint CCNOT(xs[idx_1], ys[idx_1], xs[idx_1 + 1]);
                            _index_id_92 += _step_id_93;
                        }

                    }

                }

            }
        }
        operation ApplyInnerTTKAdderWithCarry(xs : Qubit[], ys : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                Controlled ApplyInnerTTKAdderWithCarry([], (xs, ys));
            }
            adjoint ... {
                Adjoint Controlled ApplyInnerTTKAdderWithCarry([], (xs, ys));
            }
            controlled (controls, ...) {
                Fact((Length(xs) + 1) == Length(ys), $"ys must be one qubit longer than xs.");
                Fact(Length(xs) > 0, $"Array should not be empty.");
                let nQubits : Int = Length(xs);
                {
                    let _range_id_95 : Range = 0..nQubits - 2;
                    mutable _index_id_96 : Int = _range_id_95.Start;
                    let _step_id_97 : Int = _range_id_95.Step;
                    let _end_id_98 : Int = _range_id_95.End;
                    while ((_step_id_97 > 0) and (_index_id_96 <= _end_id_98)) or ((_step_id_97 < 0) and (_index_id_96 >= _end_id_98)) {
                        let idx : Int = _index_id_96;
                        CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                        _index_id_96 += _step_id_97;
                    }

                }

                Controlled CCNOT(controls, (xs[nQubits - 1], ys[nQubits - 1], ys[nQubits]));
                {
                    let _range_id_99 : Range = nQubits - 1..(-1)..1;
                    mutable _index_id_100 : Int = _range_id_99.Start;
                    let _step_id_101 : Int = _range_id_99.Step;
                    let _end_id_102 : Int = _range_id_99.End;
                    while ((_step_id_101 > 0) and (_index_id_100 <= _end_id_102)) or ((_step_id_101 < 0) and (_index_id_100 >= _end_id_102)) {
                        let idx_1 : Int = _index_id_100;
                        Controlled CNOT(controls, (xs[idx_1], ys[idx_1]));
                        CCNOT(xs[idx_1 - 1], ys[idx_1 - 1], xs[idx_1]);
                        _index_id_100 += _step_id_101;
                    }

                }

            }
            controlled adjoint (controls, ...) {
                Fact((Length(xs) + 1) == Length(ys), $"ys must be one qubit longer than xs.");
                Fact(Length(xs) > 0, $"Array should not be empty.");
                let nQubits : Int = Length(xs);
                {
                    let _range : Range = nQubits - 1..(-1)..1;
                    {
                        let _range_id_103 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                        mutable _index_id_104 : Int = _range_id_103.Start;
                        let _step_id_105 : Int = _range_id_103.Step;
                        let _end_id_106 : Int = _range_id_103.End;
                        while ((_step_id_105 > 0) and (_index_id_104 <= _end_id_106)) or ((_step_id_105 < 0) and (_index_id_104 >= _end_id_106)) {
                            let idx : Int = _index_id_104;
                            Adjoint CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                            Adjoint Controlled CNOT(controls, (xs[idx], ys[idx]));
                            _index_id_104 += _step_id_105;
                        }

                    }

                }

                Adjoint Controlled CCNOT(controls, (xs[nQubits - 1], ys[nQubits - 1], ys[nQubits]));
                {
                    let _range_1 : Range = 0..nQubits - 2;
                    {
                        let _range_id_107 : Range = _range_1.Start + (((_range_1.End - _range_1.Start) / _range_1.Step) * _range_1.Step)..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_108 : Int = _range_id_107.Start;
                        let _step_id_109 : Int = _range_id_107.Step;
                        let _end_id_110 : Int = _range_id_107.End;
                        while ((_step_id_109 > 0) and (_index_id_108 <= _end_id_110)) or ((_step_id_109 < 0) and (_index_id_108 >= _end_id_110)) {
                            let idx_1 : Int = _index_id_108;
                            Adjoint CCNOT(xs[idx_1], ys[idx_1], xs[idx_1 + 1]);
                            _index_id_108 += _step_id_109;
                        }

                    }

                }

            }
        }
        operation ApplyOrAssuming0Target(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
            body ... {
                {
                    {
                        X(control1);
                        X(control2);
                    }

                    let _apply_res : Unit = {
                        AND(control1, control2, target);
                        X(target);
                    };
                    {
                        Adjoint X(control2);
                        Adjoint X(control1);
                    }

                    _apply_res
                }

            }
            adjoint ... {
                {
                    {
                        X(control1);
                        X(control2);
                    }

                    let _apply_res : Unit = {
                        Adjoint X(target);
                        Adjoint AND(control1, control2, target);
                    };
                    {
                        Adjoint X(control2);
                        Adjoint X(control1);
                    }

                    _apply_res
                }

            }
        }
        function IndexRange_Qubit_(array : Qubit[]) : Range {
            0..Length(array) - 1
        }
        function IsEmpty_Qubit_(array : Qubit[]) : Bool {
            Length(array) == 0
        }
        function Head_Qubit_(array : Qubit[]) : Qubit {
            Fact(Length(array) > 0, $"Array must have at least 1 element");
            array[0]
        }
        function Most_Qubit_(array : Qubit[]) : Qubit[] {
            array[...Length(array) - 2]
        }
        function Tail_Qubit_(array : Qubit[]) : Qubit {
            let size : Int = Length(array);
            Fact(size > 0, $"Array must have at least 1 element");
            array[size - 1]
        }
        operation IncByIUsingIncByLE_AdjCtl__RippleCarryTTKIncByLE_(c : Int, ys : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                let ysLen : Int = Length(ys);
                Fact(ysLen > 0, $"Length of `ys` must be at least 1.");
                Fact(c >= 0, $"Constant `c` must be non-negative.");
                Fact(c < (2^ysLen), $"Constant `c` must be smaller than 2^Length(ys).");
                if c != 0 {
                    let j : Int = TrailingZeroCountI(c);
                    let x : Qubit[] = AllocateQubitArray(ysLen - j);
                    let _generated_ident_111 : Unit = {
                        {
                            ApplyXorInPlace(c >>> j, x);
                        }

                        let _apply_res : Unit = {
                            RippleCarryTTKIncByLE(x, ys[j...]);
                        };
                        {
                            Adjoint ApplyXorInPlace(c >>> j, x);
                        }

                        _apply_res
                    };
                    ReleaseQubitArray(x);
                    _generated_ident_111
                }

            }
            adjoint ... {
                let ysLen : Int = Length(ys);
                Fact(ysLen > 0, $"Length of `ys` must be at least 1.");
                Fact(c >= 0, $"Constant `c` must be non-negative.");
                Fact(c < (2^ysLen), $"Constant `c` must be smaller than 2^Length(ys).");
                if c != 0 {
                    let j : Int = TrailingZeroCountI(c);
                    let x : Qubit[] = AllocateQubitArray(ysLen - j);
                    let _generated_ident_112 : Unit = {
                        {
                            ApplyXorInPlace(c >>> j, x);
                        }

                        let _apply_res : Unit = {
                            Adjoint RippleCarryTTKIncByLE(x, ys[j...]);
                        };
                        {
                            Adjoint ApplyXorInPlace(c >>> j, x);
                        }

                        _apply_res
                    };
                    ReleaseQubitArray(x);
                    _generated_ident_112
                }

            }
            controlled (ctls, ...) {
                let ysLen : Int = Length(ys);
                Fact(ysLen > 0, $"Length of `ys` must be at least 1.");
                Fact(c >= 0, $"Constant `c` must be non-negative.");
                Fact(c < (2^ysLen), $"Constant `c` must be smaller than 2^Length(ys).");
                if c != 0 {
                    let j : Int = TrailingZeroCountI(c);
                    let x : Qubit[] = AllocateQubitArray(ysLen - j);
                    let _generated_ident_113 : Unit = {
                        {
                            ApplyXorInPlace(c >>> j, x);
                        }

                        let _apply_res : Unit = {
                            Controlled RippleCarryTTKIncByLE(ctls, (x, ys[j...]));
                        };
                        {
                            Adjoint ApplyXorInPlace(c >>> j, x);
                        }

                        _apply_res
                    };
                    ReleaseQubitArray(x);
                    _generated_ident_113
                }

            }
            controlled adjoint (ctls, ...) {
                let ysLen : Int = Length(ys);
                Fact(ysLen > 0, $"Length of `ys` must be at least 1.");
                Fact(c >= 0, $"Constant `c` must be non-negative.");
                Fact(c < (2^ysLen), $"Constant `c` must be smaller than 2^Length(ys).");
                if c != 0 {
                    let j : Int = TrailingZeroCountI(c);
                    let x : Qubit[] = AllocateQubitArray(ysLen - j);
                    let _generated_ident_114 : Unit = {
                        {
                            ApplyXorInPlace(c >>> j, x);
                        }

                        let _apply_res : Unit = {
                            Controlled Adjoint RippleCarryTTKIncByLE(ctls, (x, ys[j...]));
                        };
                        {
                            Adjoint ApplyXorInPlace(c >>> j, x);
                        }

                        _apply_res
                    };
                    ReleaseQubitArray(x);
                    _generated_ident_114
                }

            }
        }
        // package 2
        operation Main() : (Int, Int) {
            let n : Int = 187;
            let (a : Int, b : Int) = FactorSemiprimeInteger(n);
            Message($"Found factorization {n} = {a} * {b}");
            (a, b)
        }
        operation FactorSemiprimeInteger(number : Int) : (Int, Int) {
            mutable __cond_0 : Bool = false;
            mutable __has_returned : Bool = false;
            mutable __ret_val : (Int, Int) = (0, 0);
            if (number % 2) == 0 {
                Message($"An even number has been given; 2 is a factor.");
                {
                    __ret_val = (number / 2, 2);
                    __has_returned = true;
                };
            }

            mutable foundFactors : Bool = {
                false
            };
            mutable factors : (Int, Int) = if (not __has_returned) {
                (1, 1)
            } else {
                (0, 0)
            };
            mutable attempt : Int = if (not __has_returned) {
                1
            } else {
                0
            };
            if (not __has_returned) {
                {
                    mutable _continue_cond_115 : Bool = true;
                    while _continue_cond_115 {
                        Message($"*** Factorizing {number}, attempt {attempt}.");
                        let generator : Int = 2;
                        __cond_0 = GreatestCommonDivisorI(generator, number) == 1;
                        if __cond_0 {
                            Message($"Estimating period of {generator}.");
                            let period : Int = EstimatePeriod(generator, number);
                            (foundFactors, factors) = MaybeFactorsFromPeriod(number, generator, period);
                        } else {
                            let gcd : Int = GreatestCommonDivisorI(number, generator);
                            Message($"We have guessed a divisor {gcd} by accident. " + $"No quantum computation was done.");
                            foundFactors = true;
                            factors = (gcd, number / gcd);
                        }

                        attempt = attempt + 1;
                        if attempt > 100 {
                            fail $"Failed to find factors: too many attempts!";
                        }

                        _continue_cond_115 = (not foundFactors);
                        if _continue_cond_115 {
                            Message($"The estimated period did not yield a valid factor. " + $"Trying again.");
                        }

                    }

                }

            };
            if (not __has_returned) {
                {
                    __ret_val = (factors::Item < 0 >, factors::Item < 1 >);
                    __has_returned = true;
                };
            };
            __ret_val
        }
        function MaybeFactorsFromPeriod(modulus : Int, generator : Int, period : Int) : (Bool, (Int, Int)) {
            mutable __has_returned : Bool = false;
            mutable __ret_val : (Bool, (Int, Int)) = (false, (0, 0));
            if (period % 2) == 0 {
                let halfPower : Int = ExpModI(generator, period / 2, modulus);
                if halfPower != (modulus - 1) {
                    let factor : Int = MaxI(GreatestCommonDivisorI(halfPower - 1, modulus), GreatestCommonDivisorI(halfPower + 1, modulus));
                    if (factor != 1) and (factor != modulus) {
                        Message($"Found factor={factor}");
                        {
                            __ret_val = (true, (factor, modulus / factor));
                            __has_returned = true;
                        };
                    }

                }

                if (not __has_returned) {
                    Message($"Found trivial factors.");
                };
                if (not __has_returned) {
                    {
                        __ret_val = (false, (1, 1));
                        __has_returned = true;
                    };
                };
            } else {
                Message($"Estimated period {period} was odd, trying again.");
                {
                    __ret_val = (false, (1, 1));
                    __has_returned = true;
                };
            }

            __ret_val
        }
        function PeriodFromFrequency(modulus : Int, frequencyEstimate : Int, bitsPrecision : Int, currentDivisor : Int) : Int {
            let (numerator : Int, period : Int) = ContinuedFractionConvergentI(frequencyEstimate, 2^bitsPrecision, modulus);
            let (numeratorAbs : Int, periodAbs : Int) = (AbsI(numerator), AbsI(period));
            let period_1 : Int = (periodAbs * currentDivisor) / GreatestCommonDivisorI(currentDivisor, periodAbs);
            Message($"Found period={period_1}");
            period_1
        }
        operation EstimatePeriod(generator : Int, modulus : Int) : Int {
            mutable __has_returned : Bool = false;
            mutable __ret_val : Int = 0;
            Fact(GreatestCommonDivisorI(generator, modulus) == 1, $"`generator` and `modulus` must be co-prime");
            let bitsize : Int = BitSizeI(modulus);
            let bitsPrecision : Int = (2 * bitsize) + 1;
            let frequencyEstimate : Int = EstimateFrequency(generator, modulus, bitsize);
            if frequencyEstimate != 0 {
                {
                    __ret_val = PeriodFromFrequency(modulus, frequencyEstimate, bitsPrecision, 1);
                    __has_returned = true;
                };
            } else {
                Message($"The estimated frequency was 0, trying again.");
                {
                    __ret_val = 1;
                    __has_returned = true;
                };
            }

            __ret_val
        }
        operation EstimateFrequency(generator : Int, modulus : Int, bitsize : Int) : Int {
            mutable __cond_0 : Bool = false;
            mutable __has_returned : Bool = false;
            mutable __ret_val : Int = 0;
            mutable frequencyEstimate : Int = 0;
            let bitsPrecision : Int = (2 * bitsize) + 1;
            Message($"Estimating frequency with bitsPrecision={bitsPrecision}.");
            let eigenstateRegister : Qubit[] = AllocateQubitArray(bitsize);
            ApplyXorInPlace(1, eigenstateRegister);
            let c : Qubit = __quantum__rt__qubit_allocate();
            {
                let _range_id_116 : Range = bitsPrecision - 1..(-1)..0;
                mutable _index_id_117 : Int = _range_id_116.Start;
                let _step_id_118 : Int = _range_id_116.Step;
                let _end_id_119 : Int = _range_id_116.End;
                while ((_step_id_118 > 0) and (_index_id_117 <= _end_id_119)) or ((_step_id_118 < 0) and (_index_id_117 >= _end_id_119)) {
                    let idx : Int = _index_id_117;
                    H(c);
                    Controlled ApplyOrderFindingOracle([c], (generator, modulus, 1 <<< idx, eigenstateRegister));
                    R1Frac(frequencyEstimate, (bitsPrecision - 1) - idx, c);
                    H(c);
                    __cond_0 = M(c) == One;
                    if __cond_0 {
                        X(c);
                        frequencyEstimate += 1 <<< ((bitsPrecision - 1) - idx);
                    }

                    _index_id_117 += _step_id_118;
                }

            }

            ResetAll(eigenstateRegister);
            Message($"Estimated frequency={frequencyEstimate}");
            {
                let _generated_ident_120 : Int = frequencyEstimate;
                __quantum__rt__qubit_release(c);
                ReleaseQubitArray(eigenstateRegister);
                {
                    __ret_val = _generated_ident_120;
                    __has_returned = true;
                };
            };
            if (not __has_returned) {
                __quantum__rt__qubit_release(c);
            };
            if (not __has_returned) {
                ReleaseQubitArray(eigenstateRegister);
            };
            __ret_val
        }
        operation ApplyOrderFindingOracle(generator : Int, modulus : Int, power : Int, target : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                ModularMultiplyByConstant(modulus, ExpModI(generator, power, modulus), target);
            }
            adjoint ... {
                Adjoint ModularMultiplyByConstant(modulus, ExpModI(generator, power, modulus), target);
            }
            controlled (ctls, ...) {
                Controlled ModularMultiplyByConstant(ctls, (modulus, ExpModI(generator, power, modulus), target));
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint ModularMultiplyByConstant(ctls, (modulus, ExpModI(generator, power, modulus), target));
            }
        }
        operation ModularMultiplyByConstant(modulus : Int, c : Int, y : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                {
                    let _range_id_121 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_122 : Int = _range_id_121.Start;
                    let _step_id_123 : Int = _range_id_121.Step;
                    let _end_id_124 : Int = _range_id_121.End;
                    while ((_step_id_123 > 0) and (_index_id_122 <= _end_id_124)) or ((_step_id_123 < 0) and (_index_id_122 >= _end_id_124)) {
                        let idx : Int = _index_id_122;
                        let shiftedC : Int = (c <<< idx) % modulus;
                        Controlled ModularAddConstant([y[idx]], (modulus, shiftedC, qs));
                        _index_id_122 += _step_id_123;
                    }

                }

                {
                    let _range_id_125 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_126 : Int = _range_id_125.Start;
                    let _step_id_127 : Int = _range_id_125.Step;
                    let _end_id_128 : Int = _range_id_125.End;
                    while ((_step_id_127 > 0) and (_index_id_126 <= _end_id_128)) or ((_step_id_127 < 0) and (_index_id_126 >= _end_id_128)) {
                        let idx_1 : Int = _index_id_126;
                        SWAP(y[idx_1], qs[idx_1]);
                        _index_id_126 += _step_id_127;
                    }

                }

                let invC : Int = InverseModI(c, modulus);
                let _generated_ident_129 : Unit = {
                    let _range_id_130 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_131 : Int = _range_id_130.Start;
                    let _step_id_132 : Int = _range_id_130.Step;
                    let _end_id_133 : Int = _range_id_130.End;
                    while ((_step_id_132 > 0) and (_index_id_131 <= _end_id_133)) or ((_step_id_132 < 0) and (_index_id_131 >= _end_id_133)) {
                        let idx_2 : Int = _index_id_131;
                        let shiftedC_1 : Int = (invC <<< idx_2) % modulus;
                        Controlled ModularAddConstant([y[idx_2]], (modulus, modulus - shiftedC_1, qs));
                        _index_id_131 += _step_id_132;
                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_129
            }
            adjoint ... {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                let invC : Int = InverseModI(c, modulus);
                {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_134 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                        mutable _index_id_135 : Int = _range_id_134.Start;
                        let _step_id_136 : Int = _range_id_134.Step;
                        let _end_id_137 : Int = _range_id_134.End;
                        while ((_step_id_136 > 0) and (_index_id_135 <= _end_id_137)) or ((_step_id_136 < 0) and (_index_id_135 >= _end_id_137)) {
                            let idx : Int = _index_id_135;
                            let shiftedC : Int = (invC <<< idx) % modulus;
                            Controlled Adjoint ModularAddConstant([y[idx]], (modulus, modulus - shiftedC, qs));
                            _index_id_135 += _step_id_136;
                        }

                    }

                }

                {
                    let _range_1 : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_138 : Range = _range_1.Start + (((_range_1.End - _range_1.Start) / _range_1.Step) * _range_1.Step)..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_139 : Int = _range_id_138.Start;
                        let _step_id_140 : Int = _range_id_138.Step;
                        let _end_id_141 : Int = _range_id_138.End;
                        while ((_step_id_140 > 0) and (_index_id_139 <= _end_id_141)) or ((_step_id_140 < 0) and (_index_id_139 >= _end_id_141)) {
                            let idx_1 : Int = _index_id_139;
                            Adjoint SWAP(y[idx_1], qs[idx_1]);
                            _index_id_139 += _step_id_140;
                        }

                    }

                }

                let _generated_ident_142 : Unit = {
                    let _range_2 : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_143 : Range = _range_2.Start + (((_range_2.End - _range_2.Start) / _range_2.Step) * _range_2.Step)..(-_range_2.Step).._range_2.Start;
                        mutable _index_id_144 : Int = _range_id_143.Start;
                        let _step_id_145 : Int = _range_id_143.Step;
                        let _end_id_146 : Int = _range_id_143.End;
                        while ((_step_id_145 > 0) and (_index_id_144 <= _end_id_146)) or ((_step_id_145 < 0) and (_index_id_144 >= _end_id_146)) {
                            let idx_2 : Int = _index_id_144;
                            let shiftedC_1 : Int = (c <<< idx_2) % modulus;
                            Controlled Adjoint ModularAddConstant([y[idx_2]], (modulus, shiftedC_1, qs));
                            _index_id_144 += _step_id_145;
                        }

                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_142
            }
            controlled (ctls, ...) {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                {
                    let _range_id_147 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_148 : Int = _range_id_147.Start;
                    let _step_id_149 : Int = _range_id_147.Step;
                    let _end_id_150 : Int = _range_id_147.End;
                    while ((_step_id_149 > 0) and (_index_id_148 <= _end_id_150)) or ((_step_id_149 < 0) and (_index_id_148 >= _end_id_150)) {
                        let idx : Int = _index_id_148;
                        let shiftedC : Int = (c <<< idx) % modulus;
                        Controlled Controlled ModularAddConstant(ctls, ([y[idx]], (modulus, shiftedC, qs)));
                        _index_id_148 += _step_id_149;
                    }

                }

                {
                    let _range_id_151 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_152 : Int = _range_id_151.Start;
                    let _step_id_153 : Int = _range_id_151.Step;
                    let _end_id_154 : Int = _range_id_151.End;
                    while ((_step_id_153 > 0) and (_index_id_152 <= _end_id_154)) or ((_step_id_153 < 0) and (_index_id_152 >= _end_id_154)) {
                        let idx_1 : Int = _index_id_152;
                        Controlled SWAP(ctls, (y[idx_1], qs[idx_1]));
                        _index_id_152 += _step_id_153;
                    }

                }

                let invC : Int = InverseModI(c, modulus);
                let _generated_ident_155 : Unit = {
                    let _range_id_156 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_157 : Int = _range_id_156.Start;
                    let _step_id_158 : Int = _range_id_156.Step;
                    let _end_id_159 : Int = _range_id_156.End;
                    while ((_step_id_158 > 0) and (_index_id_157 <= _end_id_159)) or ((_step_id_158 < 0) and (_index_id_157 >= _end_id_159)) {
                        let idx_2 : Int = _index_id_157;
                        let shiftedC_1 : Int = (invC <<< idx_2) % modulus;
                        Controlled Controlled ModularAddConstant(ctls, ([y[idx_2]], (modulus, modulus - shiftedC_1, qs)));
                        _index_id_157 += _step_id_158;
                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_155
            }
            controlled adjoint (ctls, ...) {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                let invC : Int = InverseModI(c, modulus);
                {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_160 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                        mutable _index_id_161 : Int = _range_id_160.Start;
                        let _step_id_162 : Int = _range_id_160.Step;
                        let _end_id_163 : Int = _range_id_160.End;
                        while ((_step_id_162 > 0) and (_index_id_161 <= _end_id_163)) or ((_step_id_162 < 0) and (_index_id_161 >= _end_id_163)) {
                            let idx : Int = _index_id_161;
                            let shiftedC : Int = (invC <<< idx) % modulus;
                            Controlled Controlled Adjoint ModularAddConstant(ctls, ([y[idx]], (modulus, modulus - shiftedC, qs)));
                            _index_id_161 += _step_id_162;
                        }

                    }

                }

                {
                    let _range_1 : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_164 : Range = _range_1.Start + (((_range_1.End - _range_1.Start) / _range_1.Step) * _range_1.Step)..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_165 : Int = _range_id_164.Start;
                        let _step_id_166 : Int = _range_id_164.Step;
                        let _end_id_167 : Int = _range_id_164.End;
                        while ((_step_id_166 > 0) and (_index_id_165 <= _end_id_167)) or ((_step_id_166 < 0) and (_index_id_165 >= _end_id_167)) {
                            let idx_1 : Int = _index_id_165;
                            Controlled Adjoint SWAP(ctls, (y[idx_1], qs[idx_1]));
                            _index_id_165 += _step_id_166;
                        }

                    }

                }

                let _generated_ident_168 : Unit = {
                    let _range_2 : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_169 : Range = _range_2.Start + (((_range_2.End - _range_2.Start) / _range_2.Step) * _range_2.Step)..(-_range_2.Step).._range_2.Start;
                        mutable _index_id_170 : Int = _range_id_169.Start;
                        let _step_id_171 : Int = _range_id_169.Step;
                        let _end_id_172 : Int = _range_id_169.End;
                        while ((_step_id_171 > 0) and (_index_id_170 <= _end_id_172)) or ((_step_id_171 < 0) and (_index_id_170 >= _end_id_172)) {
                            let idx_2 : Int = _index_id_170;
                            let shiftedC_1 : Int = (c <<< idx_2) % modulus;
                            Controlled Controlled Adjoint ModularAddConstant(ctls, ([y[idx_2]], (modulus, shiftedC_1, qs)));
                            _index_id_170 += _step_id_171;
                        }

                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_168
            }
        }
        operation ModularAddConstant(modulus : Int, c : Int, y : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                Controlled ModularAddConstant([], (modulus, c, y));
            }
            adjoint ... {
                Controlled Adjoint ModularAddConstant([], (modulus, c, y));
            }
            controlled (ctrls, ...) {
                let __cond_0 : Bool = Length(ctrls) >= 2;
                if __cond_0 {
                    let control : Qubit = __quantum__rt__qubit_allocate();
                    let _generated_ident_173 : Unit = {
                        {
                            Controlled X(ctrls, control);
                        }

                        let _apply_res : Unit = {
                            Controlled ModularAddConstant([control], (modulus, c, y));
                        };
                        {
                            Controlled Adjoint X(ctrls, control);
                        }

                        _apply_res
                    };
                    __quantum__rt__qubit_release(control);
                    _generated_ident_173
                } else {
                    let carry : Qubit = __quantum__rt__qubit_allocate();
                    Controlled IncByI(ctrls, (c, y + [carry]));
                    Controlled Adjoint IncByI(ctrls, (modulus, y + [carry]));
                    Controlled IncByI([carry], (modulus, y));
                    Controlled ApplyIfLessOrEqualL_Qubit__AdjCtl__X_(ctrls, (IntAsBigInt(c), y, carry));
                    __quantum__rt__qubit_release(carry);
                }

            }
            controlled adjoint (ctrls, ...) {
                let __cond_0 : Bool = Length(ctrls) >= 2;
                if __cond_0 {
                    let control : Qubit = __quantum__rt__qubit_allocate();
                    let _generated_ident_174 : Unit = {
                        {
                            Controlled X(ctrls, control);
                        }

                        let _apply_res : Unit = {
                            Controlled Adjoint ModularAddConstant([control], (modulus, c, y));
                        };
                        {
                            Controlled Adjoint X(ctrls, control);
                        }

                        _apply_res
                    };
                    __quantum__rt__qubit_release(control);
                    _generated_ident_174
                } else {
                    let carry : Qubit = __quantum__rt__qubit_allocate();
                    Controlled Adjoint ApplyIfLessOrEqualL_Qubit__AdjCtl__X_(ctrls, (IntAsBigInt(c), y, carry));
                    Controlled Adjoint IncByI([carry], (modulus, y));
                    Controlled IncByI(ctrls, (modulus, y + [carry]));
                    Controlled Adjoint IncByI(ctrls, (c, y + [carry]));
                    __quantum__rt__qubit_release(carry);
                }

            }
        }
        operation ApplyIfLessOrEqualL_Qubit__AdjCtl__X_(c : BigInt, x : Qubit[], target : Qubit) : Unit is Adj + Ctl {
            body ... {
                ApplyActionIfGreaterThanOrEqualConstant_Qubit__AdjCtl__X_(false, c, x, target);
            }
            adjoint ... {
                Adjoint ApplyActionIfGreaterThanOrEqualConstant_Qubit__AdjCtl__X_(false, c, x, target);
            }
            controlled (ctls, ...) {
                Controlled ApplyActionIfGreaterThanOrEqualConstant_Qubit__AdjCtl__X_(ctls, (false, c, x, target));
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint ApplyActionIfGreaterThanOrEqualConstant_Qubit__AdjCtl__X_(ctls, (false, c, x, target));
            }
        }
        operation ApplyActionIfGreaterThanOrEqualConstant_Qubit__AdjCtl__X_(invertControl : Bool, c : BigInt, x : Qubit[], target : Qubit) : Unit is Adj + Ctl {
            body ... {
                let bitWidth : Int = Length(x);
                if c == 0L {
                    if (not invertControl) {
                        X(target);
                    }

                } else if c >= (2L^bitWidth) {
                    if invertControl {
                        X(target);
                    }

                } else {
                    let l : Int = TrailingZeroCountL(c);
                    let cNormalized : BigInt = c >>> l;
                    let xNormalized : Qubit[] = x[l...];
                    let bitWidthNormalized : Int = Length(xNormalized);
                    let qs : Qubit[] = AllocateQubitArray(bitWidthNormalized - 1);
                    let cs1 : Qubit[] = if IsEmpty_Qubit_(qs) {
                        []
                    } else {
                        [Head_Qubit_(xNormalized)] + Most_Qubit_(qs)
                    };
                    Fact(Length(cs1) == Length(qs), $"Arrays should be of the same length.");
                    let _generated_ident_175 : Unit = {
                        {
                            {
                                let _range_id_176 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_177 : Int = _range_id_176.Start;
                                let _step_id_178 : Int = _range_id_176.Step;
                                let _end_id_179 : Int = _range_id_176.End;
                                while ((_step_id_178 > 0) and (_index_id_177 <= _end_id_179)) or ((_step_id_178 < 0) and (_index_id_177 >= _end_id_179)) {
                                    let i : Int = _index_id_177;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_177 += _step_id_178;
                                }

                            }

                        }

                        let _apply_res : Unit = {
                            let control : Qubit = if IsEmpty_Qubit_(qs) {
                                Tail_Qubit_(x)
                            } else {
                                Tail_Qubit_(qs)
                            };
                            {
                                {
                                    if invertControl {
                                        X(control);
                                    }

                                }

                                let _apply_res_1 : Unit = {
                                    Controlled X([control], target);
                                };
                                {
                                    if invertControl {
                                        Adjoint X(control);
                                    }

                                }

                                _apply_res_1
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(cs1) - 1;
                                {
                                    let _range_id_180 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                                    mutable _index_id_181 : Int = _range_id_180.Start;
                                    let _step_id_182 : Int = _range_id_180.Step;
                                    let _end_id_183 : Int = _range_id_180.End;
                                    while ((_step_id_182 > 0) and (_index_id_181 <= _end_id_183)) or ((_step_id_182 < 0) and (_index_id_181 >= _end_id_183)) {
                                        let i_1 : Int = _index_id_181;
                                        let op : ((Qubit, Qubit, Qubit) => Unit is Adj) = if (cNormalized &&& (1L <<< (i_1 + 1))) != 0L {
                                            AND
                                        } else {
                                            ApplyOrAssuming0Target
                                        };
                                        if (cNormalized &&& (1L <<< (i_1 + 1))) != 0L {
                                            Adjoint AND(cs1[i_1], xNormalized[i_1 + 1], qs[i_1])
                                        } else {
                                            Adjoint ApplyOrAssuming0Target(cs1[i_1], xNormalized[i_1 + 1], qs[i_1])
                                        };
                                        _index_id_181 += _step_id_182;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_175
                }

            }
            adjoint ... {
                let bitWidth : Int = Length(x);
                if c == 0L {
                    if (not invertControl) {
                        Adjoint X(target);
                    }

                } else if c >= (2L^bitWidth) {
                    if invertControl {
                        Adjoint X(target);
                    }

                } else {
                    let l : Int = TrailingZeroCountL(c);
                    let cNormalized : BigInt = c >>> l;
                    let xNormalized : Qubit[] = x[l...];
                    let bitWidthNormalized : Int = Length(xNormalized);
                    let qs : Qubit[] = AllocateQubitArray(bitWidthNormalized - 1);
                    let cs1 : Qubit[] = if IsEmpty_Qubit_(qs) {
                        []
                    } else {
                        [Head_Qubit_(xNormalized)] + Most_Qubit_(qs)
                    };
                    Fact(Length(cs1) == Length(qs), $"Arrays should be of the same length.");
                    let _generated_ident_184 : Unit = {
                        {
                            {
                                let _range_id_185 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_186 : Int = _range_id_185.Start;
                                let _step_id_187 : Int = _range_id_185.Step;
                                let _end_id_188 : Int = _range_id_185.End;
                                while ((_step_id_187 > 0) and (_index_id_186 <= _end_id_188)) or ((_step_id_187 < 0) and (_index_id_186 >= _end_id_188)) {
                                    let i : Int = _index_id_186;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_186 += _step_id_187;
                                }

                            }

                        }

                        let _apply_res : Unit = {
                            let control : Qubit = if IsEmpty_Qubit_(qs) {
                                Tail_Qubit_(x)
                            } else {
                                Tail_Qubit_(qs)
                            };
                            {
                                {
                                    if invertControl {
                                        X(control);
                                    }

                                }

                                let _apply_res_1 : Unit = {
                                    Controlled Adjoint X([control], target);
                                };
                                {
                                    if invertControl {
                                        Adjoint X(control);
                                    }

                                }

                                _apply_res_1
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(cs1) - 1;
                                {
                                    let _range_id_189 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                                    mutable _index_id_190 : Int = _range_id_189.Start;
                                    let _step_id_191 : Int = _range_id_189.Step;
                                    let _end_id_192 : Int = _range_id_189.End;
                                    while ((_step_id_191 > 0) and (_index_id_190 <= _end_id_192)) or ((_step_id_191 < 0) and (_index_id_190 >= _end_id_192)) {
                                        let i_1 : Int = _index_id_190;
                                        let op : ((Qubit, Qubit, Qubit) => Unit is Adj) = if (cNormalized &&& (1L <<< (i_1 + 1))) != 0L {
                                            AND
                                        } else {
                                            ApplyOrAssuming0Target
                                        };
                                        if (cNormalized &&& (1L <<< (i_1 + 1))) != 0L {
                                            Adjoint AND(cs1[i_1], xNormalized[i_1 + 1], qs[i_1])
                                        } else {
                                            Adjoint ApplyOrAssuming0Target(cs1[i_1], xNormalized[i_1 + 1], qs[i_1])
                                        };
                                        _index_id_190 += _step_id_191;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_184
                }

            }
            controlled (ctls, ...) {
                let bitWidth : Int = Length(x);
                if c == 0L {
                    if (not invertControl) {
                        Controlled X(ctls, target);
                    }

                } else if c >= (2L^bitWidth) {
                    if invertControl {
                        Controlled X(ctls, target);
                    }

                } else {
                    let l : Int = TrailingZeroCountL(c);
                    let cNormalized : BigInt = c >>> l;
                    let xNormalized : Qubit[] = x[l...];
                    let bitWidthNormalized : Int = Length(xNormalized);
                    let qs : Qubit[] = AllocateQubitArray(bitWidthNormalized - 1);
                    let cs1 : Qubit[] = if IsEmpty_Qubit_(qs) {
                        []
                    } else {
                        [Head_Qubit_(xNormalized)] + Most_Qubit_(qs)
                    };
                    Fact(Length(cs1) == Length(qs), $"Arrays should be of the same length.");
                    let _generated_ident_193 : Unit = {
                        {
                            {
                                let _range_id_194 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_195 : Int = _range_id_194.Start;
                                let _step_id_196 : Int = _range_id_194.Step;
                                let _end_id_197 : Int = _range_id_194.End;
                                while ((_step_id_196 > 0) and (_index_id_195 <= _end_id_197)) or ((_step_id_196 < 0) and (_index_id_195 >= _end_id_197)) {
                                    let i : Int = _index_id_195;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_195 += _step_id_196;
                                }

                            }

                        }

                        let _apply_res : Unit = {
                            let control : Qubit = if IsEmpty_Qubit_(qs) {
                                Tail_Qubit_(x)
                            } else {
                                Tail_Qubit_(qs)
                            };
                            {
                                {
                                    if invertControl {
                                        X(control);
                                    }

                                }

                                let _apply_res_1 : Unit = {
                                    Controlled Controlled X(ctls, ([control], target));
                                };
                                {
                                    if invertControl {
                                        Adjoint X(control);
                                    }

                                }

                                _apply_res_1
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(cs1) - 1;
                                {
                                    let _range_id_198 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                                    mutable _index_id_199 : Int = _range_id_198.Start;
                                    let _step_id_200 : Int = _range_id_198.Step;
                                    let _end_id_201 : Int = _range_id_198.End;
                                    while ((_step_id_200 > 0) and (_index_id_199 <= _end_id_201)) or ((_step_id_200 < 0) and (_index_id_199 >= _end_id_201)) {
                                        let i_1 : Int = _index_id_199;
                                        let op : ((Qubit, Qubit, Qubit) => Unit is Adj) = if (cNormalized &&& (1L <<< (i_1 + 1))) != 0L {
                                            AND
                                        } else {
                                            ApplyOrAssuming0Target
                                        };
                                        if (cNormalized &&& (1L <<< (i_1 + 1))) != 0L {
                                            Adjoint AND(cs1[i_1], xNormalized[i_1 + 1], qs[i_1])
                                        } else {
                                            Adjoint ApplyOrAssuming0Target(cs1[i_1], xNormalized[i_1 + 1], qs[i_1])
                                        };
                                        _index_id_199 += _step_id_200;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_193
                }

            }
            controlled adjoint (ctls, ...) {
                let bitWidth : Int = Length(x);
                if c == 0L {
                    if (not invertControl) {
                        Controlled Adjoint X(ctls, target);
                    }

                } else if c >= (2L^bitWidth) {
                    if invertControl {
                        Controlled Adjoint X(ctls, target);
                    }

                } else {
                    let l : Int = TrailingZeroCountL(c);
                    let cNormalized : BigInt = c >>> l;
                    let xNormalized : Qubit[] = x[l...];
                    let bitWidthNormalized : Int = Length(xNormalized);
                    let qs : Qubit[] = AllocateQubitArray(bitWidthNormalized - 1);
                    let cs1 : Qubit[] = if IsEmpty_Qubit_(qs) {
                        []
                    } else {
                        [Head_Qubit_(xNormalized)] + Most_Qubit_(qs)
                    };
                    Fact(Length(cs1) == Length(qs), $"Arrays should be of the same length.");
                    let _generated_ident_202 : Unit = {
                        {
                            {
                                let _range_id_203 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_204 : Int = _range_id_203.Start;
                                let _step_id_205 : Int = _range_id_203.Step;
                                let _end_id_206 : Int = _range_id_203.End;
                                while ((_step_id_205 > 0) and (_index_id_204 <= _end_id_206)) or ((_step_id_205 < 0) and (_index_id_204 >= _end_id_206)) {
                                    let i : Int = _index_id_204;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_204 += _step_id_205;
                                }

                            }

                        }

                        let _apply_res : Unit = {
                            let control : Qubit = if IsEmpty_Qubit_(qs) {
                                Tail_Qubit_(x)
                            } else {
                                Tail_Qubit_(qs)
                            };
                            {
                                {
                                    if invertControl {
                                        X(control);
                                    }

                                }

                                let _apply_res_1 : Unit = {
                                    Controlled Controlled Adjoint X(ctls, ([control], target));
                                };
                                {
                                    if invertControl {
                                        Adjoint X(control);
                                    }

                                }

                                _apply_res_1
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(cs1) - 1;
                                {
                                    let _range_id_207 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                                    mutable _index_id_208 : Int = _range_id_207.Start;
                                    let _step_id_209 : Int = _range_id_207.Step;
                                    let _end_id_210 : Int = _range_id_207.End;
                                    while ((_step_id_209 > 0) and (_index_id_208 <= _end_id_210)) or ((_step_id_209 < 0) and (_index_id_208 >= _end_id_210)) {
                                        let i_1 : Int = _index_id_208;
                                        let op : ((Qubit, Qubit, Qubit) => Unit is Adj) = if (cNormalized &&& (1L <<< (i_1 + 1))) != 0L {
                                            AND
                                        } else {
                                            ApplyOrAssuming0Target
                                        };
                                        if (cNormalized &&& (1L <<< (i_1 + 1))) != 0L {
                                            Adjoint AND(cs1[i_1], xNormalized[i_1 + 1], qs[i_1])
                                        } else {
                                            Adjoint ApplyOrAssuming0Target(cs1[i_1], xNormalized[i_1 + 1], qs[i_1])
                                        };
                                        _index_id_208 += _step_id_209;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_202
                }

            }
        }
        // entry
        Main()"#]].assert_eq(&rendered);
}
