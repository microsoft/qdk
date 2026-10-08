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
                let _range_id_219 : Range = 0..size - 1;
                mutable _index_id_222 : Int = _range_id_219.Start;
                let _step_id_227 : Int = _range_id_219.Step;
                let _end_id_232 : Int = _range_id_219.End;
                while ((_step_id_227 > 0) and (_index_id_222 <= _end_id_232)) or ((_step_id_227 < 0) and (_index_id_222 >= _end_id_232)) {
                    let _ : Int = _index_id_222;
                    qs += [__quantum__rt__qubit_allocate()];
                    _index_id_222 += _step_id_227;
                }

            }

            qs
        }
        operation ReleaseQubitArray(qs : Qubit[]) : Unit {
            {
                let _array_id_305 : Qubit[] = qs;
                let _len_id_309 : Int = Length(_array_id_305);
                mutable _index_id_314 : Int = 0;
                while _index_id_314 < _len_id_309 {
                    let q : Qubit = _array_id_305[_index_id_314];
                    __quantum__rt__qubit_release(q);
                    _index_id_314 += 1;
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
                    let _array_id_48419 : Qubit[] = target;
                    let _len_id_48423 : Int = Length(_array_id_48419);
                    mutable _index_id_48428 : Int = 0;
                    while _index_id_48428 < _len_id_48423 {
                        let q : Qubit = _array_id_48419[_index_id_48428];
                        if (runningValue &&& 1) != 0 {
                            X(q);
                        }

                        runningValue >>>= 1;
                        _index_id_48428 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            adjoint ... {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_48447 : Qubit[] = target;
                    let _len_id_48451 : Int = Length(_array_id_48447);
                    mutable _index_id_48456 : Int = 0;
                    while _index_id_48456 < _len_id_48451 {
                        let q : Qubit = _array_id_48447[_index_id_48456];
                        if (runningValue &&& 1) != 0 {
                            X(q);
                        }

                        runningValue >>>= 1;
                        _index_id_48456 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            controlled (ctls, ...) {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_48475 : Qubit[] = target;
                    let _len_id_48479 : Int = Length(_array_id_48475);
                    mutable _index_id_48484 : Int = 0;
                    while _index_id_48484 < _len_id_48479 {
                        let q : Qubit = _array_id_48475[_index_id_48484];
                        if (runningValue &&& 1) != 0 {
                            Controlled X(ctls, q);
                        }

                        runningValue >>>= 1;
                        _index_id_48484 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            controlled adjoint (ctls, ...) {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_48503 : Qubit[] = target;
                    let _len_id_48507 : Int = Length(_array_id_48503);
                    mutable _index_id_48512 : Int = 0;
                    while _index_id_48512 < _len_id_48507 {
                        let q : Qubit = _array_id_48503[_index_id_48512];
                        if (runningValue &&& 1) != 0 {
                            Controlled X(ctls, q);
                        }

                        runningValue >>>= 1;
                        _index_id_48512 += 1;
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
                GlobalPhase(theta);
            }
            adjoint ... {
                ApplyGlobalPhase((-theta));
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                if __cond_0 {
                    GlobalPhase(theta);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        let control : Qubit = ctls[0];
                        Rz(theta, ctls[0]);
                        GlobalPhase(theta / 2.);
                    } else {
                        __cond_2 = Length(ctls) == 2;
                        if __cond_2 {
                            Rz(theta / 2., ctls[1]);
                            CRz(ctls[1], theta, ctls[0]);
                            GlobalPhase(theta / 4.);
                        } else {
                            let remainingControls : Qubit[] = ctls[1...];
                            let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 2);
                            let combinedControl : Qubit = aux[Length(aux) - 1];
                            let _generated_ident_54746 : Unit = {
                                {
                                    CollectControls(remainingControls, aux, 0);
                                    AdjustForSingleControl(remainingControls, aux);
                                }

                                let _apply_res : Unit = {
                                    Controlled ApplyGlobalPhase([ctls[0], combinedControl], theta);
                                };
                                {
                                    Adjoint AdjustForSingleControl(remainingControls, aux);
                                    Adjoint CollectControls(remainingControls, aux, 0);
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_54746
                        }

                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled ApplyGlobalPhase(ctls, (-theta));
            }
        }
        operation GlobalPhase(theta : Double) : Unit {
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
                    let _range_id_49587 : Range = 0..2..Length(ctls) - 2;
                    mutable _index_id_49590 : Int = _range_id_49587.Start;
                    let _step_id_49595 : Int = _range_id_49587.Step;
                    let _end_id_49600 : Int = _range_id_49587.End;
                    while ((_step_id_49595 > 0) and (_index_id_49590 <= _end_id_49600)) or ((_step_id_49595 < 0) and (_index_id_49590 >= _end_id_49600)) {
                        let i : Int = _index_id_49590;
                        CCNOT(ctls[i], ctls[i + 1], aux[i / 2]);
                        _index_id_49590 += _step_id_49595;
                    }

                }

                {
                    let _range_id_49630 : Range = 0..((Length(ctls) / 2) - 2) - adjustment;
                    mutable _index_id_49633 : Int = _range_id_49630.Start;
                    let _step_id_49638 : Int = _range_id_49630.Step;
                    let _end_id_49643 : Int = _range_id_49630.End;
                    while ((_step_id_49638 > 0) and (_index_id_49633 <= _end_id_49643)) or ((_step_id_49638 < 0) and (_index_id_49633 >= _end_id_49643)) {
                        let i_1 : Int = _index_id_49633;
                        CCNOT(aux[i_1 * 2], aux[(i_1 * 2) + 1], aux[i_1 + (Length(ctls) / 2)]);
                        _index_id_49633 += _step_id_49638;
                    }

                }

            }
            adjoint ... {
                {
                    let _range : Range = 0..((Length(ctls) / 2) - 2) - adjustment;
                    {
                        let _range_id_49673 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_49676 : Int = _range_id_49673.Start;
                        let _step_id_49681 : Int = _range_id_49673.Step;
                        let _end_id_49686 : Int = _range_id_49673.End;
                        while ((_step_id_49681 > 0) and (_index_id_49676 <= _end_id_49686)) or ((_step_id_49681 < 0) and (_index_id_49676 >= _end_id_49686)) {
                            let i : Int = _index_id_49676;
                            Adjoint CCNOT(aux[i * 2], aux[(i * 2) + 1], aux[i + (Length(ctls) / 2)]);
                            _index_id_49676 += _step_id_49681;
                        }

                    }

                }

                {
                    let _range_1 : Range = 0..2..Length(ctls) - 2;
                    {
                        let _range_id_49716 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_49719 : Int = _range_id_49716.Start;
                        let _step_id_49724 : Int = _range_id_49716.Step;
                        let _end_id_49729 : Int = _range_id_49716.End;
                        while ((_step_id_49724 > 0) and (_index_id_49719 <= _end_id_49729)) or ((_step_id_49724 < 0) and (_index_id_49719 >= _end_id_49729)) {
                            let i_1 : Int = _index_id_49719;
                            Adjoint CCNOT(ctls[i_1], ctls[i_1 + 1], aux[i_1 / 2]);
                            _index_id_49719 += _step_id_49724;
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
                            let _generated_ident_54760 : Unit = {
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
                            _generated_ident_54760
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
                            let _generated_ident_54774 : Unit = {
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
                            _generated_ident_54774
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
                let _array_id_50088 : Qubit[] = qubits;
                let _len_id_50092 : Int = Length(_array_id_50088);
                mutable _index_id_50097 : Int = 0;
                while _index_id_50097 < _len_id_50092 {
                    let q : Qubit = _array_id_50088[_index_id_50097];
                    Reset(q);
                    _index_id_50097 += 1;
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
                        let _generated_ident_54830 : Unit = {
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
                        _generated_ident_54830
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
                            let _generated_ident_54858 : Unit = {
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
                            _generated_ident_54858
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
                            let _generated_ident_54872 : Unit = {
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
                            _generated_ident_54872
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
                        let _generated_ident_54914 : Unit = {
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
                        _generated_ident_54914
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
                        let _generated_ident_54928 : Unit = {
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
                        _generated_ident_54928
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
                            let _generated_ident_54942 : Unit = {
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
                            _generated_ident_54942
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
                            let _generated_ident_54956 : Unit = {
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
                            _generated_ident_54956
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
                    let _range_id_51944 : Range = 1..Length(xs) - 1;
                    mutable _index_id_51947 : Int = _range_id_51944.Start;
                    let _step_id_51952 : Int = _range_id_51944.Step;
                    let _end_id_51957 : Int = _range_id_51944.End;
                    while ((_step_id_51952 > 0) and (_index_id_51947 <= _end_id_51957)) or ((_step_id_51952 < 0) and (_index_id_51947 >= _end_id_51957)) {
                        let i : Int = _index_id_51947;
                        CNOT(xs[i], ys[i]);
                        _index_id_51947 += _step_id_51952;
                    }

                }

                {
                    let _range_id_51987 : Range = Length(xs) - 2..(-1)..1;
                    mutable _index_id_51990 : Int = _range_id_51987.Start;
                    let _step_id_51995 : Int = _range_id_51987.Step;
                    let _end_id_52000 : Int = _range_id_51987.End;
                    while ((_step_id_51995 > 0) and (_index_id_51990 <= _end_id_52000)) or ((_step_id_51995 < 0) and (_index_id_51990 >= _end_id_52000)) {
                        let i_1 : Int = _index_id_51990;
                        CNOT(xs[i_1], xs[i_1 + 1]);
                        _index_id_51990 += _step_id_51995;
                    }

                }

            }
            adjoint ... {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range : Range = Length(xs) - 2..(-1)..1;
                    {
                        let _range_id_52030 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_52033 : Int = _range_id_52030.Start;
                        let _step_id_52038 : Int = _range_id_52030.Step;
                        let _end_id_52043 : Int = _range_id_52030.End;
                        while ((_step_id_52038 > 0) and (_index_id_52033 <= _end_id_52043)) or ((_step_id_52038 < 0) and (_index_id_52033 >= _end_id_52043)) {
                            let i : Int = _index_id_52033;
                            Adjoint CNOT(xs[i], xs[i + 1]);
                            _index_id_52033 += _step_id_52038;
                        }

                    }

                }

                {
                    let _range_1 : Range = 1..Length(xs) - 1;
                    {
                        let _range_id_52073 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_52076 : Int = _range_id_52073.Start;
                        let _step_id_52081 : Int = _range_id_52073.Step;
                        let _end_id_52086 : Int = _range_id_52073.End;
                        while ((_step_id_52081 > 0) and (_index_id_52076 <= _end_id_52086)) or ((_step_id_52081 < 0) and (_index_id_52076 >= _end_id_52086)) {
                            let i_1 : Int = _index_id_52076;
                            Adjoint CNOT(xs[i_1], ys[i_1]);
                            _index_id_52076 += _step_id_52081;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range_id_52116 : Range = 1..Length(xs) - 1;
                    mutable _index_id_52119 : Int = _range_id_52116.Start;
                    let _step_id_52124 : Int = _range_id_52116.Step;
                    let _end_id_52129 : Int = _range_id_52116.End;
                    while ((_step_id_52124 > 0) and (_index_id_52119 <= _end_id_52129)) or ((_step_id_52124 < 0) and (_index_id_52119 >= _end_id_52129)) {
                        let i : Int = _index_id_52119;
                        Controlled CNOT(ctls, (xs[i], ys[i]));
                        _index_id_52119 += _step_id_52124;
                    }

                }

                {
                    let _range_id_52159 : Range = Length(xs) - 2..(-1)..1;
                    mutable _index_id_52162 : Int = _range_id_52159.Start;
                    let _step_id_52167 : Int = _range_id_52159.Step;
                    let _end_id_52172 : Int = _range_id_52159.End;
                    while ((_step_id_52167 > 0) and (_index_id_52162 <= _end_id_52172)) or ((_step_id_52167 < 0) and (_index_id_52162 >= _end_id_52172)) {
                        let i_1 : Int = _index_id_52162;
                        Controlled CNOT(ctls, (xs[i_1], xs[i_1 + 1]));
                        _index_id_52162 += _step_id_52167;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range : Range = Length(xs) - 2..(-1)..1;
                    {
                        let _range_id_52202 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_52205 : Int = _range_id_52202.Start;
                        let _step_id_52210 : Int = _range_id_52202.Step;
                        let _end_id_52215 : Int = _range_id_52202.End;
                        while ((_step_id_52210 > 0) and (_index_id_52205 <= _end_id_52215)) or ((_step_id_52210 < 0) and (_index_id_52205 >= _end_id_52215)) {
                            let i : Int = _index_id_52205;
                            Controlled Adjoint CNOT(ctls, (xs[i], xs[i + 1]));
                            _index_id_52205 += _step_id_52210;
                        }

                    }

                }

                {
                    let _range_1 : Range = 1..Length(xs) - 1;
                    {
                        let _range_id_52245 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_52248 : Int = _range_id_52245.Start;
                        let _step_id_52253 : Int = _range_id_52245.Step;
                        let _end_id_52258 : Int = _range_id_52245.End;
                        while ((_step_id_52253 > 0) and (_index_id_52248 <= _end_id_52258)) or ((_step_id_52253 < 0) and (_index_id_52248 >= _end_id_52258)) {
                            let i_1 : Int = _index_id_52248;
                            Controlled Adjoint CNOT(ctls, (xs[i_1], ys[i_1]));
                            _index_id_52248 += _step_id_52253;
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
                    let _range_id_52288 : Range = 0..Length(xs) - 2;
                    mutable _index_id_52291 : Int = _range_id_52288.Start;
                    let _step_id_52296 : Int = _range_id_52288.Step;
                    let _end_id_52301 : Int = _range_id_52288.End;
                    while ((_step_id_52296 > 0) and (_index_id_52291 <= _end_id_52301)) or ((_step_id_52296 < 0) and (_index_id_52291 >= _end_id_52301)) {
                        let idx : Int = _index_id_52291;
                        CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                        _index_id_52291 += _step_id_52296;
                    }

                }

                {
                    let _range_id_52331 : Range = Length(xs) - 1..(-1)..1;
                    mutable _index_id_52334 : Int = _range_id_52331.Start;
                    let _step_id_52339 : Int = _range_id_52331.Step;
                    let _end_id_52344 : Int = _range_id_52331.End;
                    while ((_step_id_52339 > 0) and (_index_id_52334 <= _end_id_52344)) or ((_step_id_52339 < 0) and (_index_id_52334 >= _end_id_52344)) {
                        let idx_1 : Int = _index_id_52334;
                        Controlled CNOT(controls, (xs[idx_1], ys[idx_1]));
                        CCNOT(xs[idx_1 - 1], ys[idx_1 - 1], xs[idx_1]);
                        _index_id_52334 += _step_id_52339;
                    }

                }

            }
            controlled adjoint (controls, ...) {
                Fact(Length(xs) == Length(ys), $"Input registers must have the same number of qubits.");
                {
                    let _range : Range = Length(xs) - 1..(-1)..1;
                    {
                        let _range_id_52374 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_52377 : Int = _range_id_52374.Start;
                        let _step_id_52382 : Int = _range_id_52374.Step;
                        let _end_id_52387 : Int = _range_id_52374.End;
                        while ((_step_id_52382 > 0) and (_index_id_52377 <= _end_id_52387)) or ((_step_id_52382 < 0) and (_index_id_52377 >= _end_id_52387)) {
                            let idx : Int = _index_id_52377;
                            Adjoint CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                            Adjoint Controlled CNOT(controls, (xs[idx], ys[idx]));
                            _index_id_52377 += _step_id_52382;
                        }

                    }

                }

                {
                    let _range_1 : Range = 0..Length(xs) - 2;
                    {
                        let _range_id_52417 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_52420 : Int = _range_id_52417.Start;
                        let _step_id_52425 : Int = _range_id_52417.Step;
                        let _end_id_52430 : Int = _range_id_52417.End;
                        while ((_step_id_52425 > 0) and (_index_id_52420 <= _end_id_52430)) or ((_step_id_52425 < 0) and (_index_id_52420 >= _end_id_52430)) {
                            let idx_1 : Int = _index_id_52420;
                            Adjoint CCNOT(xs[idx_1], ys[idx_1], xs[idx_1 + 1]);
                            _index_id_52420 += _step_id_52425;
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
                    let _range_id_52460 : Range = 0..nQubits - 2;
                    mutable _index_id_52463 : Int = _range_id_52460.Start;
                    let _step_id_52468 : Int = _range_id_52460.Step;
                    let _end_id_52473 : Int = _range_id_52460.End;
                    while ((_step_id_52468 > 0) and (_index_id_52463 <= _end_id_52473)) or ((_step_id_52468 < 0) and (_index_id_52463 >= _end_id_52473)) {
                        let idx : Int = _index_id_52463;
                        CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                        _index_id_52463 += _step_id_52468;
                    }

                }

                Controlled CCNOT(controls, (xs[nQubits - 1], ys[nQubits - 1], ys[nQubits]));
                {
                    let _range_id_52503 : Range = nQubits - 1..(-1)..1;
                    mutable _index_id_52506 : Int = _range_id_52503.Start;
                    let _step_id_52511 : Int = _range_id_52503.Step;
                    let _end_id_52516 : Int = _range_id_52503.End;
                    while ((_step_id_52511 > 0) and (_index_id_52506 <= _end_id_52516)) or ((_step_id_52511 < 0) and (_index_id_52506 >= _end_id_52516)) {
                        let idx_1 : Int = _index_id_52506;
                        Controlled CNOT(controls, (xs[idx_1], ys[idx_1]));
                        CCNOT(xs[idx_1 - 1], ys[idx_1 - 1], xs[idx_1]);
                        _index_id_52506 += _step_id_52511;
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
                        let _range_id_52546 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_52549 : Int = _range_id_52546.Start;
                        let _step_id_52554 : Int = _range_id_52546.Step;
                        let _end_id_52559 : Int = _range_id_52546.End;
                        while ((_step_id_52554 > 0) and (_index_id_52549 <= _end_id_52559)) or ((_step_id_52554 < 0) and (_index_id_52549 >= _end_id_52559)) {
                            let idx : Int = _index_id_52549;
                            Adjoint CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                            Adjoint Controlled CNOT(controls, (xs[idx], ys[idx]));
                            _index_id_52549 += _step_id_52554;
                        }

                    }

                }

                Adjoint Controlled CCNOT(controls, (xs[nQubits - 1], ys[nQubits - 1], ys[nQubits]));
                {
                    let _range_1 : Range = 0..nQubits - 2;
                    {
                        let _range_id_52589 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_52592 : Int = _range_id_52589.Start;
                        let _step_id_52597 : Int = _range_id_52589.Step;
                        let _end_id_52602 : Int = _range_id_52589.End;
                        while ((_step_id_52597 > 0) and (_index_id_52592 <= _end_id_52602)) or ((_step_id_52597 < 0) and (_index_id_52592 >= _end_id_52602)) {
                            let idx_1 : Int = _index_id_52592;
                            Adjoint CCNOT(xs[idx_1], ys[idx_1], xs[idx_1 + 1]);
                            _index_id_52592 += _step_id_52597;
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
                    let _generated_ident_55200 : Unit = {
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
                    _generated_ident_55200
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
                    let _generated_ident_55214 : Unit = {
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
                    _generated_ident_55214
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
                    let _generated_ident_55228 : Unit = {
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
                    _generated_ident_55228
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
                    let _generated_ident_55242 : Unit = {
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
                    _generated_ident_55242
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
                    mutable _continue_cond_1511 : Bool = true;
                    while _continue_cond_1511 {
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

                        _continue_cond_1511 = (not foundFactors);
                        if _continue_cond_1511 {
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
                let _range_id_1528 : Range = bitsPrecision - 1..(-1)..0;
                mutable _index_id_1531 : Int = _range_id_1528.Start;
                let _step_id_1536 : Int = _range_id_1528.Step;
                let _end_id_1541 : Int = _range_id_1528.End;
                while ((_step_id_1536 > 0) and (_index_id_1531 <= _end_id_1541)) or ((_step_id_1536 < 0) and (_index_id_1531 >= _end_id_1541)) {
                    let idx : Int = _index_id_1531;
                    H(c);
                    Controlled ApplyOrderFindingOracle([c], (generator, modulus, 1 <<< idx, eigenstateRegister));
                    R1Frac(frequencyEstimate, (bitsPrecision - 1) - idx, c);
                    H(c);
                    __cond_0 = M(c) == One;
                    if __cond_0 {
                        X(c);
                        frequencyEstimate += 1 <<< ((bitsPrecision - 1) - idx);
                    }

                    _index_id_1531 += _step_id_1536;
                }

            }

            ResetAll(eigenstateRegister);
            Message($"Estimated frequency={frequencyEstimate}");
            {
                let _generated_ident_2097 : Int = frequencyEstimate;
                __quantum__rt__qubit_release(c);
                ReleaseQubitArray(eigenstateRegister);
                {
                    __ret_val = _generated_ident_2097;
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
                    let _range_id_1571 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1574 : Int = _range_id_1571.Start;
                    let _step_id_1579 : Int = _range_id_1571.Step;
                    let _end_id_1584 : Int = _range_id_1571.End;
                    while ((_step_id_1579 > 0) and (_index_id_1574 <= _end_id_1584)) or ((_step_id_1579 < 0) and (_index_id_1574 >= _end_id_1584)) {
                        let idx : Int = _index_id_1574;
                        let shiftedC : Int = (c <<< idx) % modulus;
                        Controlled ModularAddConstant([y[idx]], (modulus, shiftedC, qs));
                        _index_id_1574 += _step_id_1579;
                    }

                }

                {
                    let _range_id_1614 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1617 : Int = _range_id_1614.Start;
                    let _step_id_1622 : Int = _range_id_1614.Step;
                    let _end_id_1627 : Int = _range_id_1614.End;
                    while ((_step_id_1622 > 0) and (_index_id_1617 <= _end_id_1627)) or ((_step_id_1622 < 0) and (_index_id_1617 >= _end_id_1627)) {
                        let idx_1 : Int = _index_id_1617;
                        SWAP(y[idx_1], qs[idx_1]);
                        _index_id_1617 += _step_id_1622;
                    }

                }

                let invC : Int = InverseModI(c, modulus);
                let _generated_ident_2126 : Unit = {
                    let _range_id_1657 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1660 : Int = _range_id_1657.Start;
                    let _step_id_1665 : Int = _range_id_1657.Step;
                    let _end_id_1670 : Int = _range_id_1657.End;
                    while ((_step_id_1665 > 0) and (_index_id_1660 <= _end_id_1670)) or ((_step_id_1665 < 0) and (_index_id_1660 >= _end_id_1670)) {
                        let idx_2 : Int = _index_id_1660;
                        let shiftedC_1 : Int = (invC <<< idx_2) % modulus;
                        Controlled ModularAddConstant([y[idx_2]], (modulus, modulus - shiftedC_1, qs));
                        _index_id_1660 += _step_id_1665;
                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_2126
            }
            adjoint ... {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                let invC : Int = InverseModI(c, modulus);
                {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_1700 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_1703 : Int = _range_id_1700.Start;
                        let _step_id_1708 : Int = _range_id_1700.Step;
                        let _end_id_1713 : Int = _range_id_1700.End;
                        while ((_step_id_1708 > 0) and (_index_id_1703 <= _end_id_1713)) or ((_step_id_1708 < 0) and (_index_id_1703 >= _end_id_1713)) {
                            let idx : Int = _index_id_1703;
                            let shiftedC : Int = (invC <<< idx) % modulus;
                            Controlled Adjoint ModularAddConstant([y[idx]], (modulus, modulus - shiftedC, qs));
                            _index_id_1703 += _step_id_1708;
                        }

                    }

                }

                {
                    let _range_1 : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_1743 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_1746 : Int = _range_id_1743.Start;
                        let _step_id_1751 : Int = _range_id_1743.Step;
                        let _end_id_1756 : Int = _range_id_1743.End;
                        while ((_step_id_1751 > 0) and (_index_id_1746 <= _end_id_1756)) or ((_step_id_1751 < 0) and (_index_id_1746 >= _end_id_1756)) {
                            let idx_1 : Int = _index_id_1746;
                            Adjoint SWAP(y[idx_1], qs[idx_1]);
                            _index_id_1746 += _step_id_1751;
                        }

                    }

                }

                let _generated_ident_2140 : Unit = {
                    let _range_2 : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_1786 : Range = (_range_2.Start + ((((_range_2.End - _range_2.Start) + _range_2.Step) / _range_2.Step) * _range_2.Step)) - _range_2.Step..(-_range_2.Step).._range_2.Start;
                        mutable _index_id_1789 : Int = _range_id_1786.Start;
                        let _step_id_1794 : Int = _range_id_1786.Step;
                        let _end_id_1799 : Int = _range_id_1786.End;
                        while ((_step_id_1794 > 0) and (_index_id_1789 <= _end_id_1799)) or ((_step_id_1794 < 0) and (_index_id_1789 >= _end_id_1799)) {
                            let idx_2 : Int = _index_id_1789;
                            let shiftedC_1 : Int = (c <<< idx_2) % modulus;
                            Controlled Adjoint ModularAddConstant([y[idx_2]], (modulus, shiftedC_1, qs));
                            _index_id_1789 += _step_id_1794;
                        }

                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_2140
            }
            controlled (ctls, ...) {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                {
                    let _range_id_1829 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1832 : Int = _range_id_1829.Start;
                    let _step_id_1837 : Int = _range_id_1829.Step;
                    let _end_id_1842 : Int = _range_id_1829.End;
                    while ((_step_id_1837 > 0) and (_index_id_1832 <= _end_id_1842)) or ((_step_id_1837 < 0) and (_index_id_1832 >= _end_id_1842)) {
                        let idx : Int = _index_id_1832;
                        let shiftedC : Int = (c <<< idx) % modulus;
                        Controlled Controlled ModularAddConstant(ctls, ([y[idx]], (modulus, shiftedC, qs)));
                        _index_id_1832 += _step_id_1837;
                    }

                }

                {
                    let _range_id_1872 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1875 : Int = _range_id_1872.Start;
                    let _step_id_1880 : Int = _range_id_1872.Step;
                    let _end_id_1885 : Int = _range_id_1872.End;
                    while ((_step_id_1880 > 0) and (_index_id_1875 <= _end_id_1885)) or ((_step_id_1880 < 0) and (_index_id_1875 >= _end_id_1885)) {
                        let idx_1 : Int = _index_id_1875;
                        Controlled SWAP(ctls, (y[idx_1], qs[idx_1]));
                        _index_id_1875 += _step_id_1880;
                    }

                }

                let invC : Int = InverseModI(c, modulus);
                let _generated_ident_2154 : Unit = {
                    let _range_id_1915 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1918 : Int = _range_id_1915.Start;
                    let _step_id_1923 : Int = _range_id_1915.Step;
                    let _end_id_1928 : Int = _range_id_1915.End;
                    while ((_step_id_1923 > 0) and (_index_id_1918 <= _end_id_1928)) or ((_step_id_1923 < 0) and (_index_id_1918 >= _end_id_1928)) {
                        let idx_2 : Int = _index_id_1918;
                        let shiftedC_1 : Int = (invC <<< idx_2) % modulus;
                        Controlled Controlled ModularAddConstant(ctls, ([y[idx_2]], (modulus, modulus - shiftedC_1, qs)));
                        _index_id_1918 += _step_id_1923;
                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_2154
            }
            controlled adjoint (ctls, ...) {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                let invC : Int = InverseModI(c, modulus);
                {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_1958 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_1961 : Int = _range_id_1958.Start;
                        let _step_id_1966 : Int = _range_id_1958.Step;
                        let _end_id_1971 : Int = _range_id_1958.End;
                        while ((_step_id_1966 > 0) and (_index_id_1961 <= _end_id_1971)) or ((_step_id_1966 < 0) and (_index_id_1961 >= _end_id_1971)) {
                            let idx : Int = _index_id_1961;
                            let shiftedC : Int = (invC <<< idx) % modulus;
                            Controlled Controlled Adjoint ModularAddConstant(ctls, ([y[idx]], (modulus, modulus - shiftedC, qs)));
                            _index_id_1961 += _step_id_1966;
                        }

                    }

                }

                {
                    let _range_1 : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_2001 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_2004 : Int = _range_id_2001.Start;
                        let _step_id_2009 : Int = _range_id_2001.Step;
                        let _end_id_2014 : Int = _range_id_2001.End;
                        while ((_step_id_2009 > 0) and (_index_id_2004 <= _end_id_2014)) or ((_step_id_2009 < 0) and (_index_id_2004 >= _end_id_2014)) {
                            let idx_1 : Int = _index_id_2004;
                            Controlled Adjoint SWAP(ctls, (y[idx_1], qs[idx_1]));
                            _index_id_2004 += _step_id_2009;
                        }

                    }

                }

                let _generated_ident_2168 : Unit = {
                    let _range_2 : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_2044 : Range = (_range_2.Start + ((((_range_2.End - _range_2.Start) + _range_2.Step) / _range_2.Step) * _range_2.Step)) - _range_2.Step..(-_range_2.Step).._range_2.Start;
                        mutable _index_id_2047 : Int = _range_id_2044.Start;
                        let _step_id_2052 : Int = _range_id_2044.Step;
                        let _end_id_2057 : Int = _range_id_2044.End;
                        while ((_step_id_2052 > 0) and (_index_id_2047 <= _end_id_2057)) or ((_step_id_2052 < 0) and (_index_id_2047 >= _end_id_2057)) {
                            let idx_2 : Int = _index_id_2047;
                            let shiftedC_1 : Int = (c <<< idx_2) % modulus;
                            Controlled Controlled Adjoint ModularAddConstant(ctls, ([y[idx_2]], (modulus, shiftedC_1, qs)));
                            _index_id_2047 += _step_id_2052;
                        }

                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_2168
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
                    let _generated_ident_2182 : Unit = {
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
                    _generated_ident_2182
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
                    let _generated_ident_2205 : Unit = {
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
                    _generated_ident_2205
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
                    let _generated_ident_55371 : Unit = {
                        {
                            {
                                let _range_id_53305 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_53308 : Int = _range_id_53305.Start;
                                let _step_id_53313 : Int = _range_id_53305.Step;
                                let _end_id_53318 : Int = _range_id_53305.End;
                                while ((_step_id_53313 > 0) and (_index_id_53308 <= _end_id_53318)) or ((_step_id_53313 < 0) and (_index_id_53308 >= _end_id_53318)) {
                                    let i : Int = _index_id_53308;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_53308 += _step_id_53313;
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
                                    let _range_id_53348 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                                    mutable _index_id_53351 : Int = _range_id_53348.Start;
                                    let _step_id_53356 : Int = _range_id_53348.Step;
                                    let _end_id_53361 : Int = _range_id_53348.End;
                                    while ((_step_id_53356 > 0) and (_index_id_53351 <= _end_id_53361)) or ((_step_id_53356 < 0) and (_index_id_53351 >= _end_id_53361)) {
                                        let i_1 : Int = _index_id_53351;
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
                                        _index_id_53351 += _step_id_53356;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_55371
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
                    let _generated_ident_55385 : Unit = {
                        {
                            {
                                let _range_id_53391 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_53394 : Int = _range_id_53391.Start;
                                let _step_id_53399 : Int = _range_id_53391.Step;
                                let _end_id_53404 : Int = _range_id_53391.End;
                                while ((_step_id_53399 > 0) and (_index_id_53394 <= _end_id_53404)) or ((_step_id_53399 < 0) and (_index_id_53394 >= _end_id_53404)) {
                                    let i : Int = _index_id_53394;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_53394 += _step_id_53399;
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
                                    let _range_id_53434 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                                    mutable _index_id_53437 : Int = _range_id_53434.Start;
                                    let _step_id_53442 : Int = _range_id_53434.Step;
                                    let _end_id_53447 : Int = _range_id_53434.End;
                                    while ((_step_id_53442 > 0) and (_index_id_53437 <= _end_id_53447)) or ((_step_id_53442 < 0) and (_index_id_53437 >= _end_id_53447)) {
                                        let i_1 : Int = _index_id_53437;
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
                                        _index_id_53437 += _step_id_53442;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_55385
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
                    let _generated_ident_55399 : Unit = {
                        {
                            {
                                let _range_id_53477 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_53480 : Int = _range_id_53477.Start;
                                let _step_id_53485 : Int = _range_id_53477.Step;
                                let _end_id_53490 : Int = _range_id_53477.End;
                                while ((_step_id_53485 > 0) and (_index_id_53480 <= _end_id_53490)) or ((_step_id_53485 < 0) and (_index_id_53480 >= _end_id_53490)) {
                                    let i : Int = _index_id_53480;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_53480 += _step_id_53485;
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
                                    let _range_id_53520 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                                    mutable _index_id_53523 : Int = _range_id_53520.Start;
                                    let _step_id_53528 : Int = _range_id_53520.Step;
                                    let _end_id_53533 : Int = _range_id_53520.End;
                                    while ((_step_id_53528 > 0) and (_index_id_53523 <= _end_id_53533)) or ((_step_id_53528 < 0) and (_index_id_53523 >= _end_id_53533)) {
                                        let i_1 : Int = _index_id_53523;
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
                                        _index_id_53523 += _step_id_53528;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_55399
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
                    let _generated_ident_55413 : Unit = {
                        {
                            {
                                let _range_id_53563 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_53566 : Int = _range_id_53563.Start;
                                let _step_id_53571 : Int = _range_id_53563.Step;
                                let _end_id_53576 : Int = _range_id_53563.End;
                                while ((_step_id_53571 > 0) and (_index_id_53566 <= _end_id_53576)) or ((_step_id_53571 < 0) and (_index_id_53566 >= _end_id_53576)) {
                                    let i : Int = _index_id_53566;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_53566 += _step_id_53571;
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
                                    let _range_id_53606 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                                    mutable _index_id_53609 : Int = _range_id_53606.Start;
                                    let _step_id_53614 : Int = _range_id_53606.Step;
                                    let _end_id_53619 : Int = _range_id_53606.End;
                                    while ((_step_id_53614 > 0) and (_index_id_53609 <= _end_id_53619)) or ((_step_id_53614 < 0) and (_index_id_53609 >= _end_id_53619)) {
                                        let i_1 : Int = _index_id_53609;
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
                                        _index_id_53609 += _step_id_53614;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_55413
                }

            }
        }
        // entry
        Main()"#]].assert_eq(&rendered);
}
