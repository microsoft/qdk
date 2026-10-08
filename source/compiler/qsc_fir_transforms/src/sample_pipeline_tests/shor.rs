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
                    let _array_id_48481 : Qubit[] = target;
                    let _len_id_48485 : Int = Length(_array_id_48481);
                    mutable _index_id_48490 : Int = 0;
                    while _index_id_48490 < _len_id_48485 {
                        let q : Qubit = _array_id_48481[_index_id_48490];
                        if (runningValue &&& 1) != 0 {
                            X(q);
                        }

                        runningValue >>>= 1;
                        _index_id_48490 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            adjoint ... {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_48509 : Qubit[] = target;
                    let _len_id_48513 : Int = Length(_array_id_48509);
                    mutable _index_id_48518 : Int = 0;
                    while _index_id_48518 < _len_id_48513 {
                        let q : Qubit = _array_id_48509[_index_id_48518];
                        if (runningValue &&& 1) != 0 {
                            X(q);
                        }

                        runningValue >>>= 1;
                        _index_id_48518 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            controlled (ctls, ...) {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_48537 : Qubit[] = target;
                    let _len_id_48541 : Int = Length(_array_id_48537);
                    mutable _index_id_48546 : Int = 0;
                    while _index_id_48546 < _len_id_48541 {
                        let q : Qubit = _array_id_48537[_index_id_48546];
                        if (runningValue &&& 1) != 0 {
                            Controlled X(ctls, q);
                        }

                        runningValue >>>= 1;
                        _index_id_48546 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            controlled adjoint (ctls, ...) {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_48565 : Qubit[] = target;
                    let _len_id_48569 : Int = Length(_array_id_48565);
                    mutable _index_id_48574 : Int = 0;
                    while _index_id_48574 < _len_id_48569 {
                        let q : Qubit = _array_id_48565[_index_id_48574];
                        if (runningValue &&& 1) != 0 {
                            Controlled X(ctls, q);
                        }

                        runningValue >>>= 1;
                        _index_id_48574 += 1;
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
                GlobalPhase(theta);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                if __cond_0 {
                    GlobalPhase(theta);
                } else {
                    Controlled Rz(ctls[1...], (theta, ctls[0]));
                    Controlled ControllableGlobalPhase(ctls[1...], theta / 2.);
                }

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
                    let _range_id_49649 : Range = 0..2..Length(ctls) - 2;
                    mutable _index_id_49652 : Int = _range_id_49649.Start;
                    let _step_id_49657 : Int = _range_id_49649.Step;
                    let _end_id_49662 : Int = _range_id_49649.End;
                    while ((_step_id_49657 > 0) and (_index_id_49652 <= _end_id_49662)) or ((_step_id_49657 < 0) and (_index_id_49652 >= _end_id_49662)) {
                        let i : Int = _index_id_49652;
                        CCNOT(ctls[i], ctls[i + 1], aux[i / 2]);
                        _index_id_49652 += _step_id_49657;
                    }

                }

                {
                    let _range_id_49692 : Range = 0..((Length(ctls) / 2) - 2) - adjustment;
                    mutable _index_id_49695 : Int = _range_id_49692.Start;
                    let _step_id_49700 : Int = _range_id_49692.Step;
                    let _end_id_49705 : Int = _range_id_49692.End;
                    while ((_step_id_49700 > 0) and (_index_id_49695 <= _end_id_49705)) or ((_step_id_49700 < 0) and (_index_id_49695 >= _end_id_49705)) {
                        let i_1 : Int = _index_id_49695;
                        CCNOT(aux[i_1 * 2], aux[(i_1 * 2) + 1], aux[i_1 + (Length(ctls) / 2)]);
                        _index_id_49695 += _step_id_49700;
                    }

                }

            }
            adjoint ... {
                {
                    let _range : Range = 0..((Length(ctls) / 2) - 2) - adjustment;
                    {
                        let _range_id_49735 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_49738 : Int = _range_id_49735.Start;
                        let _step_id_49743 : Int = _range_id_49735.Step;
                        let _end_id_49748 : Int = _range_id_49735.End;
                        while ((_step_id_49743 > 0) and (_index_id_49738 <= _end_id_49748)) or ((_step_id_49743 < 0) and (_index_id_49738 >= _end_id_49748)) {
                            let i : Int = _index_id_49738;
                            Adjoint CCNOT(aux[i * 2], aux[(i * 2) + 1], aux[i + (Length(ctls) / 2)]);
                            _index_id_49738 += _step_id_49743;
                        }

                    }

                }

                {
                    let _range_1 : Range = 0..2..Length(ctls) - 2;
                    {
                        let _range_id_49778 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_49781 : Int = _range_id_49778.Start;
                        let _step_id_49786 : Int = _range_id_49778.Step;
                        let _end_id_49791 : Int = _range_id_49778.End;
                        while ((_step_id_49786 > 0) and (_index_id_49781 <= _end_id_49791)) or ((_step_id_49786 < 0) and (_index_id_49781 >= _end_id_49791)) {
                            let i_1 : Int = _index_id_49781;
                            Adjoint CCNOT(ctls[i_1], ctls[i_1 + 1], aux[i_1 / 2]);
                            _index_id_49781 += _step_id_49786;
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
                            let _generated_ident_54808 : Unit = {
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
                            _generated_ident_54808
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
                            let _generated_ident_54822 : Unit = {
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
                            _generated_ident_54822
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
                let _array_id_50150 : Qubit[] = qubits;
                let _len_id_50154 : Int = Length(_array_id_50150);
                mutable _index_id_50159 : Int = 0;
                while _index_id_50159 < _len_id_50154 {
                    let q : Qubit = _array_id_50150[_index_id_50159];
                    Reset(q);
                    _index_id_50159 += 1;
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
                        let _generated_ident_54878 : Unit = {
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
                        _generated_ident_54878
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
                            let _generated_ident_54906 : Unit = {
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
                            _generated_ident_54906
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
                            let _generated_ident_54920 : Unit = {
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
                            _generated_ident_54920
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
                        let _generated_ident_54962 : Unit = {
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
                        _generated_ident_54962
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
                        let _generated_ident_54976 : Unit = {
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
                        _generated_ident_54976
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
                            let _generated_ident_54990 : Unit = {
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
                            _generated_ident_54990
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
                            let _generated_ident_55004 : Unit = {
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
                            _generated_ident_55004
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
                    let _range_id_52006 : Range = 1..Length(xs) - 1;
                    mutable _index_id_52009 : Int = _range_id_52006.Start;
                    let _step_id_52014 : Int = _range_id_52006.Step;
                    let _end_id_52019 : Int = _range_id_52006.End;
                    while ((_step_id_52014 > 0) and (_index_id_52009 <= _end_id_52019)) or ((_step_id_52014 < 0) and (_index_id_52009 >= _end_id_52019)) {
                        let i : Int = _index_id_52009;
                        CNOT(xs[i], ys[i]);
                        _index_id_52009 += _step_id_52014;
                    }

                }

                {
                    let _range_id_52049 : Range = Length(xs) - 2..(-1)..1;
                    mutable _index_id_52052 : Int = _range_id_52049.Start;
                    let _step_id_52057 : Int = _range_id_52049.Step;
                    let _end_id_52062 : Int = _range_id_52049.End;
                    while ((_step_id_52057 > 0) and (_index_id_52052 <= _end_id_52062)) or ((_step_id_52057 < 0) and (_index_id_52052 >= _end_id_52062)) {
                        let i_1 : Int = _index_id_52052;
                        CNOT(xs[i_1], xs[i_1 + 1]);
                        _index_id_52052 += _step_id_52057;
                    }

                }

            }
            adjoint ... {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range : Range = Length(xs) - 2..(-1)..1;
                    {
                        let _range_id_52092 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_52095 : Int = _range_id_52092.Start;
                        let _step_id_52100 : Int = _range_id_52092.Step;
                        let _end_id_52105 : Int = _range_id_52092.End;
                        while ((_step_id_52100 > 0) and (_index_id_52095 <= _end_id_52105)) or ((_step_id_52100 < 0) and (_index_id_52095 >= _end_id_52105)) {
                            let i : Int = _index_id_52095;
                            Adjoint CNOT(xs[i], xs[i + 1]);
                            _index_id_52095 += _step_id_52100;
                        }

                    }

                }

                {
                    let _range_1 : Range = 1..Length(xs) - 1;
                    {
                        let _range_id_52135 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_52138 : Int = _range_id_52135.Start;
                        let _step_id_52143 : Int = _range_id_52135.Step;
                        let _end_id_52148 : Int = _range_id_52135.End;
                        while ((_step_id_52143 > 0) and (_index_id_52138 <= _end_id_52148)) or ((_step_id_52143 < 0) and (_index_id_52138 >= _end_id_52148)) {
                            let i_1 : Int = _index_id_52138;
                            Adjoint CNOT(xs[i_1], ys[i_1]);
                            _index_id_52138 += _step_id_52143;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range_id_52178 : Range = 1..Length(xs) - 1;
                    mutable _index_id_52181 : Int = _range_id_52178.Start;
                    let _step_id_52186 : Int = _range_id_52178.Step;
                    let _end_id_52191 : Int = _range_id_52178.End;
                    while ((_step_id_52186 > 0) and (_index_id_52181 <= _end_id_52191)) or ((_step_id_52186 < 0) and (_index_id_52181 >= _end_id_52191)) {
                        let i : Int = _index_id_52181;
                        Controlled CNOT(ctls, (xs[i], ys[i]));
                        _index_id_52181 += _step_id_52186;
                    }

                }

                {
                    let _range_id_52221 : Range = Length(xs) - 2..(-1)..1;
                    mutable _index_id_52224 : Int = _range_id_52221.Start;
                    let _step_id_52229 : Int = _range_id_52221.Step;
                    let _end_id_52234 : Int = _range_id_52221.End;
                    while ((_step_id_52229 > 0) and (_index_id_52224 <= _end_id_52234)) or ((_step_id_52229 < 0) and (_index_id_52224 >= _end_id_52234)) {
                        let i_1 : Int = _index_id_52224;
                        Controlled CNOT(ctls, (xs[i_1], xs[i_1 + 1]));
                        _index_id_52224 += _step_id_52229;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range : Range = Length(xs) - 2..(-1)..1;
                    {
                        let _range_id_52264 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_52267 : Int = _range_id_52264.Start;
                        let _step_id_52272 : Int = _range_id_52264.Step;
                        let _end_id_52277 : Int = _range_id_52264.End;
                        while ((_step_id_52272 > 0) and (_index_id_52267 <= _end_id_52277)) or ((_step_id_52272 < 0) and (_index_id_52267 >= _end_id_52277)) {
                            let i : Int = _index_id_52267;
                            Controlled Adjoint CNOT(ctls, (xs[i], xs[i + 1]));
                            _index_id_52267 += _step_id_52272;
                        }

                    }

                }

                {
                    let _range_1 : Range = 1..Length(xs) - 1;
                    {
                        let _range_id_52307 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_52310 : Int = _range_id_52307.Start;
                        let _step_id_52315 : Int = _range_id_52307.Step;
                        let _end_id_52320 : Int = _range_id_52307.End;
                        while ((_step_id_52315 > 0) and (_index_id_52310 <= _end_id_52320)) or ((_step_id_52315 < 0) and (_index_id_52310 >= _end_id_52320)) {
                            let i_1 : Int = _index_id_52310;
                            Controlled Adjoint CNOT(ctls, (xs[i_1], ys[i_1]));
                            _index_id_52310 += _step_id_52315;
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
                    let _range_id_52350 : Range = 0..Length(xs) - 2;
                    mutable _index_id_52353 : Int = _range_id_52350.Start;
                    let _step_id_52358 : Int = _range_id_52350.Step;
                    let _end_id_52363 : Int = _range_id_52350.End;
                    while ((_step_id_52358 > 0) and (_index_id_52353 <= _end_id_52363)) or ((_step_id_52358 < 0) and (_index_id_52353 >= _end_id_52363)) {
                        let idx : Int = _index_id_52353;
                        CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                        _index_id_52353 += _step_id_52358;
                    }

                }

                {
                    let _range_id_52393 : Range = Length(xs) - 1..(-1)..1;
                    mutable _index_id_52396 : Int = _range_id_52393.Start;
                    let _step_id_52401 : Int = _range_id_52393.Step;
                    let _end_id_52406 : Int = _range_id_52393.End;
                    while ((_step_id_52401 > 0) and (_index_id_52396 <= _end_id_52406)) or ((_step_id_52401 < 0) and (_index_id_52396 >= _end_id_52406)) {
                        let idx_1 : Int = _index_id_52396;
                        Controlled CNOT(controls, (xs[idx_1], ys[idx_1]));
                        CCNOT(xs[idx_1 - 1], ys[idx_1 - 1], xs[idx_1]);
                        _index_id_52396 += _step_id_52401;
                    }

                }

            }
            controlled adjoint (controls, ...) {
                Fact(Length(xs) == Length(ys), $"Input registers must have the same number of qubits.");
                {
                    let _range : Range = Length(xs) - 1..(-1)..1;
                    {
                        let _range_id_52436 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_52439 : Int = _range_id_52436.Start;
                        let _step_id_52444 : Int = _range_id_52436.Step;
                        let _end_id_52449 : Int = _range_id_52436.End;
                        while ((_step_id_52444 > 0) and (_index_id_52439 <= _end_id_52449)) or ((_step_id_52444 < 0) and (_index_id_52439 >= _end_id_52449)) {
                            let idx : Int = _index_id_52439;
                            Adjoint CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                            Adjoint Controlled CNOT(controls, (xs[idx], ys[idx]));
                            _index_id_52439 += _step_id_52444;
                        }

                    }

                }

                {
                    let _range_1 : Range = 0..Length(xs) - 2;
                    {
                        let _range_id_52479 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_52482 : Int = _range_id_52479.Start;
                        let _step_id_52487 : Int = _range_id_52479.Step;
                        let _end_id_52492 : Int = _range_id_52479.End;
                        while ((_step_id_52487 > 0) and (_index_id_52482 <= _end_id_52492)) or ((_step_id_52487 < 0) and (_index_id_52482 >= _end_id_52492)) {
                            let idx_1 : Int = _index_id_52482;
                            Adjoint CCNOT(xs[idx_1], ys[idx_1], xs[idx_1 + 1]);
                            _index_id_52482 += _step_id_52487;
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
                    let _range_id_52522 : Range = 0..nQubits - 2;
                    mutable _index_id_52525 : Int = _range_id_52522.Start;
                    let _step_id_52530 : Int = _range_id_52522.Step;
                    let _end_id_52535 : Int = _range_id_52522.End;
                    while ((_step_id_52530 > 0) and (_index_id_52525 <= _end_id_52535)) or ((_step_id_52530 < 0) and (_index_id_52525 >= _end_id_52535)) {
                        let idx : Int = _index_id_52525;
                        CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                        _index_id_52525 += _step_id_52530;
                    }

                }

                Controlled CCNOT(controls, (xs[nQubits - 1], ys[nQubits - 1], ys[nQubits]));
                {
                    let _range_id_52565 : Range = nQubits - 1..(-1)..1;
                    mutable _index_id_52568 : Int = _range_id_52565.Start;
                    let _step_id_52573 : Int = _range_id_52565.Step;
                    let _end_id_52578 : Int = _range_id_52565.End;
                    while ((_step_id_52573 > 0) and (_index_id_52568 <= _end_id_52578)) or ((_step_id_52573 < 0) and (_index_id_52568 >= _end_id_52578)) {
                        let idx_1 : Int = _index_id_52568;
                        Controlled CNOT(controls, (xs[idx_1], ys[idx_1]));
                        CCNOT(xs[idx_1 - 1], ys[idx_1 - 1], xs[idx_1]);
                        _index_id_52568 += _step_id_52573;
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
                        let _range_id_52608 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                        mutable _index_id_52611 : Int = _range_id_52608.Start;
                        let _step_id_52616 : Int = _range_id_52608.Step;
                        let _end_id_52621 : Int = _range_id_52608.End;
                        while ((_step_id_52616 > 0) and (_index_id_52611 <= _end_id_52621)) or ((_step_id_52616 < 0) and (_index_id_52611 >= _end_id_52621)) {
                            let idx : Int = _index_id_52611;
                            Adjoint CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                            Adjoint Controlled CNOT(controls, (xs[idx], ys[idx]));
                            _index_id_52611 += _step_id_52616;
                        }

                    }

                }

                Adjoint Controlled CCNOT(controls, (xs[nQubits - 1], ys[nQubits - 1], ys[nQubits]));
                {
                    let _range_1 : Range = 0..nQubits - 2;
                    {
                        let _range_id_52651 : Range = (_range_1.Start + ((((_range_1.End - _range_1.Start) + _range_1.Step) / _range_1.Step) * _range_1.Step)) - _range_1.Step..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_52654 : Int = _range_id_52651.Start;
                        let _step_id_52659 : Int = _range_id_52651.Step;
                        let _end_id_52664 : Int = _range_id_52651.End;
                        while ((_step_id_52659 > 0) and (_index_id_52654 <= _end_id_52664)) or ((_step_id_52659 < 0) and (_index_id_52654 >= _end_id_52664)) {
                            let idx_1 : Int = _index_id_52654;
                            Adjoint CCNOT(xs[idx_1], ys[idx_1], xs[idx_1 + 1]);
                            _index_id_52654 += _step_id_52659;
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
        function IndexRange_Qubit_(array : Qubit[]) : Range {
            0..Length(array) - 1
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
                    let _generated_ident_55248 : Unit = {
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
                    _generated_ident_55248
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
                    let _generated_ident_55262 : Unit = {
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
                    _generated_ident_55262
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
                    let _generated_ident_55276 : Unit = {
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
                    _generated_ident_55276
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
                    let _generated_ident_55290 : Unit = {
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
                    _generated_ident_55290
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
                    let _generated_ident_55419 : Unit = {
                        {
                            {
                                let _range_id_53367 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_53370 : Int = _range_id_53367.Start;
                                let _step_id_53375 : Int = _range_id_53367.Step;
                                let _end_id_53380 : Int = _range_id_53367.End;
                                while ((_step_id_53375 > 0) and (_index_id_53370 <= _end_id_53380)) or ((_step_id_53375 < 0) and (_index_id_53370 >= _end_id_53380)) {
                                    let i : Int = _index_id_53370;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_53370 += _step_id_53375;
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
                                    let _range_id_53410 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                                    mutable _index_id_53413 : Int = _range_id_53410.Start;
                                    let _step_id_53418 : Int = _range_id_53410.Step;
                                    let _end_id_53423 : Int = _range_id_53410.End;
                                    while ((_step_id_53418 > 0) and (_index_id_53413 <= _end_id_53423)) or ((_step_id_53418 < 0) and (_index_id_53413 >= _end_id_53423)) {
                                        let i_1 : Int = _index_id_53413;
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
                                        _index_id_53413 += _step_id_53418;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_55419
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
                    let _generated_ident_55433 : Unit = {
                        {
                            {
                                let _range_id_53453 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_53456 : Int = _range_id_53453.Start;
                                let _step_id_53461 : Int = _range_id_53453.Step;
                                let _end_id_53466 : Int = _range_id_53453.End;
                                while ((_step_id_53461 > 0) and (_index_id_53456 <= _end_id_53466)) or ((_step_id_53461 < 0) and (_index_id_53456 >= _end_id_53466)) {
                                    let i : Int = _index_id_53456;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_53456 += _step_id_53461;
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
                                    let _range_id_53496 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                                    mutable _index_id_53499 : Int = _range_id_53496.Start;
                                    let _step_id_53504 : Int = _range_id_53496.Step;
                                    let _end_id_53509 : Int = _range_id_53496.End;
                                    while ((_step_id_53504 > 0) and (_index_id_53499 <= _end_id_53509)) or ((_step_id_53504 < 0) and (_index_id_53499 >= _end_id_53509)) {
                                        let i_1 : Int = _index_id_53499;
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
                                        _index_id_53499 += _step_id_53504;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_55433
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
                    let _generated_ident_55447 : Unit = {
                        {
                            {
                                let _range_id_53539 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_53542 : Int = _range_id_53539.Start;
                                let _step_id_53547 : Int = _range_id_53539.Step;
                                let _end_id_53552 : Int = _range_id_53539.End;
                                while ((_step_id_53547 > 0) and (_index_id_53542 <= _end_id_53552)) or ((_step_id_53547 < 0) and (_index_id_53542 >= _end_id_53552)) {
                                    let i : Int = _index_id_53542;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_53542 += _step_id_53547;
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
                                    let _range_id_53582 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                                    mutable _index_id_53585 : Int = _range_id_53582.Start;
                                    let _step_id_53590 : Int = _range_id_53582.Step;
                                    let _end_id_53595 : Int = _range_id_53582.End;
                                    while ((_step_id_53590 > 0) and (_index_id_53585 <= _end_id_53595)) or ((_step_id_53590 < 0) and (_index_id_53585 >= _end_id_53595)) {
                                        let i_1 : Int = _index_id_53585;
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
                                        _index_id_53585 += _step_id_53590;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_55447
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
                    let _generated_ident_55461 : Unit = {
                        {
                            {
                                let _range_id_53625 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_53628 : Int = _range_id_53625.Start;
                                let _step_id_53633 : Int = _range_id_53625.Step;
                                let _end_id_53638 : Int = _range_id_53625.End;
                                while ((_step_id_53633 > 0) and (_index_id_53628 <= _end_id_53638)) or ((_step_id_53633 < 0) and (_index_id_53628 >= _end_id_53638)) {
                                    let i : Int = _index_id_53628;
                                    if (cNormalized &&& (1L <<< (i + 1))) != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_53628 += _step_id_53633;
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
                                    let _range_id_53668 : Range = (_range.Start + ((((_range.End - _range.Start) + _range.Step) / _range.Step) * _range.Step)) - _range.Step..(-_range.Step).._range.Start;
                                    mutable _index_id_53671 : Int = _range_id_53668.Start;
                                    let _step_id_53676 : Int = _range_id_53668.Step;
                                    let _end_id_53681 : Int = _range_id_53668.End;
                                    while ((_step_id_53676 > 0) and (_index_id_53671 <= _end_id_53681)) or ((_step_id_53676 < 0) and (_index_id_53671 >= _end_id_53681)) {
                                        let i_1 : Int = _index_id_53671;
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
                                        _index_id_53671 += _step_id_53676;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_55461
                }

            }
        }
        // entry
        Main()"#]].assert_eq(&rendered);
}
