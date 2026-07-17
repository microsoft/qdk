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
                let _range_id_210 : Range = 0..size - 1;
                mutable _index_id_213 : Int = _range_id_210::Start;
                let _step_id_218 : Int = _range_id_210::Step;
                let _end_id_223 : Int = _range_id_210::End;
                while _step_id_218 > 0 and _index_id_213 <= _end_id_223 or _step_id_218 < 0 and _index_id_213 >= _end_id_223 {
                    let _ : Int = _index_id_213;
                    qs += [__quantum__rt__qubit_allocate()];
                    _index_id_213 += _step_id_218;
                }

            }

            qs
        }
        operation ReleaseQubitArray(qs : Qubit[]) : Unit {
            {
                let _array_id_296 : Qubit[] = qs;
                let _len_id_300 : Int = Length(_array_id_296);
                mutable _index_id_305 : Int = 0;
                while _index_id_305 < _len_id_300 {
                    let q : Qubit = _array_id_296[_index_id_305];
                    __quantum__rt__qubit_release(q);
                    _index_id_305 += 1;
                }

            }

        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        // package 1
        operation MapPauliAxis(from : Pauli, to : Pauli, q : Qubit) : Unit is Adj + Ctl {
            body ... {
                if from == to {} else if from == PauliZ and to == PauliX or from == PauliX and to == PauliZ {
                    H(q);
                } else if from == PauliZ and to == PauliY {
                    Adjoint S(q);
                    H(q);
                } else if from == PauliY and to == PauliZ {
                    H(q);
                    S(q);
                } else if from == PauliY and to == PauliX {
                    S(q);
                } else if from == PauliX and to == PauliY {
                    Adjoint S(q);
                } else {
                    fail $"Unsupported mapping of Pauli axes.";
                }

            }
            adjoint ... {
                if from == to {} else if from == PauliZ and to == PauliX or from == PauliX and to == PauliZ {
                    Adjoint H(q);
                } else if from == PauliZ and to == PauliY {
                    Adjoint H(q);
                    Adjoint Adjoint S(q);
                } else if from == PauliY and to == PauliZ {
                    Adjoint S(q);
                    Adjoint H(q);
                } else if from == PauliY and to == PauliX {
                    Adjoint S(q);
                } else if from == PauliX and to == PauliY {
                    Adjoint Adjoint S(q);
                } else {
                    fail $"Unsupported mapping of Pauli axes.";
                }

            }
            controlled (ctls, ...) {
                if from == to {} else if from == PauliZ and to == PauliX or from == PauliX and to == PauliZ {
                    Controlled H(ctls, q);
                } else if from == PauliZ and to == PauliY {
                    Controlled Adjoint S(ctls, q);
                    Controlled H(ctls, q);
                } else if from == PauliY and to == PauliZ {
                    Controlled H(ctls, q);
                    Controlled S(ctls, q);
                } else if from == PauliY and to == PauliX {
                    Controlled S(ctls, q);
                } else if from == PauliX and to == PauliY {
                    Controlled Adjoint S(ctls, q);
                } else {
                    fail $"Unsupported mapping of Pauli axes.";
                }

            }
            controlled adjoint (ctls, ...) {
                if from == to {} else if from == PauliZ and to == PauliX or from == PauliX and to == PauliZ {
                    Controlled Adjoint H(ctls, q);
                } else if from == PauliZ and to == PauliY {
                    Controlled Adjoint H(ctls, q);
                    Controlled Adjoint Adjoint S(ctls, q);
                } else if from == PauliY and to == PauliZ {
                    Controlled Adjoint S(ctls, q);
                    Controlled Adjoint H(ctls, q);
                } else if from == PauliY and to == PauliX {
                    Controlled Adjoint S(ctls, q);
                } else if from == PauliX and to == PauliY {
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
                    let _array_id_47802 : Qubit[] = target;
                    let _len_id_47806 : Int = Length(_array_id_47802);
                    mutable _index_id_47811 : Int = 0;
                    while _index_id_47811 < _len_id_47806 {
                        let q : Qubit = _array_id_47802[_index_id_47811];
                        if runningValue &&& 1 != 0 {
                            X(q);
                        }

                        runningValue >>>= 1;
                        _index_id_47811 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            adjoint ... {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_47830 : Qubit[] = target;
                    let _len_id_47834 : Int = Length(_array_id_47830);
                    mutable _index_id_47839 : Int = 0;
                    while _index_id_47839 < _len_id_47834 {
                        let q : Qubit = _array_id_47830[_index_id_47839];
                        if runningValue &&& 1 != 0 {
                            X(q);
                        }

                        runningValue >>>= 1;
                        _index_id_47839 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            controlled (ctls, ...) {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_47858 : Qubit[] = target;
                    let _len_id_47862 : Int = Length(_array_id_47858);
                    mutable _index_id_47867 : Int = 0;
                    while _index_id_47867 < _len_id_47862 {
                        let q : Qubit = _array_id_47858[_index_id_47867];
                        if runningValue &&& 1 != 0 {
                            Controlled X(ctls, q);
                        }

                        runningValue >>>= 1;
                        _index_id_47867 += 1;
                    }

                }

                Fact(runningValue == 0, $"value is too large");
            }
            controlled adjoint (ctls, ...) {
                Fact(value >= 0, $"`value` must be non-negative.");
                mutable runningValue : Int = value;
                {
                    let _array_id_47886 : Qubit[] = target;
                    let _len_id_47890 : Int = Length(_array_id_47886);
                    mutable _index_id_47895 : Int = 0;
                    while _index_id_47895 < _len_id_47890 {
                        let q : Qubit = _array_id_47886[_index_id_47895];
                        if runningValue &&& 1 != 0 {
                            Controlled X(ctls, q);
                        }

                        runningValue >>>= 1;
                        _index_id_47895 += 1;
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
            body ... {
                if not actual {
                    fail message;
                }

            }
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
                ControllableGlobalPhase(-theta);
            }
            controlled (ctls, ...) {
                Controlled ControllableGlobalPhase(ctls, theta);
            }
            controlled adjoint (ctls, ...) {
                Controlled ControllableGlobalPhase(ctls, -theta);
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
                Rz(-theta / 2., target);
                CNOT(control, target);
            }
            adjoint ... {
                Adjoint CNOT(control, target);
                Adjoint Rz(-theta / 2., target);
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
                    let _range_id_48970 : Range = 0..2..Length(ctls) - 2;
                    mutable _index_id_48973 : Int = _range_id_48970::Start;
                    let _step_id_48978 : Int = _range_id_48970::Step;
                    let _end_id_48983 : Int = _range_id_48970::End;
                    while _step_id_48978 > 0 and _index_id_48973 <= _end_id_48983 or _step_id_48978 < 0 and _index_id_48973 >= _end_id_48983 {
                        let i : Int = _index_id_48973;
                        CCNOT(ctls[i], ctls[i + 1], aux[i / 2]);
                        _index_id_48973 += _step_id_48978;
                    }

                }

                {
                    let _range_id_49013 : Range = 0..Length(ctls) / 2 - 2 - adjustment;
                    mutable _index_id_49016 : Int = _range_id_49013::Start;
                    let _step_id_49021 : Int = _range_id_49013::Step;
                    let _end_id_49026 : Int = _range_id_49013::End;
                    while _step_id_49021 > 0 and _index_id_49016 <= _end_id_49026 or _step_id_49021 < 0 and _index_id_49016 >= _end_id_49026 {
                        let i : Int = _index_id_49016;
                        CCNOT(aux[i * 2], aux[i * 2 + 1], aux[i + Length(ctls) / 2]);
                        _index_id_49016 += _step_id_49021;
                    }

                }

            }
            adjoint ... {
                {
                    let _range : Range = 0..Length(ctls) / 2 - 2 - adjustment;
                    {
                        let _range_id_49056 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_49059 : Int = _range_id_49056::Start;
                        let _step_id_49064 : Int = _range_id_49056::Step;
                        let _end_id_49069 : Int = _range_id_49056::End;
                        while _step_id_49064 > 0 and _index_id_49059 <= _end_id_49069 or _step_id_49064 < 0 and _index_id_49059 >= _end_id_49069 {
                            let i : Int = _index_id_49059;
                            Adjoint CCNOT(aux[i * 2], aux[i * 2 + 1], aux[i + Length(ctls) / 2]);
                            _index_id_49059 += _step_id_49064;
                        }

                    }

                }

                {
                    let _range : Range = 0..2..Length(ctls) - 2;
                    {
                        let _range_id_49099 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_49102 : Int = _range_id_49099::Start;
                        let _step_id_49107 : Int = _range_id_49099::Step;
                        let _end_id_49112 : Int = _range_id_49099::End;
                        while _step_id_49107 > 0 and _index_id_49102 <= _end_id_49112 or _step_id_49107 < 0 and _index_id_49102 >= _end_id_49112 {
                            let i : Int = _index_id_49102;
                            Adjoint CCNOT(ctls[i], ctls[i + 1], aux[i / 2]);
                            _index_id_49102 += _step_id_49107;
                        }

                    }

                }

            }
        }
        operation AdjustForSingleControl(ctls : Qubit[], aux : Qubit[]) : Unit is Adj {
            body ... {
                let __cond_0 : Bool = Length(ctls) % 2 != 0;
                if __cond_0 {
                    CCNOT(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], aux[Length(ctls) - 2]);
                }

            }
            adjoint ... {
                let __cond_0 : Bool = Length(ctls) % 2 != 0;
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
                            let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 1 - Length(ctls) % 2);
                            let _generated_ident_54073 : Unit = {
                                {
                                    CollectControls(ctls, aux, 0);
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = Length(ctls) % 2 != 0;
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
                            _generated_ident_54073
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
                            let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 1 - Length(ctls) % 2);
                            let _generated_ident_54087 : Unit = {
                                {
                                    CollectControls(ctls, aux, 0);
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = Length(ctls) % 2 != 0;
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
                            _generated_ident_54087
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
                    ApplyGlobalPhase(-theta / 2.);
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
                    Adjoint ApplyGlobalPhase(-theta / 2.);
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
                    Controlled ApplyGlobalPhase(ctls, -theta / 2.);
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
                    Controlled Adjoint ApplyGlobalPhase(ctls, -theta / 2.);
                }

            }
        }
        operation R1Frac(numerator : Int, power : Int, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                RFrac(PauliZ, -numerator, power + 1, qubit);
                RFrac(PauliI, numerator, power + 1, qubit);
            }
            adjoint ... {
                Adjoint RFrac(PauliI, numerator, power + 1, qubit);
                Adjoint RFrac(PauliZ, -numerator, power + 1, qubit);
            }
            controlled (ctls, ...) {
                Controlled RFrac(ctls, (PauliZ, -numerator, power + 1, qubit));
                Controlled RFrac(ctls, (PauliI, numerator, power + 1, qubit));
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint RFrac(ctls, (PauliI, numerator, power + 1, qubit));
                Controlled Adjoint RFrac(ctls, (PauliZ, -numerator, power + 1, qubit));
            }
        }
        operation Reset(qubit : Qubit) : Unit {
            __quantum__qis__reset__body(qubit);
        }
        operation ResetAll(qubits : Qubit[]) : Unit {
            {
                let _array_id_49471 : Qubit[] = qubits;
                let _len_id_49475 : Int = Length(_array_id_49471);
                mutable _index_id_49480 : Int = 0;
                while _index_id_49480 < _len_id_49475 {
                    let q : Qubit = _array_id_49471[_index_id_49480];
                    Reset(q);
                    _index_id_49480 += 1;
                }

            }

        }
        operation RFrac(pauli : Pauli, numerator : Int, power : Int, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                let angle : Double = -2. * PI() * IntAsDouble(numerator) / 2.^IntAsDouble(power);
                R(pauli, angle, qubit);
            }
            adjoint ... {
                let angle : Double = -2. * PI() * IntAsDouble(numerator) / 2.^IntAsDouble(power);
                Adjoint R(pauli, angle, qubit);
            }
            controlled (ctls, ...) {
                let angle : Double = -2. * PI() * IntAsDouble(numerator) / 2.^IntAsDouble(power);
                Controlled R(ctls, (pauli, angle, qubit));
            }
            controlled adjoint (ctls, ...) {
                let angle : Double = -2. * PI() * IntAsDouble(numerator) / 2.^IntAsDouble(power);
                Controlled Adjoint R(ctls, (pauli, angle, qubit));
            }
        }
        operation Rx(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__rx__body(theta, qubit);
            }
            adjoint ... {
                Rx(-theta, qubit);
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
                Controlled Rx(ctls, (-theta, qubit));
            }
        }
        operation Ry(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__ry__body(theta, qubit);
            }
            adjoint ... {
                Ry(-theta, qubit);
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
                Controlled Ry(ctls, (-theta, qubit));
            }
        }
        operation Rz(theta : Double, qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__rz__body(theta, qubit);
            }
            adjoint ... {
                Rz(-theta, qubit);
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
                        let _generated_ident_54143 : Unit = {
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
                        _generated_ident_54143
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled Rz(ctls, (-theta, qubit));
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
                            let _generated_ident_54171 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - Length(ctls) % 2);
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = Length(ctls) % 2 != 0;
                                    if __cond_3 {
                                        Controlled CS([ctls[Length(ctls) - 1]], (aux[Length(ctls) - 3], qubit));
                                    } else {
                                        Controlled CS([aux[Length(ctls) - 3]], (aux[Length(ctls) - 4], qubit));
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - Length(ctls) % 2);
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_54171
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
                            let _generated_ident_54185 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - Length(ctls) % 2);
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = Length(ctls) % 2 != 0;
                                    if __cond_3 {
                                        Controlled Adjoint CS([ctls[Length(ctls) - 1]], (aux[Length(ctls) - 3], qubit));
                                    } else {
                                        Controlled Adjoint CS([aux[Length(ctls) - 3]], (aux[Length(ctls) - 4], qubit));
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - Length(ctls) % 2);
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_54185
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
                        let _generated_ident_54227 : Unit = {
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
                        _generated_ident_54227
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
                        let _generated_ident_54241 : Unit = {
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
                        _generated_ident_54241
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
                            let _generated_ident_54255 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - Length(ctls) % 2);
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = Length(ctls) % 2 != 0;
                                    if __cond_3 {
                                        __quantum__qis__ccx__body(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                                    } else {
                                        __quantum__qis__ccx__body(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - Length(ctls) % 2);
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_54255
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
                            let _generated_ident_54269 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - Length(ctls) % 2);
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = Length(ctls) % 2 != 0;
                                    if __cond_3 {
                                        __quantum__qis__ccx__body(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                                    } else {
                                        __quantum__qis__ccx__body(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - Length(ctls) % 2);
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_54269
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
        -1
            } else if a > 0 {
                + 1
            } else {
                0
            }

        }
        function AbsI(a : Int) : Int {
            if a < 0 {
        -a
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

            mutable res : Int = if not __has_returned {
                1
            } else {
                0
            };
            mutable expPow2mod : Int = if not __has_returned {
                expBase % modulus
            } else {
                0
            };
            mutable powerBits : Int = if not __has_returned {
                power
            } else {
                0
            };
            if not __has_returned {
                while powerBits > 0 {
                    if powerBits &&& 1 != 0 {
                        res = res * expPow2mod % modulus;
                    }

                    expPow2mod = expPow2mod * expPow2mod % modulus;
                    powerBits >>>= 1;
                }

            };
            if __has_returned {
                __ret_val
            } else {
                if not __has_returned {
                    res
                } else {
                    __ret_val
                }
            }

        }
        function InverseModI(a : Int, modulus : Int) : Int {
            let (u : Int, v : Int) = ExtendedGreatestCommonDivisorI(a, modulus);
            let gcd : Int = u * a + v * modulus;
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
                (r1, r2) = (r2, r1 - quotient * r2);
                (s1, s2) = (s2, s1 - quotient * s2);
                (t1, t2) = (t2, t1 - quotient * t2);
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
            while r2 != 0 and AbsI(s2) <= denominatorBound {
                let quotient : Int = r1 / r2;
                (r1, r2) = (r2, r1 - quotient * r2);
                (s1, s2) = (s2, s1 - quotient * s2);
                (t1, t2) = (t2, t1 - quotient * t2);
            }

            if r2 == 0 and AbsI(s2) <= denominatorBound {
                (-t2 * signB, s2 * signA)
            } else {
                (-t1 * signB, s1 * signA)
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
            while n &&& 1 == 0 {
                count += 1;
                n >>>= 1;
            }

            count
        }
        function TrailingZeroCountL(a : BigInt) : Int {
            Fact(a != 0L, $"TrailingZeroCountL: `a` cannot be 0.");
            mutable count : Int = 0;
            mutable n : BigInt = a;
            while n &&& 1L == 0L {
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
                } else if xsLen + 1 == ysLen {
                    if xsLen > 1 {
                        CNOT(xs[xsLen - 1], ys[ysLen - 1]);
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res : Unit = {
                                ApplyInnerTTKAdderWithCarry(xs, ys);
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res
                        }

                    } else {
                        CCNOT(xs[0], ys[0], ys[1]);
                    }

                    CNOT(xs[0], ys[0]);
                } else if xsLen + 2 <= ysLen {
                    let padding : Qubit[] = AllocateQubitArray(ysLen - xsLen - 1);
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

                } else if xsLen + 1 == ysLen {
                    Adjoint CNOT(xs[0], ys[0]);
                    if xsLen > 1 {
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res : Unit = {
                                Adjoint ApplyInnerTTKAdderWithCarry(xs, ys);
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res
                        }

                        Adjoint CNOT(xs[xsLen - 1], ys[ysLen - 1]);
                    } else {
                        Adjoint CCNOT(xs[0], ys[0], ys[1]);
                    }

                } else if xsLen + 2 <= ysLen {
                    let padding : Qubit[] = AllocateQubitArray(ysLen - xsLen - 1);
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
                } else if xsLen + 1 == ysLen {
                    if xsLen > 1 {
                        Controlled CNOT(ctls, (xs[xsLen - 1], ys[ysLen - 1]));
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res : Unit = {
                                Controlled ApplyInnerTTKAdderWithCarry(ctls, (xs, ys));
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res
                        }

                    } else {
                        Controlled CCNOT(ctls, (xs[0], ys[0], ys[1]));
                    }

                    Controlled CNOT(ctls, (xs[0], ys[0]));
                } else if xsLen + 2 <= ysLen {
                    let padding : Qubit[] = AllocateQubitArray(ysLen - xsLen - 1);
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

                } else if xsLen + 1 == ysLen {
                    Controlled Adjoint CNOT(ctls, (xs[0], ys[0]));
                    if xsLen > 1 {
                        {
                            {
                                ApplyOuterTTKAdder(xs, ys);
                            }

                            let _apply_res : Unit = {
                                Controlled Adjoint ApplyInnerTTKAdderWithCarry(ctls, (xs, ys));
                            };
                            {
                                Adjoint ApplyOuterTTKAdder(xs, ys);
                            }

                            _apply_res
                        }

                        Controlled Adjoint CNOT(ctls, (xs[xsLen - 1], ys[ysLen - 1]));
                    } else {
                        Controlled Adjoint CCNOT(ctls, (xs[0], ys[0], ys[1]));
                    }

                } else if xsLen + 2 <= ysLen {
                    let padding : Qubit[] = AllocateQubitArray(ysLen - xsLen - 1);
                    Controlled Adjoint RippleCarryTTKIncByLE(ctls, (xs + padding, ys));
                    ReleaseQubitArray(padding);
                }

            }
        }
        operation ApplyOuterTTKAdder(xs : Qubit[], ys : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range_id_51271 : Range = 1..Length(xs) - 1;
                    mutable _index_id_51274 : Int = _range_id_51271::Start;
                    let _step_id_51279 : Int = _range_id_51271::Step;
                    let _end_id_51284 : Int = _range_id_51271::End;
                    while _step_id_51279 > 0 and _index_id_51274 <= _end_id_51284 or _step_id_51279 < 0 and _index_id_51274 >= _end_id_51284 {
                        let i : Int = _index_id_51274;
                        CNOT(xs[i], ys[i]);
                        _index_id_51274 += _step_id_51279;
                    }

                }

                {
                    let _range_id_51314 : Range = Length(xs) - 2..-1..1;
                    mutable _index_id_51317 : Int = _range_id_51314::Start;
                    let _step_id_51322 : Int = _range_id_51314::Step;
                    let _end_id_51327 : Int = _range_id_51314::End;
                    while _step_id_51322 > 0 and _index_id_51317 <= _end_id_51327 or _step_id_51322 < 0 and _index_id_51317 >= _end_id_51327 {
                        let i : Int = _index_id_51317;
                        CNOT(xs[i], xs[i + 1]);
                        _index_id_51317 += _step_id_51322;
                    }

                }

            }
            adjoint ... {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range : Range = Length(xs) - 2..-1..1;
                    {
                        let _range_id_51357 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_51360 : Int = _range_id_51357::Start;
                        let _step_id_51365 : Int = _range_id_51357::Step;
                        let _end_id_51370 : Int = _range_id_51357::End;
                        while _step_id_51365 > 0 and _index_id_51360 <= _end_id_51370 or _step_id_51365 < 0 and _index_id_51360 >= _end_id_51370 {
                            let i : Int = _index_id_51360;
                            Adjoint CNOT(xs[i], xs[i + 1]);
                            _index_id_51360 += _step_id_51365;
                        }

                    }

                }

                {
                    let _range : Range = 1..Length(xs) - 1;
                    {
                        let _range_id_51400 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_51403 : Int = _range_id_51400::Start;
                        let _step_id_51408 : Int = _range_id_51400::Step;
                        let _end_id_51413 : Int = _range_id_51400::End;
                        while _step_id_51408 > 0 and _index_id_51403 <= _end_id_51413 or _step_id_51408 < 0 and _index_id_51403 >= _end_id_51413 {
                            let i : Int = _index_id_51403;
                            Adjoint CNOT(xs[i], ys[i]);
                            _index_id_51403 += _step_id_51408;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range_id_51443 : Range = 1..Length(xs) - 1;
                    mutable _index_id_51446 : Int = _range_id_51443::Start;
                    let _step_id_51451 : Int = _range_id_51443::Step;
                    let _end_id_51456 : Int = _range_id_51443::End;
                    while _step_id_51451 > 0 and _index_id_51446 <= _end_id_51456 or _step_id_51451 < 0 and _index_id_51446 >= _end_id_51456 {
                        let i : Int = _index_id_51446;
                        Controlled CNOT(ctls, (xs[i], ys[i]));
                        _index_id_51446 += _step_id_51451;
                    }

                }

                {
                    let _range_id_51486 : Range = Length(xs) - 2..-1..1;
                    mutable _index_id_51489 : Int = _range_id_51486::Start;
                    let _step_id_51494 : Int = _range_id_51486::Step;
                    let _end_id_51499 : Int = _range_id_51486::End;
                    while _step_id_51494 > 0 and _index_id_51489 <= _end_id_51499 or _step_id_51494 < 0 and _index_id_51489 >= _end_id_51499 {
                        let i : Int = _index_id_51489;
                        Controlled CNOT(ctls, (xs[i], xs[i + 1]));
                        _index_id_51489 += _step_id_51494;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Fact(Length(xs) <= Length(ys), $"Input register ys must be at least as long as xs.");
                {
                    let _range : Range = Length(xs) - 2..-1..1;
                    {
                        let _range_id_51529 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_51532 : Int = _range_id_51529::Start;
                        let _step_id_51537 : Int = _range_id_51529::Step;
                        let _end_id_51542 : Int = _range_id_51529::End;
                        while _step_id_51537 > 0 and _index_id_51532 <= _end_id_51542 or _step_id_51537 < 0 and _index_id_51532 >= _end_id_51542 {
                            let i : Int = _index_id_51532;
                            Controlled Adjoint CNOT(ctls, (xs[i], xs[i + 1]));
                            _index_id_51532 += _step_id_51537;
                        }

                    }

                }

                {
                    let _range : Range = 1..Length(xs) - 1;
                    {
                        let _range_id_51572 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_51575 : Int = _range_id_51572::Start;
                        let _step_id_51580 : Int = _range_id_51572::Step;
                        let _end_id_51585 : Int = _range_id_51572::End;
                        while _step_id_51580 > 0 and _index_id_51575 <= _end_id_51585 or _step_id_51580 < 0 and _index_id_51575 >= _end_id_51585 {
                            let i : Int = _index_id_51575;
                            Controlled Adjoint CNOT(ctls, (xs[i], ys[i]));
                            _index_id_51575 += _step_id_51580;
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
                    let _range_id_51615 : Range = 0..Length(xs) - 2;
                    mutable _index_id_51618 : Int = _range_id_51615::Start;
                    let _step_id_51623 : Int = _range_id_51615::Step;
                    let _end_id_51628 : Int = _range_id_51615::End;
                    while _step_id_51623 > 0 and _index_id_51618 <= _end_id_51628 or _step_id_51623 < 0 and _index_id_51618 >= _end_id_51628 {
                        let idx : Int = _index_id_51618;
                        CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                        _index_id_51618 += _step_id_51623;
                    }

                }

                {
                    let _range_id_51658 : Range = Length(xs) - 1..-1..1;
                    mutable _index_id_51661 : Int = _range_id_51658::Start;
                    let _step_id_51666 : Int = _range_id_51658::Step;
                    let _end_id_51671 : Int = _range_id_51658::End;
                    while _step_id_51666 > 0 and _index_id_51661 <= _end_id_51671 or _step_id_51666 < 0 and _index_id_51661 >= _end_id_51671 {
                        let idx : Int = _index_id_51661;
                        Controlled CNOT(controls, (xs[idx], ys[idx]));
                        CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                        _index_id_51661 += _step_id_51666;
                    }

                }

            }
            controlled adjoint (controls, ...) {
                Fact(Length(xs) == Length(ys), $"Input registers must have the same number of qubits.");
                {
                    let _range : Range = Length(xs) - 1..-1..1;
                    {
                        let _range_id_51701 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_51704 : Int = _range_id_51701::Start;
                        let _step_id_51709 : Int = _range_id_51701::Step;
                        let _end_id_51714 : Int = _range_id_51701::End;
                        while _step_id_51709 > 0 and _index_id_51704 <= _end_id_51714 or _step_id_51709 < 0 and _index_id_51704 >= _end_id_51714 {
                            let idx : Int = _index_id_51704;
                            Adjoint CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                            Adjoint Controlled CNOT(controls, (xs[idx], ys[idx]));
                            _index_id_51704 += _step_id_51709;
                        }

                    }

                }

                {
                    let _range : Range = 0..Length(xs) - 2;
                    {
                        let _range_id_51744 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_51747 : Int = _range_id_51744::Start;
                        let _step_id_51752 : Int = _range_id_51744::Step;
                        let _end_id_51757 : Int = _range_id_51744::End;
                        while _step_id_51752 > 0 and _index_id_51747 <= _end_id_51757 or _step_id_51752 < 0 and _index_id_51747 >= _end_id_51757 {
                            let idx : Int = _index_id_51747;
                            Adjoint CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                            _index_id_51747 += _step_id_51752;
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
                Fact(Length(xs) + 1 == Length(ys), $"ys must be one qubit longer than xs.");
                Fact(Length(xs) > 0, $"Array should not be empty.");
                let nQubits : Int = Length(xs);
                {
                    let _range_id_51787 : Range = 0..nQubits - 2;
                    mutable _index_id_51790 : Int = _range_id_51787::Start;
                    let _step_id_51795 : Int = _range_id_51787::Step;
                    let _end_id_51800 : Int = _range_id_51787::End;
                    while _step_id_51795 > 0 and _index_id_51790 <= _end_id_51800 or _step_id_51795 < 0 and _index_id_51790 >= _end_id_51800 {
                        let idx : Int = _index_id_51790;
                        CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                        _index_id_51790 += _step_id_51795;
                    }

                }

                Controlled CCNOT(controls, (xs[nQubits - 1], ys[nQubits - 1], ys[nQubits]));
                {
                    let _range_id_51830 : Range = nQubits - 1..-1..1;
                    mutable _index_id_51833 : Int = _range_id_51830::Start;
                    let _step_id_51838 : Int = _range_id_51830::Step;
                    let _end_id_51843 : Int = _range_id_51830::End;
                    while _step_id_51838 > 0 and _index_id_51833 <= _end_id_51843 or _step_id_51838 < 0 and _index_id_51833 >= _end_id_51843 {
                        let idx : Int = _index_id_51833;
                        Controlled CNOT(controls, (xs[idx], ys[idx]));
                        CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                        _index_id_51833 += _step_id_51838;
                    }

                }

            }
            controlled adjoint (controls, ...) {
                Fact(Length(xs) + 1 == Length(ys), $"ys must be one qubit longer than xs.");
                Fact(Length(xs) > 0, $"Array should not be empty.");
                let nQubits : Int = Length(xs);
                {
                    let _range : Range = nQubits - 1..-1..1;
                    {
                        let _range_id_51873 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_51876 : Int = _range_id_51873::Start;
                        let _step_id_51881 : Int = _range_id_51873::Step;
                        let _end_id_51886 : Int = _range_id_51873::End;
                        while _step_id_51881 > 0 and _index_id_51876 <= _end_id_51886 or _step_id_51881 < 0 and _index_id_51876 >= _end_id_51886 {
                            let idx : Int = _index_id_51876;
                            Adjoint CCNOT(xs[idx - 1], ys[idx - 1], xs[idx]);
                            Adjoint Controlled CNOT(controls, (xs[idx], ys[idx]));
                            _index_id_51876 += _step_id_51881;
                        }

                    }

                }

                Adjoint Controlled CCNOT(controls, (xs[nQubits - 1], ys[nQubits - 1], ys[nQubits]));
                {
                    let _range : Range = 0..nQubits - 2;
                    {
                        let _range_id_51916 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_51919 : Int = _range_id_51916::Start;
                        let _step_id_51924 : Int = _range_id_51916::Step;
                        let _end_id_51929 : Int = _range_id_51916::End;
                        while _step_id_51924 > 0 and _index_id_51919 <= _end_id_51929 or _step_id_51924 < 0 and _index_id_51919 >= _end_id_51929 {
                            let idx : Int = _index_id_51919;
                            Adjoint CCNOT(xs[idx], ys[idx], xs[idx + 1]);
                            _index_id_51919 += _step_id_51924;
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
                Fact(c < 2^ysLen, $"Constant `c` must be smaller than 2^Length(ys).");
                if c != 0 {
                    let j : Int = TrailingZeroCountI(c);
                    let x : Qubit[] = AllocateQubitArray(ysLen - j);
                    let _generated_ident_54513 : Unit = {
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
                    _generated_ident_54513
                }

            }
            adjoint ... {
                let ysLen : Int = Length(ys);
                Fact(ysLen > 0, $"Length of `ys` must be at least 1.");
                Fact(c >= 0, $"Constant `c` must be non-negative.");
                Fact(c < 2^ysLen, $"Constant `c` must be smaller than 2^Length(ys).");
                if c != 0 {
                    let j : Int = TrailingZeroCountI(c);
                    let x : Qubit[] = AllocateQubitArray(ysLen - j);
                    let _generated_ident_54527 : Unit = {
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
                    _generated_ident_54527
                }

            }
            controlled (ctls, ...) {
                let ysLen : Int = Length(ys);
                Fact(ysLen > 0, $"Length of `ys` must be at least 1.");
                Fact(c >= 0, $"Constant `c` must be non-negative.");
                Fact(c < 2^ysLen, $"Constant `c` must be smaller than 2^Length(ys).");
                if c != 0 {
                    let j : Int = TrailingZeroCountI(c);
                    let x : Qubit[] = AllocateQubitArray(ysLen - j);
                    let _generated_ident_54541 : Unit = {
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
                    _generated_ident_54541
                }

            }
            controlled adjoint (ctls, ...) {
                let ysLen : Int = Length(ys);
                Fact(ysLen > 0, $"Length of `ys` must be at least 1.");
                Fact(c >= 0, $"Constant `c` must be non-negative.");
                Fact(c < 2^ysLen, $"Constant `c` must be smaller than 2^Length(ys).");
                if c != 0 {
                    let j : Int = TrailingZeroCountI(c);
                    let x : Qubit[] = AllocateQubitArray(ysLen - j);
                    let _generated_ident_54555 : Unit = {
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
                    _generated_ident_54555
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
            if number % 2 == 0 {
                Message($"An even number has been given; 2 is a factor.");
                {
                    __ret_val = (number / 2, 2);
                    __has_returned = true;
                };
            }

            mutable foundFactors : Bool = {
                false
            };
            mutable factors : (Int, Int) = if not __has_returned {
                (1, 1)
            } else {
                (0, 0)
            };
            mutable attempt : Int = if not __has_returned {
                1
            } else {
                0
            };
            if not __has_returned {
                {
                    mutable _continue_cond_1475 : Bool = true;
                    while _continue_cond_1475 {
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

                        _continue_cond_1475 = not foundFactors;
                        if _continue_cond_1475 {
                            Message($"The estimated period did not yield a valid factor. " + $"Trying again.");
                        }

                    }

                }

            };
            if not __has_returned {
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
            if period % 2 == 0 {
                let halfPower : Int = ExpModI(generator, period / 2, modulus);
                if halfPower != modulus - 1 {
                    let factor : Int = MaxI(GreatestCommonDivisorI(halfPower - 1, modulus), GreatestCommonDivisorI(halfPower + 1, modulus));
                    if factor != 1 and factor != modulus {
                        Message($"Found factor={factor}");
                        {
                            __ret_val = (true, (factor, modulus / factor));
                            __has_returned = true;
                        };
                    }

                }

                if not __has_returned {
                    Message($"Found trivial factors.");
                };
                if not __has_returned {
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
            let period : Int = periodAbs * currentDivisor / GreatestCommonDivisorI(currentDivisor, periodAbs);
            Message($"Found period={period}");
            period
        }
        operation EstimatePeriod(generator : Int, modulus : Int) : Int {
            mutable __has_returned : Bool = false;
            mutable __ret_val : Int = 0;
            Fact(GreatestCommonDivisorI(generator, modulus) == 1, $"`generator` and `modulus` must be co-prime");
            let bitsize : Int = BitSizeI(modulus);
            let bitsPrecision : Int = 2 * bitsize + 1;
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
            let bitsPrecision : Int = 2 * bitsize + 1;
            Message($"Estimating frequency with bitsPrecision={bitsPrecision}.");
            let eigenstateRegister : Qubit[] = AllocateQubitArray(bitsize);
            ApplyXorInPlace(1, eigenstateRegister);
            let c : Qubit = __quantum__rt__qubit_allocate();
            {
                let _range_id_1492 : Range = bitsPrecision - 1..-1..0;
                mutable _index_id_1495 : Int = _range_id_1492::Start;
                let _step_id_1500 : Int = _range_id_1492::Step;
                let _end_id_1505 : Int = _range_id_1492::End;
                while _step_id_1500 > 0 and _index_id_1495 <= _end_id_1505 or _step_id_1500 < 0 and _index_id_1495 >= _end_id_1505 {
                    let idx : Int = _index_id_1495;
                    H(c);
                    Controlled ApplyOrderFindingOracle([c], (generator, modulus, 1 <<< idx, eigenstateRegister));
                    R1Frac(frequencyEstimate, bitsPrecision - 1 - idx, c);
                    H(c);
                    __cond_0 = M(c) == One;
                    if __cond_0 {
                        X(c);
                        frequencyEstimate += 1 <<< bitsPrecision - 1 - idx;
                    }

                    _index_id_1495 += _step_id_1500;
                }

            }

            ResetAll(eigenstateRegister);
            Message($"Estimated frequency={frequencyEstimate}");
            {
                let _generated_ident_2061 : Int = frequencyEstimate;
                __quantum__rt__qubit_release(c);
                ReleaseQubitArray(eigenstateRegister);
                {
                    __ret_val = _generated_ident_2061;
                    __has_returned = true;
                };
            };
            if not __has_returned {
                __quantum__rt__qubit_release(c);
            };
            if not __has_returned {
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
                    let _range_id_1535 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1538 : Int = _range_id_1535::Start;
                    let _step_id_1543 : Int = _range_id_1535::Step;
                    let _end_id_1548 : Int = _range_id_1535::End;
                    while _step_id_1543 > 0 and _index_id_1538 <= _end_id_1548 or _step_id_1543 < 0 and _index_id_1538 >= _end_id_1548 {
                        let idx : Int = _index_id_1538;
                        let shiftedC : Int = c <<< idx % modulus;
                        Controlled ModularAddConstant([y[idx]], (modulus, shiftedC, qs));
                        _index_id_1538 += _step_id_1543;
                    }

                }

                {
                    let _range_id_1578 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1581 : Int = _range_id_1578::Start;
                    let _step_id_1586 : Int = _range_id_1578::Step;
                    let _end_id_1591 : Int = _range_id_1578::End;
                    while _step_id_1586 > 0 and _index_id_1581 <= _end_id_1591 or _step_id_1586 < 0 and _index_id_1581 >= _end_id_1591 {
                        let idx : Int = _index_id_1581;
                        SWAP(y[idx], qs[idx]);
                        _index_id_1581 += _step_id_1586;
                    }

                }

                let invC : Int = InverseModI(c, modulus);
                let _generated_ident_2090 : Unit = {
                    let _range_id_1621 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1624 : Int = _range_id_1621::Start;
                    let _step_id_1629 : Int = _range_id_1621::Step;
                    let _end_id_1634 : Int = _range_id_1621::End;
                    while _step_id_1629 > 0 and _index_id_1624 <= _end_id_1634 or _step_id_1629 < 0 and _index_id_1624 >= _end_id_1634 {
                        let idx : Int = _index_id_1624;
                        let shiftedC : Int = invC <<< idx % modulus;
                        Controlled ModularAddConstant([y[idx]], (modulus, modulus - shiftedC, qs));
                        _index_id_1624 += _step_id_1629;
                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_2090
            }
            adjoint ... {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                let invC : Int = InverseModI(c, modulus);
                {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_1664 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_1667 : Int = _range_id_1664::Start;
                        let _step_id_1672 : Int = _range_id_1664::Step;
                        let _end_id_1677 : Int = _range_id_1664::End;
                        while _step_id_1672 > 0 and _index_id_1667 <= _end_id_1677 or _step_id_1672 < 0 and _index_id_1667 >= _end_id_1677 {
                            let idx : Int = _index_id_1667;
                            let shiftedC : Int = invC <<< idx % modulus;
                            Controlled Adjoint ModularAddConstant([y[idx]], (modulus, modulus - shiftedC, qs));
                            _index_id_1667 += _step_id_1672;
                        }

                    }

                }

                {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_1707 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_1710 : Int = _range_id_1707::Start;
                        let _step_id_1715 : Int = _range_id_1707::Step;
                        let _end_id_1720 : Int = _range_id_1707::End;
                        while _step_id_1715 > 0 and _index_id_1710 <= _end_id_1720 or _step_id_1715 < 0 and _index_id_1710 >= _end_id_1720 {
                            let idx : Int = _index_id_1710;
                            Adjoint SWAP(y[idx], qs[idx]);
                            _index_id_1710 += _step_id_1715;
                        }

                    }

                }

                let _generated_ident_2104 : Unit = {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_1750 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_1753 : Int = _range_id_1750::Start;
                        let _step_id_1758 : Int = _range_id_1750::Step;
                        let _end_id_1763 : Int = _range_id_1750::End;
                        while _step_id_1758 > 0 and _index_id_1753 <= _end_id_1763 or _step_id_1758 < 0 and _index_id_1753 >= _end_id_1763 {
                            let idx : Int = _index_id_1753;
                            let shiftedC : Int = c <<< idx % modulus;
                            Controlled Adjoint ModularAddConstant([y[idx]], (modulus, shiftedC, qs));
                            _index_id_1753 += _step_id_1758;
                        }

                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_2104
            }
            controlled (ctls, ...) {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                {
                    let _range_id_1793 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1796 : Int = _range_id_1793::Start;
                    let _step_id_1801 : Int = _range_id_1793::Step;
                    let _end_id_1806 : Int = _range_id_1793::End;
                    while _step_id_1801 > 0 and _index_id_1796 <= _end_id_1806 or _step_id_1801 < 0 and _index_id_1796 >= _end_id_1806 {
                        let idx : Int = _index_id_1796;
                        let shiftedC : Int = c <<< idx % modulus;
                        Controlled Controlled ModularAddConstant(ctls, ([y[idx]], (modulus, shiftedC, qs)));
                        _index_id_1796 += _step_id_1801;
                    }

                }

                {
                    let _range_id_1836 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1839 : Int = _range_id_1836::Start;
                    let _step_id_1844 : Int = _range_id_1836::Step;
                    let _end_id_1849 : Int = _range_id_1836::End;
                    while _step_id_1844 > 0 and _index_id_1839 <= _end_id_1849 or _step_id_1844 < 0 and _index_id_1839 >= _end_id_1849 {
                        let idx : Int = _index_id_1839;
                        Controlled SWAP(ctls, (y[idx], qs[idx]));
                        _index_id_1839 += _step_id_1844;
                    }

                }

                let invC : Int = InverseModI(c, modulus);
                let _generated_ident_2118 : Unit = {
                    let _range_id_1879 : Range = IndexRange_Qubit_(y);
                    mutable _index_id_1882 : Int = _range_id_1879::Start;
                    let _step_id_1887 : Int = _range_id_1879::Step;
                    let _end_id_1892 : Int = _range_id_1879::End;
                    while _step_id_1887 > 0 and _index_id_1882 <= _end_id_1892 or _step_id_1887 < 0 and _index_id_1882 >= _end_id_1892 {
                        let idx : Int = _index_id_1882;
                        let shiftedC : Int = invC <<< idx % modulus;
                        Controlled Controlled ModularAddConstant(ctls, ([y[idx]], (modulus, modulus - shiftedC, qs)));
                        _index_id_1882 += _step_id_1887;
                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_2118
            }
            controlled adjoint (ctls, ...) {
                let qs : Qubit[] = AllocateQubitArray(Length(y));
                let invC : Int = InverseModI(c, modulus);
                {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_1922 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_1925 : Int = _range_id_1922::Start;
                        let _step_id_1930 : Int = _range_id_1922::Step;
                        let _end_id_1935 : Int = _range_id_1922::End;
                        while _step_id_1930 > 0 and _index_id_1925 <= _end_id_1935 or _step_id_1930 < 0 and _index_id_1925 >= _end_id_1935 {
                            let idx : Int = _index_id_1925;
                            let shiftedC : Int = invC <<< idx % modulus;
                            Controlled Controlled Adjoint ModularAddConstant(ctls, ([y[idx]], (modulus, modulus - shiftedC, qs)));
                            _index_id_1925 += _step_id_1930;
                        }

                    }

                }

                {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_1965 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_1968 : Int = _range_id_1965::Start;
                        let _step_id_1973 : Int = _range_id_1965::Step;
                        let _end_id_1978 : Int = _range_id_1965::End;
                        while _step_id_1973 > 0 and _index_id_1968 <= _end_id_1978 or _step_id_1973 < 0 and _index_id_1968 >= _end_id_1978 {
                            let idx : Int = _index_id_1968;
                            Controlled Adjoint SWAP(ctls, (y[idx], qs[idx]));
                            _index_id_1968 += _step_id_1973;
                        }

                    }

                }

                let _generated_ident_2132 : Unit = {
                    let _range : Range = IndexRange_Qubit_(y);
                    {
                        let _range_id_2008 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_2011 : Int = _range_id_2008::Start;
                        let _step_id_2016 : Int = _range_id_2008::Step;
                        let _end_id_2021 : Int = _range_id_2008::End;
                        while _step_id_2016 > 0 and _index_id_2011 <= _end_id_2021 or _step_id_2016 < 0 and _index_id_2011 >= _end_id_2021 {
                            let idx : Int = _index_id_2011;
                            let shiftedC : Int = c <<< idx % modulus;
                            Controlled Controlled Adjoint ModularAddConstant(ctls, ([y[idx]], (modulus, shiftedC, qs)));
                            _index_id_2011 += _step_id_2016;
                        }

                    }

                };
                ReleaseQubitArray(qs);
                _generated_ident_2132
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
                    let _generated_ident_2146 : Unit = {
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
                    _generated_ident_2146
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
                    let _generated_ident_2169 : Unit = {
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
                    _generated_ident_2169
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
                    if not invertControl {
                        X(target);
                    }

                } else if c >= 2L^bitWidth {
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
                    let _generated_ident_54684 : Unit = {
                        {
                            {
                                let _range_id_52632 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_52635 : Int = _range_id_52632::Start;
                                let _step_id_52640 : Int = _range_id_52632::Step;
                                let _end_id_52645 : Int = _range_id_52632::End;
                                while _step_id_52640 > 0 and _index_id_52635 <= _end_id_52645 or _step_id_52640 < 0 and _index_id_52635 >= _end_id_52645 {
                                    let i : Int = _index_id_52635;
                                    if cNormalized &&& 1L <<< i + 1 != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_52635 += _step_id_52640;
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

                                let _apply_res : Unit = {
                                    Controlled X([control], target);
                                };
                                {
                                    if invertControl {
                                        Adjoint X(control);
                                    }

                                }

                                _apply_res
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(cs1) - 1;
                                {
                                    let _range_id_52675 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                                    mutable _index_id_52678 : Int = _range_id_52675::Start;
                                    let _step_id_52683 : Int = _range_id_52675::Step;
                                    let _end_id_52688 : Int = _range_id_52675::End;
                                    while _step_id_52683 > 0 and _index_id_52678 <= _end_id_52688 or _step_id_52683 < 0 and _index_id_52678 >= _end_id_52688 {
                                        let i : Int = _index_id_52678;
                                        if cNormalized &&& 1L <<< i + 1 != 0L {
                                            Adjoint AND(cs1[i], xNormalized[i + 1], qs[i])
                                        } else {
                                            Adjoint ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                        };
                                        _index_id_52678 += _step_id_52683;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_54684
                }

            }
            adjoint ... {
                let bitWidth : Int = Length(x);
                if c == 0L {
                    if not invertControl {
                        Adjoint X(target);
                    }

                } else if c >= 2L^bitWidth {
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
                    let _generated_ident_54698 : Unit = {
                        {
                            {
                                let _range_id_52718 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_52721 : Int = _range_id_52718::Start;
                                let _step_id_52726 : Int = _range_id_52718::Step;
                                let _end_id_52731 : Int = _range_id_52718::End;
                                while _step_id_52726 > 0 and _index_id_52721 <= _end_id_52731 or _step_id_52726 < 0 and _index_id_52721 >= _end_id_52731 {
                                    let i : Int = _index_id_52721;
                                    if cNormalized &&& 1L <<< i + 1 != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_52721 += _step_id_52726;
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

                                let _apply_res : Unit = {
                                    Controlled Adjoint X([control], target);
                                };
                                {
                                    if invertControl {
                                        Adjoint X(control);
                                    }

                                }

                                _apply_res
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(cs1) - 1;
                                {
                                    let _range_id_52761 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                                    mutable _index_id_52764 : Int = _range_id_52761::Start;
                                    let _step_id_52769 : Int = _range_id_52761::Step;
                                    let _end_id_52774 : Int = _range_id_52761::End;
                                    while _step_id_52769 > 0 and _index_id_52764 <= _end_id_52774 or _step_id_52769 < 0 and _index_id_52764 >= _end_id_52774 {
                                        let i : Int = _index_id_52764;
                                        if cNormalized &&& 1L <<< i + 1 != 0L {
                                            Adjoint AND(cs1[i], xNormalized[i + 1], qs[i])
                                        } else {
                                            Adjoint ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                        };
                                        _index_id_52764 += _step_id_52769;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_54698
                }

            }
            controlled (ctls, ...) {
                let bitWidth : Int = Length(x);
                if c == 0L {
                    if not invertControl {
                        Controlled X(ctls, target);
                    }

                } else if c >= 2L^bitWidth {
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
                    let _generated_ident_54712 : Unit = {
                        {
                            {
                                let _range_id_52804 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_52807 : Int = _range_id_52804::Start;
                                let _step_id_52812 : Int = _range_id_52804::Step;
                                let _end_id_52817 : Int = _range_id_52804::End;
                                while _step_id_52812 > 0 and _index_id_52807 <= _end_id_52817 or _step_id_52812 < 0 and _index_id_52807 >= _end_id_52817 {
                                    let i : Int = _index_id_52807;
                                    if cNormalized &&& 1L <<< i + 1 != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_52807 += _step_id_52812;
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

                                let _apply_res : Unit = {
                                    Controlled Controlled X(ctls, ([control], target));
                                };
                                {
                                    if invertControl {
                                        Adjoint X(control);
                                    }

                                }

                                _apply_res
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(cs1) - 1;
                                {
                                    let _range_id_52847 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                                    mutable _index_id_52850 : Int = _range_id_52847::Start;
                                    let _step_id_52855 : Int = _range_id_52847::Step;
                                    let _end_id_52860 : Int = _range_id_52847::End;
                                    while _step_id_52855 > 0 and _index_id_52850 <= _end_id_52860 or _step_id_52855 < 0 and _index_id_52850 >= _end_id_52860 {
                                        let i : Int = _index_id_52850;
                                        if cNormalized &&& 1L <<< i + 1 != 0L {
                                            Adjoint AND(cs1[i], xNormalized[i + 1], qs[i])
                                        } else {
                                            Adjoint ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                        };
                                        _index_id_52850 += _step_id_52855;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_54712
                }

            }
            controlled adjoint (ctls, ...) {
                let bitWidth : Int = Length(x);
                if c == 0L {
                    if not invertControl {
                        Controlled Adjoint X(ctls, target);
                    }

                } else if c >= 2L^bitWidth {
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
                    let _generated_ident_54726 : Unit = {
                        {
                            {
                                let _range_id_52890 : Range = 0..Length(cs1) - 1;
                                mutable _index_id_52893 : Int = _range_id_52890::Start;
                                let _step_id_52898 : Int = _range_id_52890::Step;
                                let _end_id_52903 : Int = _range_id_52890::End;
                                while _step_id_52898 > 0 and _index_id_52893 <= _end_id_52903 or _step_id_52898 < 0 and _index_id_52893 >= _end_id_52903 {
                                    let i : Int = _index_id_52893;
                                    if cNormalized &&& 1L <<< i + 1 != 0L {
                                        AND(cs1[i], xNormalized[i + 1], qs[i])
                                    } else {
                                        ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                    };
                                    _index_id_52893 += _step_id_52898;
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

                                let _apply_res : Unit = {
                                    Controlled Controlled Adjoint X(ctls, ([control], target));
                                };
                                {
                                    if invertControl {
                                        Adjoint X(control);
                                    }

                                }

                                _apply_res
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(cs1) - 1;
                                {
                                    let _range_id_52933 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                                    mutable _index_id_52936 : Int = _range_id_52933::Start;
                                    let _step_id_52941 : Int = _range_id_52933::Step;
                                    let _end_id_52946 : Int = _range_id_52933::End;
                                    while _step_id_52941 > 0 and _index_id_52936 <= _end_id_52946 or _step_id_52941 < 0 and _index_id_52936 >= _end_id_52946 {
                                        let i : Int = _index_id_52936;
                                        if cNormalized &&& 1L <<< i + 1 != 0L {
                                            Adjoint AND(cs1[i], xNormalized[i + 1], qs[i])
                                        } else {
                                            Adjoint ApplyOrAssuming0Target(cs1[i], xNormalized[i + 1], qs[i])
                                        };
                                        _index_id_52936 += _step_id_52941;
                                    }

                                }

                            }

                        }

                        _apply_res
                    };
                    ReleaseQubitArray(qs);
                    _generated_ident_54726
                }

            }
        }
        // entry
        Main()"#]].assert_eq(&rendered);
}
