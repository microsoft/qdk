// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use expect_test::expect;

use crate::PipelineStage;
use crate::pretty::write_reachable_qsharp_parseable;
use crate::test_utils::compile_and_run_pipeline_to;

const DYNAMICS_SOURCE: &str = include_str!("../../../../../samples/estimation/Dynamics.qs");

#[test]
#[allow(clippy::too_many_lines)]
fn dynamics_sample_full_pipeline_reachable_items() {
    let (store, pkg_id) = compile_and_run_pipeline_to(DYNAMICS_SOURCE, PipelineStage::Full);
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
        function Length(a : (Bool, Bool)[]) : Int {
            body intrinsic;
        }
        function Length(a : (Qubit, Qubit)[]) : Int {
            body intrinsic;
        }
        function Length(a : Pauli[]) : Int {
            body intrinsic;
        }
        function Length(a : (Qubit, Qubit[])[]) : Int {
            body intrinsic;
        }
        function Length(a : Int[]) : Int {
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
        function IntAsDouble(number : Int) : Double {
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
                    let _range_id_48882 : Range = 0..2..Length(ctls) - 2;
                    mutable _index_id_48885 : Int = _range_id_48882::Start;
                    let _step_id_48890 : Int = _range_id_48882::Step;
                    let _end_id_48895 : Int = _range_id_48882::End;
                    while _step_id_48890 > 0 and _index_id_48885 <= _end_id_48895 or _step_id_48890 < 0 and _index_id_48885 >= _end_id_48895 {
                        let i : Int = _index_id_48885;
                        CCNOT(ctls[i], ctls[i + 1], aux[i / 2]);
                        _index_id_48885 += _step_id_48890;
                    }

                }

                {
                    let _range_id_48925 : Range = 0..Length(ctls) / 2 - 2 - adjustment;
                    mutable _index_id_48928 : Int = _range_id_48925::Start;
                    let _step_id_48933 : Int = _range_id_48925::Step;
                    let _end_id_48938 : Int = _range_id_48925::End;
                    while _step_id_48933 > 0 and _index_id_48928 <= _end_id_48938 or _step_id_48933 < 0 and _index_id_48928 >= _end_id_48938 {
                        let i : Int = _index_id_48928;
                        CCNOT(aux[i * 2], aux[i * 2 + 1], aux[i + Length(ctls) / 2]);
                        _index_id_48928 += _step_id_48933;
                    }

                }

            }
            adjoint ... {
                {
                    let _range : Range = 0..Length(ctls) / 2 - 2 - adjustment;
                    {
                        let _range_id_48968 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_48971 : Int = _range_id_48968::Start;
                        let _step_id_48976 : Int = _range_id_48968::Step;
                        let _end_id_48981 : Int = _range_id_48968::End;
                        while _step_id_48976 > 0 and _index_id_48971 <= _end_id_48981 or _step_id_48976 < 0 and _index_id_48971 >= _end_id_48981 {
                            let i : Int = _index_id_48971;
                            Adjoint CCNOT(aux[i * 2], aux[i * 2 + 1], aux[i + Length(ctls) / 2]);
                            _index_id_48971 += _step_id_48976;
                        }

                    }

                }

                {
                    let _range : Range = 0..2..Length(ctls) - 2;
                    {
                        let _range_id_49011 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_49014 : Int = _range_id_49011::Start;
                        let _step_id_49019 : Int = _range_id_49011::Step;
                        let _end_id_49024 : Int = _range_id_49011::End;
                        while _step_id_49019 > 0 and _index_id_49014 <= _end_id_49024 or _step_id_49019 < 0 and _index_id_49014 >= _end_id_49024 {
                            let i : Int = _index_id_49014;
                            Adjoint CCNOT(ctls[i], ctls[i + 1], aux[i / 2]);
                            _index_id_49014 += _step_id_49019;
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
        operation CRxx(control : Qubit, theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit {
            {
                {
                    MapPauliAxis(PauliZ, PauliX, qubit0);
                    MapPauliAxis(PauliZ, PauliX, qubit1);
                }

                let _apply_res : Unit = {
                    CRzz(control, theta, qubit0, qubit1);
                };
                {
                    Adjoint MapPauliAxis(PauliZ, PauliX, qubit1);
                    Adjoint MapPauliAxis(PauliZ, PauliX, qubit0);
                }

                _apply_res
            }

        }
        operation CRyy(control : Qubit, theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit {
            {
                {
                    MapPauliAxis(PauliZ, PauliY, qubit0);
                    MapPauliAxis(PauliZ, PauliY, qubit1);
                }

                let _apply_res : Unit = {
                    CRzz(control, theta, qubit0, qubit1);
                };
                {
                    Adjoint MapPauliAxis(PauliZ, PauliY, qubit1);
                    Adjoint MapPauliAxis(PauliZ, PauliY, qubit0);
                }

                _apply_res
            }

        }
        operation CRzz(control : Qubit, theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit {
            {
                {
                    CNOT(qubit1, qubit0);
                }

                let _apply_res : Unit = {
                    Controlled Rz([control], (theta, qubit0));
                };
                {
                    Adjoint CNOT(qubit1, qubit0);
                }

                _apply_res
            }

        }
        function IndicesOfNonIdentity(paulies : Pauli[]) : Int[] {
            mutable indices : Int[] = [];
            {
                let _range_id_49054 : Range = 0..Length(paulies) - 1;
                mutable _index_id_49057 : Int = _range_id_49054::Start;
                let _step_id_49062 : Int = _range_id_49054::Step;
                let _end_id_49067 : Int = _range_id_49054::End;
                while _step_id_49062 > 0 and _index_id_49057 <= _end_id_49067 or _step_id_49062 < 0 and _index_id_49057 >= _end_id_49067 {
                    let i : Int = _index_id_49057;
                    if paulies[i] != PauliI {
                        indices += [i];
                    }

                    _index_id_49057 += _step_id_49062;
                }

            }

            indices
        }
        function RemovePauliI(paulis : Pauli[], qubits : Qubit[]) : (Pauli[], Qubit[]) {
            let indices : Int[] = IndicesOfNonIdentity(paulis);
            let newPaulis : Pauli[] = Subarray_Pauli_(indices, paulis);
            let newQubits : Qubit[] = Subarray_Qubit_(indices, qubits);
            (newPaulis, newQubits)
        }
        operation SpreadZ(from : Qubit, to : Qubit[]) : Unit is Adj {
            body ... {
                let targets : (Qubit, Qubit)[] = GetSpread(from, to);
                {
                    let _array_id_49097 : (Qubit, Qubit)[] = targets;
                    let _len_id_49101 : Int = Length(_array_id_49097);
                    mutable _index_id_49106 : Int = 0;
                    while _index_id_49106 < _len_id_49101 {
                        let (ctl : Qubit, tgt : Qubit) = _array_id_49097[_index_id_49106];
                        CNOT(ctl, tgt);
                        _index_id_49106 += 1;
                    }

                }

            }
            adjoint ... {
                let targets : (Qubit, Qubit)[] = GetSpread(from, to);
                {
                    let _array : (Qubit, Qubit)[] = targets;
                    {
                        let _range_id_49125 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_49128 : Int = _range_id_49125::Start;
                        let _step_id_49133 : Int = _range_id_49125::Step;
                        let _end_id_49138 : Int = _range_id_49125::End;
                        while _step_id_49133 > 0 and _index_id_49128 <= _end_id_49138 or _step_id_49133 < 0 and _index_id_49128 >= _end_id_49138 {
                            let _index : Int = _index_id_49128;
                            let (ctl : Qubit, tgt : Qubit) = _array[_index];
                            Adjoint CNOT(ctl, tgt);
                            _index_id_49128 += _step_id_49133;
                        }

                    }

                }

            }
        }
        function GetSpread(from : Qubit, to : Qubit[]) : (Qubit, Qubit)[] {
            mutable __cond_0 : Bool = false;
            mutable __cond_1 : Bool = false;
            mutable queue : (Qubit, Qubit[])[] = [(from, to)];
            mutable targets : (Qubit, Qubit)[] = [];
            while Length(queue) > 0 {
                mutable ((next_0 : Qubit, next_1 : Qubit[]), rest : (Qubit, Qubit[])[]) = (queue[0], queue[1...]);
                queue = rest;
                let next_from : Qubit = next_0;
                let next_to : Qubit[] = next_1;
                __cond_0 = Length(next_to) > 0;
                if __cond_0 {
                    targets = [(next_to[0], next_from)] + targets;
                    __cond_1 = Length(next_to) > 1;
                    if __cond_1 {
                        let half : Int = Length(next_to) / 2;
                        queue = [(next_from, next_to[1..half]), (next_to[0], next_to[half + 1...])] + rest;
                    }

                }

            }

            targets
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
        operation Exp(paulis : Pauli[], theta : Double, qubits : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                Fact(Length(paulis) == Length(qubits), $"Arrays 'pauli' and 'qubits' must have the same length");
                let (paulis : Pauli[], qubits : Qubit[]) = RemovePauliI(paulis, qubits);
                let angle : Double = -2. * theta;
                let len : Int = Length(paulis);
                if len == 0 {
                    ApplyGlobalPhase(theta);
                } else if len == 1 {
                    R(paulis[0], angle, qubits[0]);
                } else if len == 2 {
                    {
                        {
                            MapPauliAxis(paulis[0], paulis[1], qubits[1]);
                        }

                        let _apply_res : Unit = {
                            if paulis[0] == PauliX {
                                Rxx(angle, qubits[0], qubits[1]);
                            } else if paulis[0] == PauliY {
                                Ryy(angle, qubits[0], qubits[1]);
                            } else if paulis[0] == PauliZ {
                                Rzz(angle, qubits[0], qubits[1]);
                            }

                        };
                        {
                            Adjoint MapPauliAxis(paulis[0], paulis[1], qubits[1]);
                        }

                        _apply_res
                    }

                } else {
                    {
                        {
                            {
                                let _range_id_49168 : Range = 0..Length(paulis) - 1;
                                mutable _index_id_49171 : Int = _range_id_49168::Start;
                                let _step_id_49176 : Int = _range_id_49168::Step;
                                let _end_id_49181 : Int = _range_id_49168::End;
                                while _step_id_49176 > 0 and _index_id_49171 <= _end_id_49181 or _step_id_49176 < 0 and _index_id_49171 >= _end_id_49181 {
                                    let i : Int = _index_id_49171;
                                    MapPauliAxis(PauliZ, paulis[i], qubits[i]);
                                    _index_id_49171 += _step_id_49176;
                                }

                            }

                        }

                        let _apply_res : Unit = {
                            {
                                {
                                    SpreadZ(qubits[1], qubits[2..Length(qubits) - 1]);
                                }

                                let _apply_res : Unit = {
                                    Rzz(angle, qubits[0], qubits[1]);
                                };
                                {
                                    Adjoint SpreadZ(qubits[1], qubits[2..Length(qubits) - 1]);
                                }

                                _apply_res
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(paulis) - 1;
                                {
                                    let _range_id_49211 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                                    mutable _index_id_49214 : Int = _range_id_49211::Start;
                                    let _step_id_49219 : Int = _range_id_49211::Step;
                                    let _end_id_49224 : Int = _range_id_49211::End;
                                    while _step_id_49219 > 0 and _index_id_49214 <= _end_id_49224 or _step_id_49219 < 0 and _index_id_49214 >= _end_id_49224 {
                                        let i : Int = _index_id_49214;
                                        Adjoint MapPauliAxis(PauliZ, paulis[i], qubits[i]);
                                        _index_id_49214 += _step_id_49219;
                                    }

                                }

                            }

                        }

                        _apply_res
                    }

                }

            }
            adjoint ... {
                Exp(paulis, -theta, qubits);
            }
            controlled (ctls, ...) {
                Fact(Length(paulis) == Length(qubits), $"Arrays 'pauli' and 'qubits' must have the same length");
                let (paulis : Pauli[], qubits : Qubit[]) = RemovePauliI(paulis, qubits);
                let angle : Double = -2. * theta;
                let len : Int = Length(paulis);
                if len == 0 {
                    Controlled ApplyGlobalPhase(ctls, theta);
                } else if len == 1 {
                    Controlled R(ctls, (paulis[0], angle, qubits[0]));
                } else if len == 2 {
                    {
                        {
                            MapPauliAxis(paulis[0], paulis[1], qubits[1]);
                        }

                        let _apply_res : Unit = {
                            if paulis[0] == PauliX {
                                Controlled Rxx(ctls, (angle, qubits[0], qubits[1]));
                            } else if paulis[0] == PauliY {
                                Controlled Ryy(ctls, (angle, qubits[0], qubits[1]));
                            } else if paulis[0] == PauliZ {
                                Controlled Rzz(ctls, (angle, qubits[0], qubits[1]));
                            }

                        };
                        {
                            Adjoint MapPauliAxis(paulis[0], paulis[1], qubits[1]);
                        }

                        _apply_res
                    }

                } else {
                    {
                        {
                            {
                                let _range_id_49254 : Range = 0..Length(paulis) - 1;
                                mutable _index_id_49257 : Int = _range_id_49254::Start;
                                let _step_id_49262 : Int = _range_id_49254::Step;
                                let _end_id_49267 : Int = _range_id_49254::End;
                                while _step_id_49262 > 0 and _index_id_49257 <= _end_id_49267 or _step_id_49262 < 0 and _index_id_49257 >= _end_id_49267 {
                                    let i : Int = _index_id_49257;
                                    MapPauliAxis(PauliZ, paulis[i], qubits[i]);
                                    _index_id_49257 += _step_id_49262;
                                }

                            }

                        }

                        let _apply_res : Unit = {
                            {
                                {
                                    SpreadZ(qubits[1], qubits[2..Length(qubits) - 1]);
                                }

                                let _apply_res : Unit = {
                                    Controlled Rzz(ctls, (angle, qubits[0], qubits[1]));
                                };
                                {
                                    Adjoint SpreadZ(qubits[1], qubits[2..Length(qubits) - 1]);
                                }

                                _apply_res
                            }

                        };
                        {
                            {
                                let _range : Range = 0..Length(paulis) - 1;
                                {
                                    let _range_id_49297 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                                    mutable _index_id_49300 : Int = _range_id_49297::Start;
                                    let _step_id_49305 : Int = _range_id_49297::Step;
                                    let _end_id_49310 : Int = _range_id_49297::End;
                                    while _step_id_49305 > 0 and _index_id_49300 <= _end_id_49310 or _step_id_49305 < 0 and _index_id_49300 >= _end_id_49310 {
                                        let i : Int = _index_id_49300;
                                        Adjoint MapPauliAxis(PauliZ, paulis[i], qubits[i]);
                                        _index_id_49300 += _step_id_49305;
                                    }

                                }

                            }

                        }

                        _apply_res
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled Exp(ctls, (paulis, -theta, qubits));
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
                            let _generated_ident_53985 : Unit = {
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
                            _generated_ident_53985
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
                            let _generated_ident_53999 : Unit = {
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
                            _generated_ident_53999
                        }

                    }

                }

            }
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
        operation Rxx(theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__rxx__body(theta, qubit0, qubit1);
            }
            adjoint ... {
                Rxx(-theta, qubit0, qubit1);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                if __cond_0 {
                    __quantum__qis__rxx__body(theta, qubit0, qubit1);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        CRxx(ctls[0], theta, qubit0, qubit1);
                    } else {
                        let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 1);
                        let _generated_ident_54027 : Unit = {
                            {
                                CollectControls(ctls, aux, 0);
                                AdjustForSingleControl(ctls, aux);
                            }

                            let _apply_res : Unit = {
                                CRxx(aux[Length(ctls) - 2], theta, qubit0, qubit1);
                            };
                            {
                                Adjoint AdjustForSingleControl(ctls, aux);
                                Adjoint CollectControls(ctls, aux, 0);
                            }

                            _apply_res
                        };
                        ReleaseQubitArray(aux);
                        _generated_ident_54027
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled Rxx(ctls, (-theta, qubit0, qubit1));
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
        operation Ryy(theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__ryy__body(theta, qubit0, qubit1);
            }
            adjoint ... {
                Ryy(-theta, qubit0, qubit1);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                if __cond_0 {
                    __quantum__qis__ryy__body(theta, qubit0, qubit1);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        CRyy(ctls[0], theta, qubit0, qubit1);
                    } else {
                        let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 1);
                        let _generated_ident_54041 : Unit = {
                            {
                                CollectControls(ctls, aux, 0);
                                AdjustForSingleControl(ctls, aux);
                            }

                            let _apply_res : Unit = {
                                CRyy(aux[Length(ctls) - 2], theta, qubit0, qubit1);
                            };
                            {
                                Adjoint AdjustForSingleControl(ctls, aux);
                                Adjoint CollectControls(ctls, aux, 0);
                            }

                            _apply_res
                        };
                        ReleaseQubitArray(aux);
                        _generated_ident_54041
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled Ryy(ctls, (-theta, qubit0, qubit1));
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
                        let _generated_ident_54055 : Unit = {
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
                        _generated_ident_54055
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled Rz(ctls, (-theta, qubit));
            }
        }
        operation Rzz(theta : Double, qubit0 : Qubit, qubit1 : Qubit) : Unit is Adj + Ctl {
            body ... {
                __quantum__qis__rzz__body(theta, qubit0, qubit1);
            }
            adjoint ... {
                Rzz(-theta, qubit0, qubit1);
            }
            controlled (ctls, ...) {
                let __cond_0 : Bool = Length(ctls) == 0;
                mutable __cond_1 : Bool = false;
                if __cond_0 {
                    __quantum__qis__rzz__body(theta, qubit0, qubit1);
                } else {
                    __cond_1 = Length(ctls) == 1;
                    if __cond_1 {
                        CRzz(ctls[0], theta, qubit0, qubit1);
                    } else {
                        let aux : Qubit[] = AllocateQubitArray(Length(ctls) - 1);
                        let _generated_ident_54069 : Unit = {
                            {
                                CollectControls(ctls, aux, 0);
                                AdjustForSingleControl(ctls, aux);
                            }

                            let _apply_res : Unit = {
                                CRzz(aux[Length(ctls) - 2], theta, qubit0, qubit1);
                            };
                            {
                                Adjoint AdjustForSingleControl(ctls, aux);
                                Adjoint CollectControls(ctls, aux, 0);
                            }

                            _apply_res
                        };
                        ReleaseQubitArray(aux);
                        _generated_ident_54069
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                Controlled Rzz(ctls, (-theta, qubit0, qubit1));
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
                            let _generated_ident_54083 : Unit = {
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
                            _generated_ident_54083
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
                            let _generated_ident_54097 : Unit = {
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
                            _generated_ident_54097
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
                        let _generated_ident_54139 : Unit = {
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
                        _generated_ident_54139
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
                        let _generated_ident_54153 : Unit = {
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
                        _generated_ident_54153
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
                            let _generated_ident_54167 : Unit = {
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
                            _generated_ident_54167
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
                            let _generated_ident_54181 : Unit = {
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
                            _generated_ident_54181
                        }

                    }

                }

            }
        }
        function PI() : Double {
            3.141592653589793
        }
        function AbsD(a : Double) : Double {
            if a < 0. {
        -a
            } else {
                a
            }
        }
        function MinI(a : Int, b : Int) : Int {
            if a < b {
                a
            } else {
                b
            }
        }
        function Truncate(value : Double) : Int {
            body intrinsic;
        }
        function ExtendedTruncation(value : Double) : (Int, Double, Bool) {
            let truncated : Int = Truncate(value);
            (truncated, IntAsDouble(truncated) - value, value >= 0.)
        }
        function Ceiling(value : Double) : Int {
            let (truncated : Int, remainder : Double, isPositive : Bool) = ExtendedTruncation(value);
            let __cond_0 : Bool = AbsD(remainder) <= 0.000000000000001;
            if __cond_0 {
                truncated
            } else {
                if isPositive {
                    truncated + 1
                } else {
                    truncated
                }
            }

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
        function Chunks_Qubit_(chunkSize : Int, array : Qubit[]) : Qubit[][] {
            Fact(chunkSize > 0, $"`chunkSize` must be positive");
            mutable output : Qubit[][] = [];
            mutable remaining : Qubit[] = array;
            while not IsEmpty_Qubit_(remaining) {
                let chunkSizeToTake : Int = MinI(Length(remaining), chunkSize);
                output += [remaining[...chunkSizeToTake - 1]];
                remaining = remaining[chunkSizeToTake...];
            }

            output
        }
        function Subarray_Pauli_(locations : Int[], array : Pauli[]) : Pauli[] {
            mutable subarray : Pauli[] = [];
            {
                let _array_id_45927 : Int[] = locations;
                let _len_id_45931 : Int = Length(_array_id_45927);
                mutable _index_id_45936 : Int = 0;
                while _index_id_45936 < _len_id_45931 {
                    let location : Int = _array_id_45927[_index_id_45936];
                    subarray += [array[location]];
                    _index_id_45936 += 1;
                }

            }

            subarray
        }
        function Subarray_Qubit_(locations : Int[], array : Qubit[]) : Qubit[] {
            mutable subarray : Qubit[] = [];
            {
                let _array_id_45927 : Int[] = locations;
                let _len_id_45931 : Int = Length(_array_id_45927);
                mutable _index_id_45936 : Int = 0;
                while _index_id_45936 < _len_id_45931 {
                    let location : Int = _array_id_45927[_index_id_45936];
                    subarray += [array[location]];
                    _index_id_45936 += 1;
                }

            }

            subarray
        }
        function IsEmpty_Qubit_(array : Qubit[]) : Bool {
            Length(array) == 0
        }
        // package 2
        operation Main() : Unit {
            let n : Int = 10;
            let m : Int = 10;
            let J : Double = 1.;
            let g : Double = 1.;
            let totTime : Double = 30.;
            let dt : Double = 0.9;
            IsingModel2DSim(n, m, J, g, totTime, dt);
        }
        function SetAngleSequence(p : Double, dt : Double, J : Double, g : Double) : Double[] {
            mutable __cond_0 : Bool = false;
            mutable __cond_1 : Bool = false;
            mutable __cond_2 : Bool = false;
            let len1 : Int = 3;
            let len2 : Int = 3;
            let valLength : Int = 2 * len1 + len2 + 1;
            mutable values : Double[] = [0., size = valLength];
            let val1 : Double = J * p * dt;
            let val2 : Double = -g * p * dt;
            let val3 : Double = J * 1. - 3. * p * dt / 2.;
            let val4 : Double = g * 1. - 4. * p * dt / 2.;
            {
                let _range_id_578 : Range = 0..len1;
                mutable _index_id_581 : Int = _range_id_578::Start;
                let _step_id_586 : Int = _range_id_578::Step;
                let _end_id_591 : Int = _range_id_578::End;
                while _step_id_586 > 0 and _index_id_581 <= _end_id_591 or _step_id_586 < 0 and _index_id_581 >= _end_id_591 {
                    let i : Int = _index_id_581;
                    __cond_0 = i % 2 == 0;
                    if __cond_0 {
                        values w/= i <- val1;
                    } else {
                        values w/= i <- val2;
                    }

                    _index_id_581 += _step_id_586;
                }

            }

            {
                let _range_id_621 : Range = len1 + 1..len1 + len2;
                mutable _index_id_624 : Int = _range_id_621::Start;
                let _step_id_629 : Int = _range_id_621::Step;
                let _end_id_634 : Int = _range_id_621::End;
                while _step_id_629 > 0 and _index_id_624 <= _end_id_634 or _step_id_629 < 0 and _index_id_624 >= _end_id_634 {
                    let i : Int = _index_id_624;
                    __cond_1 = i % 2 == 0;
                    if __cond_1 {
                        values w/= i <- val3;
                    } else {
                        values w/= i <- val4;
                    }

                    _index_id_624 += _step_id_629;
                }

            }

            {
                let _range_id_664 : Range = len1 + len2 + 1..valLength - 1;
                mutable _index_id_667 : Int = _range_id_664::Start;
                let _step_id_672 : Int = _range_id_664::Step;
                let _end_id_677 : Int = _range_id_664::End;
                while _step_id_672 > 0 and _index_id_667 <= _end_id_677 or _step_id_672 < 0 and _index_id_667 >= _end_id_677 {
                    let i : Int = _index_id_667;
                    __cond_2 = i % 2 == 0;
                    if __cond_2 {
                        values w/= i <- val1;
                    } else {
                        values w/= i <- val2;
                    }

                    _index_id_667 += _step_id_672;
                }

            }

            values
        }
        operation ApplyAllX(n : Int, qArr : Qubit[][], theta : Double) : Unit {
            {
                let _range_id_707 : Range = 0..n - 1;
                mutable _index_id_710 : Int = _range_id_707::Start;
                let _step_id_715 : Int = _range_id_707::Step;
                let _end_id_720 : Int = _range_id_707::End;
                while _step_id_715 > 0 and _index_id_710 <= _end_id_720 or _step_id_715 < 0 and _index_id_710 >= _end_id_720 {
                    let row : Int = _index_id_710;
                    ApplyToEach_Qubit__AdjCtl__closure_(qArr[row], 2. * theta);
                    _index_id_710 += _step_id_715;
                }

            }

        }
        operation ApplyDoubleZ(n : Int, m : Int, qArr : Qubit[][], theta : Double, dir : Bool, grp : Bool) : Unit {
            let start : Int = if grp {
                1
            } else {
                0
            };
            let P_op : Pauli[] = [PauliZ, PauliZ];
            let c_end : Int = if dir {
                m - 1
            } else {
                m - 2
            };
            let r_end : Int = if dir {
                m - 2
            } else {
                m - 1
            };
            {
                let _range_id_793 : Range = 0..r_end;
                mutable _index_id_796 : Int = _range_id_793::Start;
                let _step_id_801 : Int = _range_id_793::Step;
                let _end_id_806 : Int = _range_id_793::End;
                while _step_id_801 > 0 and _index_id_796 <= _end_id_806 or _step_id_801 < 0 and _index_id_796 >= _end_id_806 {
                    let row : Int = _index_id_796;
                    {
                        let _range_id_750 : Range = start..2..c_end;
                        mutable _index_id_753 : Int = _range_id_750::Start;
                        let _step_id_758 : Int = _range_id_750::Step;
                        let _end_id_763 : Int = _range_id_750::End;
                        while _step_id_758 > 0 and _index_id_753 <= _end_id_763 or _step_id_758 < 0 and _index_id_753 >= _end_id_763 {
                            let col : Int = _index_id_753;
                            let row2 : Int = if dir {
                                row + 1
                            } else {
                                row
                            };
                            let col2 : Int = if dir {
                                col
                            } else {
                                col + 1
                            };
                            Exp(P_op, theta, [qArr[row][col], qArr[row2][col2]]);
                            _index_id_753 += _step_id_758;
                        }

                    }

                    _index_id_796 += _step_id_801;
                }

            }

        }
        operation IsingModel2DSim(N1 : Int, N2 : Int, J : Double, g : Double, totTime : Double, dt : Double) : Unit {
            mutable __cond_0 : Bool = false;
            let qs : Qubit[] = AllocateQubitArray(N1 * N2);
            let qubitArray : Qubit[][] = Chunks_Qubit_(N2, qs);
            let p : Double = 1. / 4. - 4.^1. / 3.;
            let t : Int = Ceiling(totTime / dt);
            let seqLen : Int = 10 * t + 1;
            let angSeq : Double[] = SetAngleSequence(p, dt, J, g);
            let _generated_ident_912 : Unit = {
                let _range_id_864 : Range = 0..seqLen - 1;
                mutable _index_id_867 : Int = _range_id_864::Start;
                let _step_id_872 : Int = _range_id_864::Step;
                let _end_id_877 : Int = _range_id_864::End;
                while _step_id_872 > 0 and _index_id_867 <= _end_id_877 or _step_id_872 < 0 and _index_id_867 >= _end_id_877 {
                    let i : Int = _index_id_867;
                    let theta : Double = if i == 0 or i == seqLen - 1 {
                        J * p * dt / 2.
                    } else {
                        angSeq[i % 10]
                    };
                    __cond_0 = i % 2 == 0;
                    if __cond_0 {
                        ApplyAllX(N1, qubitArray, theta);
                    } else {
                        {
                            let _array_id_836 : (Bool, Bool)[] = [(true, true), (true, false), (false, true), (false, false)];
                            let _len_id_840 : Int = Length(_array_id_836);
                            mutable _index_id_845 : Int = 0;
                            while _index_id_845 < _len_id_840 {
                                let (dir : Bool, grp : Bool) = _array_id_836[_index_id_845];
                                ApplyDoubleZ(N1, N2, qubitArray, theta, dir, grp);
                                _index_id_845 += 1;
                            }

                        }

                    }

                    _index_id_867 += _step_id_872;
                }

            };
            ReleaseQubitArray(qs);
            _generated_ident_912
        }
        operation _lambda_(arg : Double, hole : Qubit) : Unit is Adj + Ctl {
            body ... {
                Rx(arg, hole)
            }
            adjoint ... {
                Adjoint Rx(arg, hole)
            }
            controlled (ctls, ...) {
                Controlled Rx(ctls, (arg, hole))
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint Rx(ctls, (arg, hole))
            }
        }
        operation ApplyToEach_Qubit__AdjCtl__closure_(register : Qubit[], __capture_0 : Double) : Unit {
            {
                let _array_id_46213 : Qubit[] = register;
                let _len_id_46217 : Int = Length(_array_id_46213);
                mutable _index_id_46222 : Int = 0;
                while _index_id_46222 < _len_id_46217 {
                    let item : Qubit = _array_id_46213[_index_id_46222];
                    _lambda_(__capture_0, item);
                    _index_id_46222 += 1;
                }

            }

        }
        // entry
        Main()"#]].assert_eq(&rendered);
}
