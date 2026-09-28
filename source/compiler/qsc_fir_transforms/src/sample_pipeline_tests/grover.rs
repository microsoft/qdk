// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::PipelineStage;
use crate::pretty::write_reachable_qsharp_parseable;
use crate::test_utils::compile_and_run_pipeline_to;

const GROVER_SOURCE: &str = include_str!("../../../../../samples/algorithms/Grover.qs");

#[test]
#[allow(clippy::too_many_lines)]
fn grover_sample_full_pipeline_reachable_items() {
    let (store, pkg_id) = compile_and_run_pipeline_to(GROVER_SOURCE, PipelineStage::Full);
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
        function IntAsDouble(number : Int) : Double {
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
                    let _range_id_7 : Range = 0..2..Length(ctls) - 2;
                    mutable _index_id_8 : Int = _range_id_7.Start;
                    let _step_id_9 : Int = _range_id_7.Step;
                    let _end_id_10 : Int = _range_id_7.End;
                    while ((_step_id_9 > 0) and (_index_id_8 <= _end_id_10)) or ((_step_id_9 < 0) and (_index_id_8 >= _end_id_10)) {
                        let i : Int = _index_id_8;
                        CCNOT(ctls[i], ctls[i + 1], aux[i / 2]);
                        _index_id_8 += _step_id_9;
                    }

                }

                {
                    let _range_id_11 : Range = 0..((Length(ctls) / 2) - 2) - adjustment;
                    mutable _index_id_12 : Int = _range_id_11.Start;
                    let _step_id_13 : Int = _range_id_11.Step;
                    let _end_id_14 : Int = _range_id_11.End;
                    while ((_step_id_13 > 0) and (_index_id_12 <= _end_id_14)) or ((_step_id_13 < 0) and (_index_id_12 >= _end_id_14)) {
                        let i_1 : Int = _index_id_12;
                        CCNOT(aux[i_1 * 2], aux[(i_1 * 2) + 1], aux[i_1 + (Length(ctls) / 2)]);
                        _index_id_12 += _step_id_13;
                    }

                }

            }
            adjoint ... {
                {
                    let _range : Range = 0..((Length(ctls) / 2) - 2) - adjustment;
                    {
                        let _range_id_15 : Range = _range.Start + (((_range.End - _range.Start) / _range.Step) * _range.Step)..(-_range.Step).._range.Start;
                        mutable _index_id_16 : Int = _range_id_15.Start;
                        let _step_id_17 : Int = _range_id_15.Step;
                        let _end_id_18 : Int = _range_id_15.End;
                        while ((_step_id_17 > 0) and (_index_id_16 <= _end_id_18)) or ((_step_id_17 < 0) and (_index_id_16 >= _end_id_18)) {
                            let i : Int = _index_id_16;
                            Adjoint CCNOT(aux[i * 2], aux[(i * 2) + 1], aux[i + (Length(ctls) / 2)]);
                            _index_id_16 += _step_id_17;
                        }

                    }

                }

                {
                    let _range_1 : Range = 0..2..Length(ctls) - 2;
                    {
                        let _range_id_19 : Range = _range_1.Start + (((_range_1.End - _range_1.Start) / _range_1.Step) * _range_1.Step)..(-_range_1.Step).._range_1.Start;
                        mutable _index_id_20 : Int = _range_id_19.Start;
                        let _step_id_21 : Int = _range_id_19.Step;
                        let _end_id_22 : Int = _range_id_19.End;
                        while ((_step_id_21 > 0) and (_index_id_20 <= _end_id_22)) or ((_step_id_21 < 0) and (_index_id_20 >= _end_id_22)) {
                            let i_1 : Int = _index_id_20;
                            Adjoint CCNOT(ctls[i_1], ctls[i_1 + 1], aux[i_1 / 2]);
                            _index_id_20 += _step_id_21;
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
        operation CCZ(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit is Adj {
            body ... {
                {
                    {
                        MapPauliAxis(PauliX, PauliZ, target);
                    }

                    let _apply_res : Unit = {
                        CCNOT(control1, control2, target);
                    };
                    {
                        Adjoint MapPauliAxis(PauliX, PauliZ, target);
                    }

                    _apply_res
                }

            }
            adjoint ... {
                {
                    {
                        MapPauliAxis(PauliX, PauliZ, target);
                    }

                    let _apply_res : Unit = {
                        Adjoint CCNOT(control1, control2, target);
                    };
                    {
                        Adjoint MapPauliAxis(PauliX, PauliZ, target);
                    }

                    _apply_res
                }

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
                            let _generated_ident_23 : Unit = {
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
                            _generated_ident_23
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
                            let _generated_ident_24 : Unit = {
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
                            _generated_ident_24
                        }

                    }

                }

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
                        let _generated_ident_25 : Unit = {
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
                        _generated_ident_25
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
                            let _generated_ident_26 : Unit = {
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
                            _generated_ident_26
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
                            let _generated_ident_27 : Unit = {
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
                            _generated_ident_27
                        }

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
                        let _generated_ident_28 : Unit = {
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
                        _generated_ident_28
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
                        let _generated_ident_29 : Unit = {
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
                        _generated_ident_29
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
                            let _generated_ident_30 : Unit = {
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
                            _generated_ident_30
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
                            let _generated_ident_31 : Unit = {
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
                            _generated_ident_31
                        }

                    }

                }

            }
        }
        operation Z(qubit : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__z__body(qubit);
            }
            adjoint ... {
                __quantum__qis__z__body(qubit);
            }
            controlled (ctls, ...) {
                mutable __cond_3 : Bool = false;
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                mutable __cond_2 : Bool = false;
                if __cond_0 {
                    __quantum__qis__z__body(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        __quantum__qis__cz__body(ctls[0], qubit);
                    } else {
                        __cond_2 = Length(ctls) == 2;
                        if __cond_2 {
                            CCZ(ctls[0], ctls[1], qubit);
                        } else {
                            let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 2);
                            let _generated_ident_32 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = (Length(ctls) % 2) != 0;
                                    if __cond_3 {
                                        CCZ(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                                    } else {
                                        CCZ(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_32
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
                    __quantum__qis__z__body(qubit);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        __quantum__qis__cz__body(ctls[0], qubit);
                    } else {
                        __cond_2 = Length(ctls) == 2;
                        if __cond_2 {
                            CCZ(ctls[0], ctls[1], qubit);
                        } else {
                            let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 2);
                            let _generated_ident_33 : Unit = {
                                {
                                    CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                let _apply_res : Unit = {
                                    __cond_3 = (Length(ctls) % 2) != 0;
                                    if __cond_3 {
                                        CCZ(ctls[Length(ctls) - 1], aux[Length(ctls) - 3], qubit);
                                    } else {
                                        CCZ(aux[Length(ctls) - 3], aux[Length(ctls) - 4], qubit);
                                    }

                                };
                                {
                                    Adjoint CollectControls(ctls, aux, 1 - (Length(ctls) % 2));
                                }

                                _apply_res
                            };
                            ReleaseQubitArray(aux);
                            _generated_ident_33
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
        function AbsD(a : Double) : Double {
            if a < 0. {
                (-a)
            } else {
                a
            }
        }
        function ArcSin(y : Double) : Double {
            body intrinsic;
        }
        function Sqrt(d : Double) : Double {
            body intrinsic;
        }
        function Truncate(value : Double) : Int {
            body intrinsic;
        }
        function ExtendedTruncation(value : Double) : (Int, Double, Bool) {
            let truncated : Int = Truncate(value);
            (truncated, IntAsDouble(truncated) - value, value >= 0.)
        }
        function Round(value : Double) : Int {
            let (truncated : Int, remainder : Double, isPositive : Bool) = ExtendedTruncation(value);
            let abs : Double = AbsD(remainder);
            truncated + if abs <= 0.5 {
                0
            } else if isPositive {
                1
            } else {
                (-1)
            }
        }
        operation MResetEachZ(register : Qubit[]) : Result[] {
            mutable results : Result[] = [];
            {
                let _array_id_34 : Qubit[] = register;
                let _len_id_35 : Int = Length(_array_id_34);
                mutable _index_id_36 : Int = 0;
                while _index_id_36 < _len_id_35 {
                    let qubit : Qubit = _array_id_34[_index_id_36];
                    results += [MResetZ(qubit)];
                    _index_id_36 += 1;
                }

            }

            results
        }
        operation MResetZ(target : Qubit) : Result {
            __quantum__qis__mresetz__body(target)
        }
        operation __quantum__qis__ccx__body(control1 : Qubit, control2 : Qubit, target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__cx__body(control : Qubit, target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__cz__body(control : Qubit, target : Qubit) : Unit {
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
        operation __quantum__qis__z__body(target : Qubit) : Unit {
            body intrinsic;
        }
        operation __quantum__qis__mresetz__body(target : Qubit) : Result {
            body intrinsic;
        }
        function Most_Qubit_(array : Qubit[]) : Qubit[] {
            array[...Length(array) - 2]
        }
        function Tail_Qubit_(array : Qubit[]) : Qubit {
            let size : Int = Length(array);
            Fact(size > 0, $"Array must have at least 1 element");
            array[size - 1]
        }
        // package 2
        operation Main() : Result[] {
            let nQubits : Int = 5;
            let nIterations : Int = IterationsToMarked(nQubits);
            Message($"Number of iterations: {nIterations}");
            let results : Result[] = GroverSearch_Empty__ReflectAboutMarked_(nQubits, nIterations);
            results
        }
        function IterationsToMarked(nQubits : Int) : Int {
            if nQubits > 126 {
                fail $"This sample supports at most 126 qubits.";
            }

            let nItems : Double = 2.^IntAsDouble(nQubits);
            let angle : Double = ArcSin(1. / Sqrt(nItems));
            let iterations : Int = Round(((0.25 * PI()) / angle) - 0.5);
            iterations
        }
        operation ReflectAboutMarked(inputQubits : Qubit[]) : Unit {
            Message($"Reflecting about marked state...");
            let outputQubit : Qubit = __quantum__rt__qubit_allocate();
            let _generated_ident_37 : Unit = {
                {
                    X(outputQubit);
                    H(outputQubit);
                    {
                        let _array_id_38 : Qubit[] = inputQubits[...2...];
                        let _len_id_39 : Int = Length(_array_id_38);
                        mutable _index_id_40 : Int = 0;
                        while _index_id_40 < _len_id_39 {
                            let q : Qubit = _array_id_38[_index_id_40];
                            X(q);
                            _index_id_40 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    Controlled X(inputQubits, outputQubit);
                };
                {
                    {
                        let _array : Qubit[] = inputQubits[...2...];
                        {
                            let _range_id_41 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_42 : Int = _range_id_41.Start;
                            let _step_id_43 : Int = _range_id_41.Step;
                            let _end_id_44 : Int = _range_id_41.End;
                            while ((_step_id_43 > 0) and (_index_id_42 <= _end_id_44)) or ((_step_id_43 < 0) and (_index_id_42 >= _end_id_44)) {
                                let _index : Int = _index_id_42;
                                let q_1 : Qubit = _array[_index];
                                Adjoint X(q_1);
                                _index_id_42 += _step_id_43;
                            }

                        }

                    }

                    Adjoint H(outputQubit);
                    Adjoint X(outputQubit);
                }

                _apply_res
            };
            __quantum__rt__qubit_release(outputQubit);
            _generated_ident_37
        }
        operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_45 : Qubit[] = inputQubits;
                    let _len_id_46 : Int = Length(_array_id_45);
                    mutable _index_id_47 : Int = 0;
                    while _index_id_47 < _len_id_46 {
                        let q : Qubit = _array_id_45[_index_id_47];
                        H(q);
                        _index_id_47 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = inputQubits;
                    {
                        let _range_id_48 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_49 : Int = _range_id_48.Start;
                        let _step_id_50 : Int = _range_id_48.Step;
                        let _end_id_51 : Int = _range_id_48.End;
                        while ((_step_id_50 > 0) and (_index_id_49 <= _end_id_51)) or ((_step_id_50 < 0) and (_index_id_49 >= _end_id_51)) {
                            let _index : Int = _index_id_49;
                            let q : Qubit = _array[_index];
                            Adjoint H(q);
                            _index_id_49 += _step_id_50;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_52 : Qubit[] = inputQubits;
                    let _len_id_53 : Int = Length(_array_id_52);
                    mutable _index_id_54 : Int = 0;
                    while _index_id_54 < _len_id_53 {
                        let q : Qubit = _array_id_52[_index_id_54];
                        Controlled H(ctls, q);
                        _index_id_54 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = inputQubits;
                    {
                        let _range_id_55 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_56 : Int = _range_id_55.Start;
                        let _step_id_57 : Int = _range_id_55.Step;
                        let _end_id_58 : Int = _range_id_55.End;
                        while ((_step_id_57 > 0) and (_index_id_56 <= _end_id_58)) or ((_step_id_57 < 0) and (_index_id_56 >= _end_id_58)) {
                            let _index : Int = _index_id_56;
                            let q : Qubit = _array[_index];
                            Controlled Adjoint H(ctls, q);
                            _index_id_56 += _step_id_57;
                        }

                    }

                }

            }
        }
        operation ReflectAboutAllOnes(inputQubits : Qubit[]) : Unit {
            Controlled Z(Most_Qubit_(inputQubits), Tail_Qubit_(inputQubits));
        }
        operation ReflectAboutUniform(inputQubits : Qubit[]) : Unit {
            {
                {
                    Adjoint PrepareUniform(inputQubits);
                    {
                        let _array_id_59 : Qubit[] = inputQubits;
                        let _len_id_60 : Int = Length(_array_id_59);
                        mutable _index_id_61 : Int = 0;
                        while _index_id_61 < _len_id_60 {
                            let q : Qubit = _array_id_59[_index_id_61];
                            X(q);
                            _index_id_61 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    ReflectAboutAllOnes(inputQubits);
                };
                {
                    {
                        let _array : Qubit[] = inputQubits;
                        {
                            let _range_id_62 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_63 : Int = _range_id_62.Start;
                            let _step_id_64 : Int = _range_id_62.Step;
                            let _end_id_65 : Int = _range_id_62.End;
                            while ((_step_id_64 > 0) and (_index_id_63 <= _end_id_65)) or ((_step_id_64 < 0) and (_index_id_63 >= _end_id_65)) {
                                let _index : Int = _index_id_63;
                                let q_1 : Qubit = _array[_index];
                                Adjoint X(q_1);
                                _index_id_63 += _step_id_64;
                            }

                        }

                    }

                    PrepareUniform(inputQubits);
                }

                _apply_res
            }

        }
        operation GroverSearch_Empty__ReflectAboutMarked_(nQubits : Int, iterations : Int) : Result[] {
            mutable __has_returned : Bool = false;
            mutable __ret_val : Result[] = [];
            let qubits : Qubit[] = AllocateQubitArray(nQubits);
            PrepareUniform(qubits);
            {
                let _range_id_66 : Range = 1..iterations;
                mutable _index_id_67 : Int = _range_id_66.Start;
                let _step_id_68 : Int = _range_id_66.Step;
                let _end_id_69 : Int = _range_id_66.End;
                while ((_step_id_68 > 0) and (_index_id_67 <= _end_id_69)) or ((_step_id_68 < 0) and (_index_id_67 >= _end_id_69)) {
                    let _ : Int = _index_id_67;
                    ReflectAboutMarked(qubits);
                    ReflectAboutUniform(qubits);
                    _index_id_67 += _step_id_68;
                }

            }

            {
                let _generated_ident_70 : Result[] = MResetEachZ(qubits);
                ReleaseQubitArray(qubits);
                {
                    __ret_val = _generated_ident_70;
                    __has_returned = true;
                };
            };
            if (not __has_returned) {
                ReleaseQubitArray(qubits);
            };
            __ret_val
        }
        // entry
        Main()"#]].assert_eq(&rendered);
}
