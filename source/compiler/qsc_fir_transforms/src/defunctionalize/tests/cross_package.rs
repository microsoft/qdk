// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Many tests pair a primary assertion with a `check_rewrite` before/after
// snapshot, so the generated Q# pushes function bodies past the line limit.
#![allow(clippy::too_many_lines)]

use super::*;
use expect_test::expect;
use indoc::indoc;

#[test]
fn analysis_apply_operation_power_ca_consumer() {
    let source = r#"
        operation Consume(apply_power_of_u : (Int, Qubit[]) => Unit is Adj + Ctl, target : Qubit[]) : Unit {
            apply_power_of_u(1, target);
        }

        operation U(qs : Qubit[]) : Unit is Adj + Ctl {
            H(qs[0]);
        }

        operation Main() : Unit {
            use qs = Qubit[1];
            Consume(ApplyOperationPowerCA(_, U, _), qs);
        }
                "#;
    check_analysis_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            callable_params: 3
              param: callable_id=4, path=[0], ty=((Qubit)[] => Unit is Adj + Ctl)
              param: callable_id=6, path=[1], ty=((Qubit)[] => Unit is Adj + Ctl)
              param: callable_id=7, path=[0], ty=((Int, (Qubit)[]) => Unit is Adj + Ctl)
            call_sites: 5
              site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic
              site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic
              site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic
              site: hof=ApplyOperationPowerCA<(Qubit)[], AdjCtl>, arg=Dynamic
              site: hof=Consume<AdjCtl>, arg=Closure(target=4, Body)
            direct_call_sites: 3
              site: callee=H:Adj, default
              site: callee=H:Ctl, default
              site: callee=H:CtlAdj, default
            lattice states:
              callable ApplyOperationPowerCA<(Qubit)[], AdjCtl>:
                3: Dynamic
                8: Dynamic
                15: Dynamic
                21: Dynamic"#]],
    );
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Consume(apply_power_of_u : ((Int, Qubit[]) => Unit), target : Qubit[]) : Unit {
            apply_power_of_u(1, target);
        }
        operation U(qs : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                H(qs[0]);
            }
            adjoint ... {
                Adjoint H(qs[0]);
            }
            controlled (ctls, ...) {
                Controlled H(ctls, qs[0]);
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint H(ctls, qs[0]);
            }
        }
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(1);
            Consume_AdjCtl_({
                let arg : (Qubit[] => Unit is Adj + Ctl) = U;
                / * closure item = 4 captures = [arg] * / _lambda_
            }, qs);
            ReleaseQubitArray(qs);
        }
        operation _lambda_(arg : (Qubit[] => Unit is Adj + Ctl), (hole : Int, hole : Qubit[])) : Unit is Adj + Ctl {
            body ... {
                ApplyOperationPowerCA__Qubit_____AdjCtl_(hole, arg, hole)
            }
            adjoint ... {
                Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl_(hole, arg, hole)
            }
            controlled (ctls, ...) {
                Controlled ApplyOperationPowerCA__Qubit_____AdjCtl_(ctls, (hole, arg, hole))
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl_(ctls, (hole, arg, hole))
            }
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyOperationPowerCA__Qubit_____AdjCtl_(power : Int, op : (Qubit[] => Unit is Adj + Ctl), target : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                let u : (Qubit[] => Unit is Adj + Ctl) = if power >= 0 {
                    op
                } else {
                    Adjoint op
                };
                {
                    let _range_id_48008 : Range = 1..AbsI(power);
                    mutable _index_id_48011 : Int = _range_id_48008::Start;
                    let _step_id_48016 : Int = _range_id_48008::Step;
                    let _end_id_48021 : Int = _range_id_48008::End;
                    while _step_id_48016 > 0 and _index_id_48011 <= _end_id_48021 or _step_id_48016 < 0 and _index_id_48011 >= _end_id_48021 {
                        let _ : Int = _index_id_48011;
                        u(target);
                        _index_id_48011 += _step_id_48016;
                    }

                }

            }
            adjoint ... {
                let u : (Qubit[] => Unit is Adj + Ctl) = if power >= 0 {
                    op
                } else {
                    Adjoint op
                };
                {
                    let _range : Range = 1..AbsI(power);
                    {
                        let _range_id_48051 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_48054 : Int = _range_id_48051::Start;
                        let _step_id_48059 : Int = _range_id_48051::Step;
                        let _end_id_48064 : Int = _range_id_48051::End;
                        while _step_id_48059 > 0 and _index_id_48054 <= _end_id_48064 or _step_id_48059 < 0 and _index_id_48054 >= _end_id_48064 {
                            let _ : Int = _index_id_48054;
                            Adjoint u(target);
                            _index_id_48054 += _step_id_48059;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                let u : (Qubit[] => Unit is Adj + Ctl) = if power >= 0 {
                    op
                } else {
                    Adjoint op
                };
                {
                    let _range_id_48094 : Range = 1..AbsI(power);
                    mutable _index_id_48097 : Int = _range_id_48094::Start;
                    let _step_id_48102 : Int = _range_id_48094::Step;
                    let _end_id_48107 : Int = _range_id_48094::End;
                    while _step_id_48102 > 0 and _index_id_48097 <= _end_id_48107 or _step_id_48102 < 0 and _index_id_48097 >= _end_id_48107 {
                        let _ : Int = _index_id_48097;
                        Controlled u(ctls, target);
                        _index_id_48097 += _step_id_48102;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                let u : (Qubit[] => Unit is Adj + Ctl) = if power >= 0 {
                    op
                } else {
                    Adjoint op
                };
                {
                    let _range : Range = 1..AbsI(power);
                    {
                        let _range_id_48137 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_48140 : Int = _range_id_48137::Start;
                        let _step_id_48145 : Int = _range_id_48137::Step;
                        let _end_id_48150 : Int = _range_id_48137::End;
                        while _step_id_48145 > 0 and _index_id_48140 <= _end_id_48150 or _step_id_48145 < 0 and _index_id_48140 >= _end_id_48150 {
                            let _ : Int = _index_id_48140;
                            Controlled Adjoint u(ctls, target);
                            _index_id_48140 += _step_id_48145;
                        }

                    }

                }

            }
        }
        operation Consume_AdjCtl_(apply_power_of_u : ((Int, Qubit[]) => Unit is Adj + Ctl), target : Qubit[]) : Unit {
            apply_power_of_u(1, target);
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Consume(apply_power_of_u : ((Int, Qubit[]) => Unit), target : Qubit[]) : Unit {
            apply_power_of_u(1, target);
        }
        operation U(qs : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                H(qs[0]);
            }
            adjoint ... {
                Adjoint H(qs[0]);
            }
            controlled (ctls, ...) {
                Controlled H(ctls, qs[0]);
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint H(ctls, qs[0]);
            }
        }
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(1);
            Consume_AdjCtl__closure__U_(qs);
            ReleaseQubitArray(qs);
        }
        operation _lambda_(arg : (Qubit[] => Unit is Adj + Ctl), (hole : Int, hole : Qubit[])) : Unit is Adj + Ctl {
            body ... {
                ApplyOperationPowerCA__Qubit_____AdjCtl_(hole, arg, hole)
            }
            adjoint ... {
                Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl_(hole, arg, hole)
            }
            controlled (ctls, ...) {
                Controlled ApplyOperationPowerCA__Qubit_____AdjCtl_(ctls, (hole, arg, hole))
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl_(ctls, (hole, arg, hole))
            }
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyOperationPowerCA__Qubit_____AdjCtl_(power : Int, op : (Qubit[] => Unit is Adj + Ctl), target : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                let u : (Qubit[] => Unit is Adj + Ctl) = if power >= 0 {
                    op
                } else {
                    Adjoint op
                };
                {
                    let _range_id_48008 : Range = 1..AbsI(power);
                    mutable _index_id_48011 : Int = _range_id_48008::Start;
                    let _step_id_48016 : Int = _range_id_48008::Step;
                    let _end_id_48021 : Int = _range_id_48008::End;
                    while _step_id_48016 > 0 and _index_id_48011 <= _end_id_48021 or _step_id_48016 < 0 and _index_id_48011 >= _end_id_48021 {
                        let _ : Int = _index_id_48011;
                        u(target);
                        _index_id_48011 += _step_id_48016;
                    }

                }

            }
            adjoint ... {
                let u : (Qubit[] => Unit is Adj + Ctl) = if power >= 0 {
                    op
                } else {
                    Adjoint op
                };
                {
                    let _range : Range = 1..AbsI(power);
                    {
                        let _range_id_48051 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_48054 : Int = _range_id_48051::Start;
                        let _step_id_48059 : Int = _range_id_48051::Step;
                        let _end_id_48064 : Int = _range_id_48051::End;
                        while _step_id_48059 > 0 and _index_id_48054 <= _end_id_48064 or _step_id_48059 < 0 and _index_id_48054 >= _end_id_48064 {
                            let _ : Int = _index_id_48054;
                            Adjoint u(target);
                            _index_id_48054 += _step_id_48059;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                let u : (Qubit[] => Unit is Adj + Ctl) = if power >= 0 {
                    op
                } else {
                    Adjoint op
                };
                {
                    let _range_id_48094 : Range = 1..AbsI(power);
                    mutable _index_id_48097 : Int = _range_id_48094::Start;
                    let _step_id_48102 : Int = _range_id_48094::Step;
                    let _end_id_48107 : Int = _range_id_48094::End;
                    while _step_id_48102 > 0 and _index_id_48097 <= _end_id_48107 or _step_id_48102 < 0 and _index_id_48097 >= _end_id_48107 {
                        let _ : Int = _index_id_48097;
                        Controlled u(ctls, target);
                        _index_id_48097 += _step_id_48102;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                let u : (Qubit[] => Unit is Adj + Ctl) = if power >= 0 {
                    op
                } else {
                    Adjoint op
                };
                {
                    let _range : Range = 1..AbsI(power);
                    {
                        let _range_id_48137 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_48140 : Int = _range_id_48137::Start;
                        let _step_id_48145 : Int = _range_id_48137::Step;
                        let _end_id_48150 : Int = _range_id_48137::End;
                        while _step_id_48145 > 0 and _index_id_48140 <= _end_id_48150 or _step_id_48145 < 0 and _index_id_48140 >= _end_id_48150 {
                            let _ : Int = _index_id_48140;
                            Controlled Adjoint u(ctls, target);
                            _index_id_48140 += _step_id_48145;
                        }

                    }

                }

            }
        }
        operation Consume_AdjCtl_(apply_power_of_u : ((Int, Qubit[]) => Unit is Adj + Ctl), target : Qubit[]) : Unit {
            apply_power_of_u(1, target);
        }
        operation Consume_AdjCtl__closure_(target : Qubit[], __capture_0 : (Qubit[] => Unit is Adj + Ctl)) : Unit {
            _lambda_(__capture_0, (1, target));
        }
        operation Consume_AdjCtl__closure__U_(target : Qubit[]) : Unit {
            _lambda__U_(1, target);
        }
        operation _lambda__U_(hole : Int, hole : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                ApplyOperationPowerCA__Qubit_____AdjCtl__U_(hole, hole)
            }
            adjoint ... {
                Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl__U_(hole, hole)
            }
            controlled (ctls, ...) {
                Controlled ApplyOperationPowerCA__Qubit_____AdjCtl__U_(ctls, (hole, hole))
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint ApplyOperationPowerCA__Qubit_____AdjCtl__U_(ctls, (hole, hole))
            }
        }
        operation ApplyOperationPowerCA__Qubit_____AdjCtl__U_(power : Int, target : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _range_id_48008 : Range = 1..AbsI(power);
                    mutable _index_id_48011 : Int = _range_id_48008::Start;
                    let _step_id_48016 : Int = _range_id_48008::Step;
                    let _end_id_48021 : Int = _range_id_48008::End;
                    while _step_id_48016 > 0 and _index_id_48011 <= _end_id_48021 or _step_id_48016 < 0 and _index_id_48011 >= _end_id_48021 {
                        let _ : Int = _index_id_48011;
                        if power >= 0 {
                            U(target)
                        } else {
                            Adjoint U(target)
                        };
                        _index_id_48011 += _step_id_48016;
                    }

                }

            }
            adjoint ... {
                {
                    let _range : Range = 1..AbsI(power);
                    {
                        let _range_id_48051 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_48054 : Int = _range_id_48051::Start;
                        let _step_id_48059 : Int = _range_id_48051::Step;
                        let _end_id_48064 : Int = _range_id_48051::End;
                        while _step_id_48059 > 0 and _index_id_48054 <= _end_id_48064 or _step_id_48059 < 0 and _index_id_48054 >= _end_id_48064 {
                            let _ : Int = _index_id_48054;
                            if power >= 0 {
                                Adjoint U(target)
                            } else {
                                U(target)
                            };
                            _index_id_48054 += _step_id_48059;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _range_id_48094 : Range = 1..AbsI(power);
                    mutable _index_id_48097 : Int = _range_id_48094::Start;
                    let _step_id_48102 : Int = _range_id_48094::Step;
                    let _end_id_48107 : Int = _range_id_48094::End;
                    while _step_id_48102 > 0 and _index_id_48097 <= _end_id_48107 or _step_id_48102 < 0 and _index_id_48097 >= _end_id_48107 {
                        let _ : Int = _index_id_48097;
                        if power >= 0 {
                            Controlled U(ctls, target)
                        } else {
                            Controlled Adjoint U(ctls, target)
                        };
                        _index_id_48097 += _step_id_48102;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _range : Range = 1..AbsI(power);
                    {
                        let _range_id_48137 : Range = _range::Start + _range::End - _range::Start / _range::Step * _range::Step..-_range::Step.._range::Start;
                        mutable _index_id_48140 : Int = _range_id_48137::Start;
                        let _step_id_48145 : Int = _range_id_48137::Step;
                        let _end_id_48150 : Int = _range_id_48137::End;
                        while _step_id_48145 > 0 and _index_id_48140 <= _end_id_48150 or _step_id_48145 < 0 and _index_id_48140 >= _end_id_48150 {
                            let _ : Int = _index_id_48140;
                            if power >= 0 {
                                Controlled Adjoint U(ctls, target)
                            } else {
                                Controlled U(ctls, target)
                            };
                            _index_id_48140 += _step_id_48145;
                        }

                    }

                }

            }
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn analysis_bernstein_vazirani_sample_shape() {
    let source = r#"
        import Std.Arrays.*;
        import Std.Convert.*;
        import Std.Diagnostics.*;
        import Std.Math.*;
        import Std.Measurement.*;

        operation Main() : Unit {
            let nQubits = 10;
            let integers = [127, 238, 512];
            for integer in integers {
                let parityOperation = EncodeIntegerAsParityOperation(integer);
                let _ = BernsteinVazirani(parityOperation, nQubits);
            }
        }

        operation BernsteinVazirani(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
            use queryRegister = Qubit[n];
            use target = Qubit();
            X(target);
            within {
                ApplyToEachA(H, queryRegister);
            } apply {
                H(target);
                Uf(queryRegister, target);
            }
            let resultArray = MResetEachZ(queryRegister);
            Reset(target);
            resultArray
        }

        operation ApplyParityOperation(bitStringAsInt : Int, xRegister : Qubit[], yQubit : Qubit) : Unit {
            let requiredBits = BitSizeI(bitStringAsInt);
            let availableQubits = Length(xRegister);
            Fact(availableQubits >= requiredBits, "enough qubits");
            for index in IndexRange(xRegister) {
                if ((bitStringAsInt &&& 2^index) != 0) {
                    CNOT(xRegister[index], yQubit);
                }
            }
        }

        function EncodeIntegerAsParityOperation(bitStringAsInt : Int) : (Qubit[], Qubit) => Unit {
            return ApplyParityOperation(bitStringAsInt, _, _);
        }
                "#;
    check_analysis_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            callable_params: 2
              param: callable_id=10, path=[0], ty=(((Qubit)[], Qubit) => Unit)
              param: callable_id=6, path=[0], ty=(Qubit => Unit is Adj + Ctl)
            call_sites: 3
              site: hof=ApplyToEachA<Qubit, AdjCtl>, arg=Global(H, Body)
              site: hof=ApplyToEachA<Qubit, AdjCtl>, arg=Global(H, Body)
              site: hof=BernsteinVazirani<Empty>, arg=Closure(target=5, Body)
            lattice states:
              callable Main:
                7: Single(Closure(5):Body)"#]],
    );
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let nQubits : Int = 10;
            let integers : Int[] = [127, 238, 512];
            {
                let _array_id_207 : Int[] = integers;
                let _len_id_211 : Int = Length(_array_id_207);
                mutable _index_id_216 : Int = 0;
                while _index_id_216 < _len_id_211 {
                    let integer : Int = _array_id_207[_index_id_216];
                    let parityOperation : ((Qubit[], Qubit) => Unit) = EncodeIntegerAsParityOperation(integer);
                    let _ : Result[] = BernsteinVazirani_Empty_(parityOperation, nQubits);
                    _index_id_216 += 1;
                }

            }

        }
        operation BernsteinVazirani(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            {
                {
                    ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                }

                let _apply_res : Unit = {
                    H(target);
                    Uf(queryRegister, target);
                };
                {
                    Adjoint ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                }

                _apply_res
            }

            let resultArray : Result[] = MResetEachZ(queryRegister);
            Reset(target);
            let _generated_ident_288 : Result[] = resultArray;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_288
        }
        operation ApplyParityOperation(bitStringAsInt : Int, xRegister : Qubit[], yQubit : Qubit) : Unit {
            let requiredBits : Int = BitSizeI(bitStringAsInt);
            let availableQubits : Int = Length(xRegister);
            Fact(availableQubits >= requiredBits, $"enough qubits");
            {
                let _range_id_235 : Range = IndexRange_Qubit_(xRegister);
                mutable _index_id_238 : Int = _range_id_235::Start;
                let _step_id_243 : Int = _range_id_235::Step;
                let _end_id_248 : Int = _range_id_235::End;
                while _step_id_243 > 0 and _index_id_238 <= _end_id_248 or _step_id_243 < 0 and _index_id_238 >= _end_id_248 {
                    let index : Int = _index_id_238;
                    if bitStringAsInt &&& 2^index != 0 {
                        CNOT(xRegister[index], yQubit);
                    }

                    _index_id_238 += _step_id_243;
                }

            }

        }
        function EncodeIntegerAsParityOperation(bitStringAsInt : Int) : ((Qubit[], Qubit) => Unit) {
            return {
                let arg : Int = bitStringAsInt;
                / * closure item = 5 captures = [arg] * / _lambda_
            };
        }
        operation _lambda_(arg : Int, (hole : Qubit[], hole : Qubit)) : Unit {
            ApplyParityOperation(arg, hole, hole)
        }
        operation ApplyToEachA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj {
            body ... {
                {
                    let _array_id_46225 : Qubit[] = register;
                    let _len_id_46229 : Int = Length(_array_id_46225);
                    mutable _index_id_46234 : Int = 0;
                    while _index_id_46234 < _len_id_46229 {
                        let item : Qubit = _array_id_46225[_index_id_46234];
                        singleElementOperation(item);
                        _index_id_46234 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46253 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46256 : Int = _range_id_46253::Start;
                        let _step_id_46261 : Int = _range_id_46253::Step;
                        let _end_id_46266 : Int = _range_id_46253::End;
                        while _step_id_46261 > 0 and _index_id_46256 <= _end_id_46266 or _step_id_46261 < 0 and _index_id_46256 >= _end_id_46266 {
                            let _index : Int = _index_id_46256;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46256 += _step_id_46261;
                        }

                    }

                }

            }
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        function IndexRange_Qubit_(array : Qubit[]) : Range {
            0..Length(array) - 1
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        operation BernsteinVazirani_Empty_(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            {
                {
                    ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                }

                let _apply_res : Unit = {
                    H(target);
                    Uf(queryRegister, target);
                };
                {
                    Adjoint ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                }

                _apply_res
            }

            let resultArray : Result[] = MResetEachZ(queryRegister);
            Reset(target);
            let _generated_ident_288 : Result[] = resultArray;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_288
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let nQubits : Int = 10;
            let integers : Int[] = [127, 238, 512];
            {
                let _array_id_207 : Int[] = integers;
                let _len_id_211 : Int = Length(_array_id_207);
                mutable _index_id_216 : Int = 0;
                while _index_id_216 < _len_id_211 {
                    let integer : Int = _array_id_207[_index_id_216];
                    let _ : Result[] = BernsteinVazirani_Empty__closure_(nQubits, nQubits);
                    _index_id_216 += 1;
                }

            }

        }
        operation BernsteinVazirani(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            {
                {
                    ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                }

                let _apply_res : Unit = {
                    H(target);
                    Uf(queryRegister, target);
                };
                {
                    Adjoint ApplyToEachA_Qubit__AdjCtl_(H, queryRegister);
                }

                _apply_res
            }

            let resultArray : Result[] = MResetEachZ(queryRegister);
            Reset(target);
            let _generated_ident_288 : Result[] = resultArray;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_288
        }
        operation ApplyParityOperation(bitStringAsInt : Int, xRegister : Qubit[], yQubit : Qubit) : Unit {
            let requiredBits : Int = BitSizeI(bitStringAsInt);
            let availableQubits : Int = Length(xRegister);
            Fact(availableQubits >= requiredBits, $"enough qubits");
            {
                let _range_id_235 : Range = IndexRange_Qubit_(xRegister);
                mutable _index_id_238 : Int = _range_id_235::Start;
                let _step_id_243 : Int = _range_id_235::Step;
                let _end_id_248 : Int = _range_id_235::End;
                while _step_id_243 > 0 and _index_id_238 <= _end_id_248 or _step_id_243 < 0 and _index_id_238 >= _end_id_248 {
                    let index : Int = _index_id_238;
                    if bitStringAsInt &&& 2^index != 0 {
                        CNOT(xRegister[index], yQubit);
                    }

                    _index_id_238 += _step_id_243;
                }

            }

        }
        function EncodeIntegerAsParityOperation(bitStringAsInt : Int) : ((Qubit[], Qubit) => Unit) {
            return {
                let arg : Int = bitStringAsInt;
                ()
            };
        }
        operation _lambda_(arg : Int, (hole : Qubit[], hole : Qubit)) : Unit {
            ApplyParityOperation(arg, hole, hole)
        }
        operation ApplyToEachA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj {
            body ... {
                {
                    let _array_id_46225 : Qubit[] = register;
                    let _len_id_46229 : Int = Length(_array_id_46225);
                    mutable _index_id_46234 : Int = 0;
                    while _index_id_46234 < _len_id_46229 {
                        let item : Qubit = _array_id_46225[_index_id_46234];
                        singleElementOperation(item);
                        _index_id_46234 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46253 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46256 : Int = _range_id_46253::Start;
                        let _step_id_46261 : Int = _range_id_46253::Step;
                        let _end_id_46266 : Int = _range_id_46253::End;
                        while _step_id_46261 > 0 and _index_id_46256 <= _end_id_46266 or _step_id_46261 < 0 and _index_id_46256 >= _end_id_46266 {
                            let _index : Int = _index_id_46256;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46256 += _step_id_46261;
                        }

                    }

                }

            }
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        function IndexRange_Qubit_(array : Qubit[]) : Range {
            0..Length(array) - 1
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        operation BernsteinVazirani_Empty_(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Result[] {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            {
                {
                    ApplyToEachA_Qubit__AdjCtl__H_(queryRegister);
                }

                let _apply_res : Unit = {
                    H(target);
                    Uf(queryRegister, target);
                };
                {
                    Adjoint ApplyToEachA_Qubit__AdjCtl__H_(queryRegister);
                }

                _apply_res
            }

            let resultArray : Result[] = MResetEachZ(queryRegister);
            Reset(target);
            let _generated_ident_288 : Result[] = resultArray;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_288
        }
        operation ApplyToEachA_Qubit__AdjCtl__H_(register : Qubit[]) : Unit is Adj {
            body ... {
                {
                    let _array_id_46225 : Qubit[] = register;
                    let _len_id_46229 : Int = Length(_array_id_46225);
                    mutable _index_id_46234 : Int = 0;
                    while _index_id_46234 < _len_id_46229 {
                        let item : Qubit = _array_id_46225[_index_id_46234];
                        H(item);
                        _index_id_46234 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46253 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46256 : Int = _range_id_46253::Start;
                        let _step_id_46261 : Int = _range_id_46253::Step;
                        let _end_id_46266 : Int = _range_id_46253::End;
                        while _step_id_46261 > 0 and _index_id_46256 <= _end_id_46266 or _step_id_46261 < 0 and _index_id_46256 >= _end_id_46266 {
                            let _index : Int = _index_id_46256;
                            let item : Qubit = _array[_index];
                            Adjoint H(item);
                            _index_id_46256 += _step_id_46261;
                        }

                    }

                }

            }
        }
        operation BernsteinVazirani_Empty__closure_(n : Int, __capture_0 : Int) : Result[] {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            {
                {
                    ApplyToEachA_Qubit__AdjCtl__H_(queryRegister);
                }

                let _apply_res : Unit = {
                    H(target);
                    _lambda_(__capture_0, (queryRegister, target));
                };
                {
                    Adjoint ApplyToEachA_Qubit__AdjCtl__H_(queryRegister);
                }

                _apply_res
            }

            let resultArray : Result[] = MResetEachZ(queryRegister);
            Reset(target);
            let _generated_ident_288 : Result[] = resultArray;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_288
        }
        operation ApplyToEachA_Qubit__AdjCtl__H_(register : Qubit[]) : Unit is Adj {
            body ... {
                {
                    let _array_id_46225 : Qubit[] = register;
                    let _len_id_46229 : Int = Length(_array_id_46225);
                    mutable _index_id_46234 : Int = 0;
                    while _index_id_46234 < _len_id_46229 {
                        let item : Qubit = _array_id_46225[_index_id_46234];
                        H(item);
                        _index_id_46234 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46253 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46256 : Int = _range_id_46253::Start;
                        let _step_id_46261 : Int = _range_id_46253::Step;
                        let _end_id_46266 : Int = _range_id_46253::End;
                        while _step_id_46261 > 0 and _index_id_46256 <= _end_id_46266 or _step_id_46261 < 0 and _index_id_46256 >= _end_id_46266 {
                            let _index : Int = _index_id_46256;
                            let item : Qubit = _array[_index];
                            Adjoint H(item);
                            _index_id_46256 += _step_id_46261;
                        }

                    }

                }

            }
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn analysis_deutsch_jozsa_sample_shape() {
    let source = r#"
        import Std.Diagnostics.*;
        import Std.Math.*;
        import Std.Measurement.*;

        operation Main() : Unit {
            let functionsToTest = [SimpleConstantBoolF, SimpleBalancedBoolF, ConstantBoolF, BalancedBoolF];
            for fn in functionsToTest {
                let _ = DeutschJozsa(fn, 5);
            }
        }

        operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
            use queryRegister = Qubit[n];
            use target = Qubit();
            X(target);
            H(target);
            within {
                for q in queryRegister {
                    H(q);
                }
            } apply {
                Uf(queryRegister, target);
            }
            mutable result = true;
            for q in queryRegister {
                if MResetZ(q) == One {
                    result = false;
                }
            }
            Reset(target);
            result
        }

        operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            X(target);
        }

        operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            CX(args[0], target);
        }

        operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            for i in 0..(2^Length(args)) - 1 {
                ApplyControlledOnInt(i, X, args, target);
            }
        }

        operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            for i in 0..2..(2^Length(args)) - 1 {
                ApplyControlledOnInt(i, X, args, target);
            }
        }
                "#;
    check_analysis_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
            callable_params: 2
              param: callable_id=8, path=[1], ty=(Qubit => Unit is Adj + Ctl)
              param: callable_id=10, path=[0], ty=(((Qubit)[], Qubit) => Unit)
            call_sites: 6
              site: hof=ApplyControlledOnInt<Qubit, AdjCtl>, arg=Global(X, Body)
              site: hof=ApplyControlledOnInt<Qubit, AdjCtl>, arg=Global(X, Body)
              site: hof=DeutschJozsa<Empty>, arg=Global(SimpleConstantBoolF, Body)
              site: hof=DeutschJozsa<Empty>, arg=Global(SimpleBalancedBoolF, Body)
              site: hof=DeutschJozsa<Empty>, arg=Global(ConstantBoolF, Body)
              site: hof=DeutschJozsa<Empty>, arg=Global(BalancedBoolF, Body)
            direct_call_sites: 5
              site: callee=ApplyPauliFromInt:Adj, default
              site: callee=ApplyPauliFromInt:Adj, default
              site: callee=ApplyPauliFromInt:Adj, default
              site: callee=ApplyPauliFromInt:Adj, default
              site: callee=H:Adj, default
            lattice states:
              callable Main:
                5: Multi([SimpleConstantBoolF:Body, SimpleBalancedBoolF:Body, ConstantBoolF:Body, BalancedBoolF:Body])"#]],
    );
    check_rewrite_with_capabilities(
        source,
        adaptive_qirgen_capabilities(),
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let functionsToTest : ((Qubit[], Qubit) => Unit)[] = [SimpleConstantBoolF, SimpleBalancedBoolF, ConstantBoolF, BalancedBoolF];
            {
                let _array_id_244 : ((Qubit[], Qubit) => Unit)[] = functionsToTest;
                let _len_id_248 : Int = Length(_array_id_244);
                mutable _index_id_253 : Int = 0;
                while _index_id_253 < _len_id_248 {
                    let fn : ((Qubit[], Qubit) => Unit) = _array_id_244[_index_id_253];
                    let _ : Bool = DeutschJozsa_Empty_(fn, 5);
                    _index_id_253 += 1;
                }

            }

        }
        operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            H(target);
            {
                {
                    {
                        let _array_id_272 : Qubit[] = queryRegister;
                        let _len_id_276 : Int = Length(_array_id_272);
                        mutable _index_id_281 : Int = 0;
                        while _index_id_281 < _len_id_276 {
                            let q : Qubit = _array_id_272[_index_id_281];
                            H(q);
                            _index_id_281 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    Uf(queryRegister, target);
                };
                {
                    {
                        let _array : Qubit[] = queryRegister;
                        {
                            let _range_id_300 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_303 : Int = _range_id_300::Start;
                            let _step_id_308 : Int = _range_id_300::Step;
                            let _end_id_313 : Int = _range_id_300::End;
                            while _step_id_308 > 0 and _index_id_303 <= _end_id_313 or _step_id_308 < 0 and _index_id_303 >= _end_id_313 {
                                let _index : Int = _index_id_303;
                                let q : Qubit = _array[_index];
                                Adjoint H(q);
                                _index_id_303 += _step_id_308;
                            }

                        }

                    }

                }

                _apply_res
            }

            mutable result : Bool = true;
            {
                let _array_id_343 : Qubit[] = queryRegister;
                let _len_id_347 : Int = Length(_array_id_343);
                mutable _index_id_352 : Int = 0;
                while _index_id_352 < _len_id_347 {
                    let q : Qubit = _array_id_343[_index_id_352];
                    if MResetZ(q) == One {
                        result = false;
                    }

                    _index_id_352 += 1;
                }

            }

            Reset(target);
            let _generated_ident_467 : Bool = result;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_467
        }
        operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            X(target);
        }
        operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            CX(args[0], target);
        }
        operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            {
                let _range_id_371 : Range = 0..2^Length(args) - 1;
                mutable _index_id_374 : Int = _range_id_371::Start;
                let _step_id_379 : Int = _range_id_371::Step;
                let _end_id_384 : Int = _range_id_371::End;
                while _step_id_379 > 0 and _index_id_374 <= _end_id_384 or _step_id_379 < 0 and _index_id_374 >= _end_id_384 {
                    let i : Int = _index_id_374;
                    ApplyControlledOnInt_Qubit__AdjCtl_(i, X, args, target);
                    _index_id_374 += _step_id_379;
                }

            }

        }
        operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            {
                let _range_id_414 : Range = 0..2..2^Length(args) - 1;
                mutable _index_id_417 : Int = _range_id_414::Start;
                let _step_id_422 : Int = _range_id_414::Step;
                let _end_id_427 : Int = _range_id_414::End;
                while _step_id_422 > 0 and _index_id_417 <= _end_id_427 or _step_id_422 < 0 and _index_id_417 >= _end_id_427 {
                    let i : Int = _index_id_417;
                    ApplyControlledOnInt_Qubit__AdjCtl_(i, X, args, target);
                    _index_id_417 += _step_id_422;
                }

            }

        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyControlledOnInt_Qubit__AdjCtl_(numberState : Int, oracle : (Qubit => Unit is Adj + Ctl), controlRegister : Qubit[], target : Qubit) : Unit is Adj + Ctl {
            body ... {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled oracle(controlRegister, target);
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
            adjoint ... {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Adjoint Controlled oracle(controlRegister, target);
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
            controlled (ctls, ...) {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled Controlled oracle(ctls, (controlRegister, target));
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
            controlled adjoint (ctls, ...) {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled Adjoint Controlled oracle(ctls, (controlRegister, target));
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
        }
        function Length(a : ((Qubit[], Qubit) => Unit)[]) : Int {
            body intrinsic;
        }
        operation DeutschJozsa_Empty_(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            H(target);
            {
                {
                    {
                        let _array_id_272 : Qubit[] = queryRegister;
                        let _len_id_276 : Int = Length(_array_id_272);
                        mutable _index_id_281 : Int = 0;
                        while _index_id_281 < _len_id_276 {
                            let q : Qubit = _array_id_272[_index_id_281];
                            H(q);
                            _index_id_281 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    Uf(queryRegister, target);
                };
                {
                    {
                        let _array : Qubit[] = queryRegister;
                        {
                            let _range_id_300 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_303 : Int = _range_id_300::Start;
                            let _step_id_308 : Int = _range_id_300::Step;
                            let _end_id_313 : Int = _range_id_300::End;
                            while _step_id_308 > 0 and _index_id_303 <= _end_id_313 or _step_id_308 < 0 and _index_id_303 >= _end_id_313 {
                                let _index : Int = _index_id_303;
                                let q : Qubit = _array[_index];
                                Adjoint H(q);
                                _index_id_303 += _step_id_308;
                            }

                        }

                    }

                }

                _apply_res
            }

            mutable result : Bool = true;
            {
                let _array_id_343 : Qubit[] = queryRegister;
                let _len_id_347 : Int = Length(_array_id_343);
                mutable _index_id_352 : Int = 0;
                while _index_id_352 < _len_id_347 {
                    let q : Qubit = _array_id_343[_index_id_352];
                    if MResetZ(q) == One {
                        result = false;
                    }

                    _index_id_352 += 1;
                }

            }

            Reset(target);
            let _generated_ident_467 : Bool = result;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_467
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let functionsToTest : ((Qubit[], Qubit) => Unit)[] = [SimpleConstantBoolF, SimpleBalancedBoolF, ConstantBoolF, BalancedBoolF];
            {
                let _array_id_244 : ((Qubit[], Qubit) => Unit)[] = functionsToTest;
                let _len_id_248 : Int = Length(_array_id_244);
                mutable _index_id_253 : Int = 0;
                while _index_id_253 < _len_id_248 {
                    let _ : Bool = if _index_id_253 == 0 {
                        DeutschJozsa_Empty__SimpleConstantBoolF_(5)
                    } else if _index_id_253 == 1 {
                        DeutschJozsa_Empty__SimpleBalancedBoolF_(5)
                    } else if _index_id_253 == 2 {
                        DeutschJozsa_Empty__ConstantBoolF_(5)
                    } else {
                        DeutschJozsa_Empty__BalancedBoolF_(5)
                    };
                    _index_id_253 += 1;
                }

            }

        }
        operation DeutschJozsa(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            H(target);
            {
                {
                    {
                        let _array_id_272 : Qubit[] = queryRegister;
                        let _len_id_276 : Int = Length(_array_id_272);
                        mutable _index_id_281 : Int = 0;
                        while _index_id_281 < _len_id_276 {
                            let q : Qubit = _array_id_272[_index_id_281];
                            H(q);
                            _index_id_281 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    Uf(queryRegister, target);
                };
                {
                    {
                        let _array : Qubit[] = queryRegister;
                        {
                            let _range_id_300 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_303 : Int = _range_id_300::Start;
                            let _step_id_308 : Int = _range_id_300::Step;
                            let _end_id_313 : Int = _range_id_300::End;
                            while _step_id_308 > 0 and _index_id_303 <= _end_id_313 or _step_id_308 < 0 and _index_id_303 >= _end_id_313 {
                                let _index : Int = _index_id_303;
                                let q : Qubit = _array[_index];
                                Adjoint H(q);
                                _index_id_303 += _step_id_308;
                            }

                        }

                    }

                }

                _apply_res
            }

            mutable result : Bool = true;
            {
                let _array_id_343 : Qubit[] = queryRegister;
                let _len_id_347 : Int = Length(_array_id_343);
                mutable _index_id_352 : Int = 0;
                while _index_id_352 < _len_id_347 {
                    let q : Qubit = _array_id_343[_index_id_352];
                    if MResetZ(q) == One {
                        result = false;
                    }

                    _index_id_352 += 1;
                }

            }

            Reset(target);
            let _generated_ident_467 : Bool = result;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_467
        }
        operation SimpleConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            X(target);
        }
        operation SimpleBalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            CX(args[0], target);
        }
        operation ConstantBoolF(args : Qubit[], target : Qubit) : Unit {
            {
                let _range_id_371 : Range = 0..2^Length(args) - 1;
                mutable _index_id_374 : Int = _range_id_371::Start;
                let _step_id_379 : Int = _range_id_371::Step;
                let _end_id_384 : Int = _range_id_371::End;
                while _step_id_379 > 0 and _index_id_374 <= _end_id_384 or _step_id_379 < 0 and _index_id_374 >= _end_id_384 {
                    let i : Int = _index_id_374;
                    ApplyControlledOnInt_Qubit__AdjCtl__X_(i, args, target);
                    _index_id_374 += _step_id_379;
                }

            }

        }
        operation BalancedBoolF(args : Qubit[], target : Qubit) : Unit {
            {
                let _range_id_414 : Range = 0..2..2^Length(args) - 1;
                mutable _index_id_417 : Int = _range_id_414::Start;
                let _step_id_422 : Int = _range_id_414::Step;
                let _end_id_427 : Int = _range_id_414::End;
                while _step_id_422 > 0 and _index_id_417 <= _end_id_427 or _step_id_422 < 0 and _index_id_417 >= _end_id_427 {
                    let i : Int = _index_id_417;
                    ApplyControlledOnInt_Qubit__AdjCtl__X_(i, args, target);
                    _index_id_417 += _step_id_422;
                }

            }

        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyControlledOnInt_Qubit__AdjCtl_(numberState : Int, oracle : (Qubit => Unit is Adj + Ctl), controlRegister : Qubit[], target : Qubit) : Unit is Adj + Ctl {
            body ... {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled oracle(controlRegister, target);
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
            adjoint ... {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Adjoint Controlled oracle(controlRegister, target);
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
            controlled (ctls, ...) {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled Controlled oracle(ctls, (controlRegister, target));
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
            controlled adjoint (ctls, ...) {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled Adjoint Controlled oracle(ctls, (controlRegister, target));
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
        }
        function Length(a : ((Qubit[], Qubit) => Unit)[]) : Int {
            body intrinsic;
        }
        operation DeutschJozsa_Empty_(Uf : ((Qubit[], Qubit) => Unit), n : Int) : Bool {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            H(target);
            {
                {
                    {
                        let _array_id_272 : Qubit[] = queryRegister;
                        let _len_id_276 : Int = Length(_array_id_272);
                        mutable _index_id_281 : Int = 0;
                        while _index_id_281 < _len_id_276 {
                            let q : Qubit = _array_id_272[_index_id_281];
                            H(q);
                            _index_id_281 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    Uf(queryRegister, target);
                };
                {
                    {
                        let _array : Qubit[] = queryRegister;
                        {
                            let _range_id_300 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_303 : Int = _range_id_300::Start;
                            let _step_id_308 : Int = _range_id_300::Step;
                            let _end_id_313 : Int = _range_id_300::End;
                            while _step_id_308 > 0 and _index_id_303 <= _end_id_313 or _step_id_308 < 0 and _index_id_303 >= _end_id_313 {
                                let _index : Int = _index_id_303;
                                let q : Qubit = _array[_index];
                                Adjoint H(q);
                                _index_id_303 += _step_id_308;
                            }

                        }

                    }

                }

                _apply_res
            }

            mutable result : Bool = true;
            {
                let _array_id_343 : Qubit[] = queryRegister;
                let _len_id_347 : Int = Length(_array_id_343);
                mutable _index_id_352 : Int = 0;
                while _index_id_352 < _len_id_347 {
                    let q : Qubit = _array_id_343[_index_id_352];
                    if MResetZ(q) == One {
                        result = false;
                    }

                    _index_id_352 += 1;
                }

            }

            Reset(target);
            let _generated_ident_467 : Bool = result;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_467
        }
        operation ApplyControlledOnInt_Qubit__AdjCtl__X_(numberState : Int, controlRegister : Qubit[], target : Qubit) : Unit is Adj + Ctl {
            body ... {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled X(controlRegister, target);
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
            adjoint ... {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled Adjoint X(controlRegister, target);
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
            controlled (ctls, ...) {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled Controlled X(ctls, (controlRegister, target));
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
            controlled adjoint (ctls, ...) {
                {
                    {
                        ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    let _apply_res : Unit = {
                        Controlled Controlled Adjoint X(ctls, (controlRegister, target));
                    };
                    {
                        Adjoint ApplyPauliFromInt(PauliX, false, numberState, controlRegister);
                    }

                    _apply_res
                }

            }
        }
        operation DeutschJozsa_Empty__SimpleConstantBoolF_(n : Int) : Bool {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            H(target);
            {
                {
                    {
                        let _array_id_272 : Qubit[] = queryRegister;
                        let _len_id_276 : Int = Length(_array_id_272);
                        mutable _index_id_281 : Int = 0;
                        while _index_id_281 < _len_id_276 {
                            let q : Qubit = _array_id_272[_index_id_281];
                            H(q);
                            _index_id_281 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    SimpleConstantBoolF(queryRegister, target);
                };
                {
                    {
                        let _array : Qubit[] = queryRegister;
                        {
                            let _range_id_300 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_303 : Int = _range_id_300::Start;
                            let _step_id_308 : Int = _range_id_300::Step;
                            let _end_id_313 : Int = _range_id_300::End;
                            while _step_id_308 > 0 and _index_id_303 <= _end_id_313 or _step_id_308 < 0 and _index_id_303 >= _end_id_313 {
                                let _index : Int = _index_id_303;
                                let q : Qubit = _array[_index];
                                Adjoint H(q);
                                _index_id_303 += _step_id_308;
                            }

                        }

                    }

                }

                _apply_res
            }

            mutable result : Bool = true;
            {
                let _array_id_343 : Qubit[] = queryRegister;
                let _len_id_347 : Int = Length(_array_id_343);
                mutable _index_id_352 : Int = 0;
                while _index_id_352 < _len_id_347 {
                    let q : Qubit = _array_id_343[_index_id_352];
                    if MResetZ(q) == One {
                        result = false;
                    }

                    _index_id_352 += 1;
                }

            }

            Reset(target);
            let _generated_ident_467 : Bool = result;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_467
        }
        operation DeutschJozsa_Empty__SimpleBalancedBoolF_(n : Int) : Bool {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            H(target);
            {
                {
                    {
                        let _array_id_272 : Qubit[] = queryRegister;
                        let _len_id_276 : Int = Length(_array_id_272);
                        mutable _index_id_281 : Int = 0;
                        while _index_id_281 < _len_id_276 {
                            let q : Qubit = _array_id_272[_index_id_281];
                            H(q);
                            _index_id_281 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    SimpleBalancedBoolF(queryRegister, target);
                };
                {
                    {
                        let _array : Qubit[] = queryRegister;
                        {
                            let _range_id_300 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_303 : Int = _range_id_300::Start;
                            let _step_id_308 : Int = _range_id_300::Step;
                            let _end_id_313 : Int = _range_id_300::End;
                            while _step_id_308 > 0 and _index_id_303 <= _end_id_313 or _step_id_308 < 0 and _index_id_303 >= _end_id_313 {
                                let _index : Int = _index_id_303;
                                let q : Qubit = _array[_index];
                                Adjoint H(q);
                                _index_id_303 += _step_id_308;
                            }

                        }

                    }

                }

                _apply_res
            }

            mutable result : Bool = true;
            {
                let _array_id_343 : Qubit[] = queryRegister;
                let _len_id_347 : Int = Length(_array_id_343);
                mutable _index_id_352 : Int = 0;
                while _index_id_352 < _len_id_347 {
                    let q : Qubit = _array_id_343[_index_id_352];
                    if MResetZ(q) == One {
                        result = false;
                    }

                    _index_id_352 += 1;
                }

            }

            Reset(target);
            let _generated_ident_467 : Bool = result;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_467
        }
        operation DeutschJozsa_Empty__ConstantBoolF_(n : Int) : Bool {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            H(target);
            {
                {
                    {
                        let _array_id_272 : Qubit[] = queryRegister;
                        let _len_id_276 : Int = Length(_array_id_272);
                        mutable _index_id_281 : Int = 0;
                        while _index_id_281 < _len_id_276 {
                            let q : Qubit = _array_id_272[_index_id_281];
                            H(q);
                            _index_id_281 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    ConstantBoolF(queryRegister, target);
                };
                {
                    {
                        let _array : Qubit[] = queryRegister;
                        {
                            let _range_id_300 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_303 : Int = _range_id_300::Start;
                            let _step_id_308 : Int = _range_id_300::Step;
                            let _end_id_313 : Int = _range_id_300::End;
                            while _step_id_308 > 0 and _index_id_303 <= _end_id_313 or _step_id_308 < 0 and _index_id_303 >= _end_id_313 {
                                let _index : Int = _index_id_303;
                                let q : Qubit = _array[_index];
                                Adjoint H(q);
                                _index_id_303 += _step_id_308;
                            }

                        }

                    }

                }

                _apply_res
            }

            mutable result : Bool = true;
            {
                let _array_id_343 : Qubit[] = queryRegister;
                let _len_id_347 : Int = Length(_array_id_343);
                mutable _index_id_352 : Int = 0;
                while _index_id_352 < _len_id_347 {
                    let q : Qubit = _array_id_343[_index_id_352];
                    if MResetZ(q) == One {
                        result = false;
                    }

                    _index_id_352 += 1;
                }

            }

            Reset(target);
            let _generated_ident_467 : Bool = result;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_467
        }
        operation DeutschJozsa_Empty__BalancedBoolF_(n : Int) : Bool {
            let queryRegister : Qubit[] = AllocateQubitArray(n);
            let target : Qubit = __quantum__rt__qubit_allocate();
            X(target);
            H(target);
            {
                {
                    {
                        let _array_id_272 : Qubit[] = queryRegister;
                        let _len_id_276 : Int = Length(_array_id_272);
                        mutable _index_id_281 : Int = 0;
                        while _index_id_281 < _len_id_276 {
                            let q : Qubit = _array_id_272[_index_id_281];
                            H(q);
                            _index_id_281 += 1;
                        }

                    }

                }

                let _apply_res : Unit = {
                    BalancedBoolF(queryRegister, target);
                };
                {
                    {
                        let _array : Qubit[] = queryRegister;
                        {
                            let _range_id_300 : Range = Length(_array) - 1..-1..0;
                            mutable _index_id_303 : Int = _range_id_300::Start;
                            let _step_id_308 : Int = _range_id_300::Step;
                            let _end_id_313 : Int = _range_id_300::End;
                            while _step_id_308 > 0 and _index_id_303 <= _end_id_313 or _step_id_308 < 0 and _index_id_303 >= _end_id_313 {
                                let _index : Int = _index_id_303;
                                let q : Qubit = _array[_index];
                                Adjoint H(q);
                                _index_id_303 += _step_id_308;
                            }

                        }

                    }

                }

                _apply_res
            }

            mutable result : Bool = true;
            {
                let _array_id_343 : Qubit[] = queryRegister;
                let _len_id_347 : Int = Length(_array_id_343);
                mutable _index_id_352 : Int = 0;
                while _index_id_352 < _len_id_347 {
                    let q : Qubit = _array_id_343[_index_id_352];
                    if MResetZ(q) == One {
                        result = false;
                    }

                    _index_id_352 += 1;
                }

            }

            Reset(target);
            let _generated_ident_467 : Bool = result;
            __quantum__rt__qubit_release(target);
            ReleaseQubitArray(queryRegister);
            _generated_ident_467
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn full_pipeline_handles_stdlib_apply_to_each() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(H, qs);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEach_Qubit__AdjCtl_(H, qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEach_Qubit__AdjCtl__H_(qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        operation ApplyToEach_Qubit__AdjCtl__H_(register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    H(item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn full_pipeline_handles_stdlib_apply_to_each_with_custom_intrinsic() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(SX, qs);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEach_Qubit__AdjCtl_(SX, qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEach_Qubit__AdjCtl__SX_(qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        operation ApplyToEach_Qubit__AdjCtl__SX_(register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    SX(item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn apply_to_each_body_callable_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(H, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEach_Qubit__AdjCtl_(H, qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEach_Qubit__AdjCtl__H_(qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        operation ApplyToEach_Qubit__AdjCtl__H_(register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    H(item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn apply_to_each_a_adjoint_callable_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEachA(S, qs);
            Adjoint ApplyToEachA(S, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEachA_Qubit__AdjCtl_(S, qs);
            Adjoint ApplyToEachA_Qubit__AdjCtl_(S, qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEachA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj {
            body ... {
                {
                    let _array_id_46215 : Qubit[] = register;
                    let _len_id_46219 : Int = Length(_array_id_46215);
                    mutable _index_id_46224 : Int = 0;
                    while _index_id_46224 < _len_id_46219 {
                        let item : Qubit = _array_id_46215[_index_id_46224];
                        singleElementOperation(item);
                        _index_id_46224 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46243 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46246 : Int = _range_id_46243::Start;
                        let _step_id_46251 : Int = _range_id_46243::Step;
                        let _end_id_46256 : Int = _range_id_46243::End;
                        while _step_id_46251 > 0 and _index_id_46246 <= _end_id_46256 or _step_id_46251 < 0 and _index_id_46246 >= _end_id_46256 {
                            let _index : Int = _index_id_46246;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46246 += _step_id_46251;
                        }

                    }

                }

            }
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEachA_Qubit__AdjCtl__S_(qs);
            Adjoint ApplyToEachA_Qubit__AdjCtl__S_(qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEachA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj {
            body ... {
                {
                    let _array_id_46215 : Qubit[] = register;
                    let _len_id_46219 : Int = Length(_array_id_46215);
                    mutable _index_id_46224 : Int = 0;
                    while _index_id_46224 < _len_id_46219 {
                        let item : Qubit = _array_id_46215[_index_id_46224];
                        singleElementOperation(item);
                        _index_id_46224 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46243 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46246 : Int = _range_id_46243::Start;
                        let _step_id_46251 : Int = _range_id_46243::Step;
                        let _end_id_46256 : Int = _range_id_46243::End;
                        while _step_id_46251 > 0 and _index_id_46246 <= _end_id_46256 or _step_id_46251 < 0 and _index_id_46246 >= _end_id_46256 {
                            let _index : Int = _index_id_46246;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46246 += _step_id_46251;
                        }

                    }

                }

            }
        }
        operation ApplyToEachA_Qubit__AdjCtl__S_(register : Qubit[]) : Unit is Adj {
            body ... {
                {
                    let _array_id_46215 : Qubit[] = register;
                    let _len_id_46219 : Int = Length(_array_id_46215);
                    mutable _index_id_46224 : Int = 0;
                    while _index_id_46224 < _len_id_46219 {
                        let item : Qubit = _array_id_46215[_index_id_46224];
                        S(item);
                        _index_id_46224 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46243 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46246 : Int = _range_id_46243::Start;
                        let _step_id_46251 : Int = _range_id_46243::Step;
                        let _end_id_46256 : Int = _range_id_46243::End;
                        while _step_id_46251 > 0 and _index_id_46246 <= _end_id_46256 or _step_id_46251 < 0 and _index_id_46246 >= _end_id_46256 {
                            let _index : Int = _index_id_46246;
                            let item : Qubit = _array[_index];
                            Adjoint S(item);
                            _index_id_46246 += _step_id_46251;
                        }

                    }

                }

            }
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn apply_to_each_c_controlled_callable_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use (ctl, qs) = (Qubit(), Qubit[3]);
            ApplyToEachC(X, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let _generated_ident_25 : Qubit = __quantum__rt__qubit_allocate();
            let _generated_ident_27 : Qubit[] = AllocateQubitArray(3);
            let (ctl : Qubit, qs : Qubit[]) = (_generated_ident_25, _generated_ident_27);
            ApplyToEachC_Qubit__AdjCtl_(X, qs);
            ReleaseQubitArray(_generated_ident_27);
            __quantum__rt__qubit_release(_generated_ident_25);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEachC_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Ctl {
            body ... {
                {
                    let _array_id_46286 : Qubit[] = register;
                    let _len_id_46290 : Int = Length(_array_id_46286);
                    mutable _index_id_46295 : Int = 0;
                    while _index_id_46295 < _len_id_46290 {
                        let item : Qubit = _array_id_46286[_index_id_46295];
                        singleElementOperation(item);
                        _index_id_46295 += 1;
                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46314 : Qubit[] = register;
                    let _len_id_46318 : Int = Length(_array_id_46314);
                    mutable _index_id_46323 : Int = 0;
                    while _index_id_46323 < _len_id_46318 {
                        let item : Qubit = _array_id_46314[_index_id_46323];
                        Controlled singleElementOperation(ctls, item);
                        _index_id_46323 += 1;
                    }

                }

            }
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let _generated_ident_25 : Qubit = __quantum__rt__qubit_allocate();
            let _generated_ident_27 : Qubit[] = AllocateQubitArray(3);
            let (ctl : Qubit, qs : Qubit[]) = (_generated_ident_25, _generated_ident_27);
            ApplyToEachC_Qubit__AdjCtl__X_(qs);
            ReleaseQubitArray(_generated_ident_27);
            __quantum__rt__qubit_release(_generated_ident_25);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEachC_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Ctl {
            body ... {
                {
                    let _array_id_46286 : Qubit[] = register;
                    let _len_id_46290 : Int = Length(_array_id_46286);
                    mutable _index_id_46295 : Int = 0;
                    while _index_id_46295 < _len_id_46290 {
                        let item : Qubit = _array_id_46286[_index_id_46295];
                        singleElementOperation(item);
                        _index_id_46295 += 1;
                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46314 : Qubit[] = register;
                    let _len_id_46318 : Int = Length(_array_id_46314);
                    mutable _index_id_46323 : Int = 0;
                    while _index_id_46323 < _len_id_46318 {
                        let item : Qubit = _array_id_46314[_index_id_46323];
                        Controlled singleElementOperation(ctls, item);
                        _index_id_46323 += 1;
                    }

                }

            }
        }
        operation ApplyToEachC_Qubit__AdjCtl__X_(register : Qubit[]) : Unit is Ctl {
            body ... {
                {
                    let _array_id_46286 : Qubit[] = register;
                    let _len_id_46290 : Int = Length(_array_id_46286);
                    mutable _index_id_46295 : Int = 0;
                    while _index_id_46295 < _len_id_46290 {
                        let item : Qubit = _array_id_46286[_index_id_46295];
                        X(item);
                        _index_id_46295 += 1;
                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46314 : Qubit[] = register;
                    let _len_id_46318 : Int = Length(_array_id_46314);
                    mutable _index_id_46323 : Int = 0;
                    while _index_id_46323 < _len_id_46318 {
                        let item : Qubit = _array_id_46314[_index_id_46323];
                        Controlled X(ctls, item);
                        _index_id_46323 += 1;
                    }

                }

            }
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn apply_to_each_ca_callable_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEachCA(S, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEachCA_Qubit__AdjCtl_(S, qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEachCA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        singleElementOperation(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled singleElementOperation(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint singleElementOperation(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEachCA_Qubit__AdjCtl__S_(qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEachCA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        singleElementOperation(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled singleElementOperation(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint singleElementOperation(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        operation ApplyToEachCA_Qubit__AdjCtl__S_(register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        S(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint S(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled S(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint S(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn cross_package_apply_to_each_closure_arg_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            let angle = 1.0;
            ApplyToEach(q => Rx(angle, q), qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            let angle : Double = 1.;
            ApplyToEach_Qubit__Empty_(/ * closure item = 2 captures = [angle] * / _lambda_, qs);
            ReleaseQubitArray(qs);
        }
        operation _lambda_(angle : Double, q : Qubit) : Unit {
            Rx(angle, q)
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__Empty_(singleElementOperation : (Qubit => Unit), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            let angle : Double = 1.;
            ApplyToEach_Qubit__Empty__closure_(qs, angle);
            ReleaseQubitArray(qs);
        }
        operation _lambda_(angle : Double, q : Qubit) : Unit {
            Rx(angle, q)
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__Empty_(singleElementOperation : (Qubit => Unit), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        operation ApplyToEach_Qubit__Empty__closure_(register : Qubit[], __capture_0 : Double) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    _lambda_(__capture_0, item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn cross_package_apply_to_each_adjoint_arg_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            ApplyToEach(Adjoint S, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEach_Qubit__AdjCtl_(Adjoint S, qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ApplyToEach_Qubit__AdjCtl__Adj_S_(qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEach_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    singleElementOperation(item);
                    _index_id_46196 += 1;
                }

            }

        }
        operation ApplyToEach_Qubit__AdjCtl__Adj_S_(register : Qubit[]) : Unit {
            {
                let _array_id_46187 : Qubit[] = register;
                let _len_id_46191 : Int = Length(_array_id_46187);
                mutable _index_id_46196 : Int = 0;
                while _index_id_46196 < _len_id_46191 {
                    let item : Qubit = _array_id_46187[_index_id_46196];
                    Adjoint S(item);
                    _index_id_46196 += 1;
                }

            }

        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn adjoint_cross_package_apply_to_each_ca_defunctionalizes() {
    let source = r#"
        open Std.Canon;
        operation Main() : Unit {
            use qs = Qubit[3];
            Adjoint ApplyToEachCA(S, qs);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            Adjoint ApplyToEachCA_Qubit__AdjCtl_(S, qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEachCA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        singleElementOperation(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled singleElementOperation(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint singleElementOperation(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            Adjoint ApplyToEachCA_Qubit__AdjCtl__S_(qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEachCA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        singleElementOperation(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled singleElementOperation(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint singleElementOperation(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        operation ApplyToEachCA_Qubit__AdjCtl__S_(register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        S(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint S(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled S(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint S(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn controlled_apply_to_each_ca_keeps_body_callable_static() {
    let source = r#"
        open Std.Canon;

        operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            ApplyToEachCA(H, inputQubits);
        }

        operation PrepareAllOnes(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            ApplyToEachCA(X, inputQubits);
        }

        @EntryPoint()
        operation Main() : Unit {
            use qs = Qubit[3];
            let register = [qs[1], qs[2]];
            Controlled PrepareUniform([qs[0]], register);
            Controlled PrepareAllOnes([qs[0]], register);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                ApplyToEachCA_Qubit__AdjCtl_(H, inputQubits);
            }
            adjoint ... {
                Adjoint ApplyToEachCA_Qubit__AdjCtl_(H, inputQubits);
            }
            controlled (ctls, ...) {
                Controlled ApplyToEachCA_Qubit__AdjCtl_(ctls, (H, inputQubits));
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint ApplyToEachCA_Qubit__AdjCtl_(ctls, (H, inputQubits));
            }
        }
        operation PrepareAllOnes(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                ApplyToEachCA_Qubit__AdjCtl_(X, inputQubits);
            }
            adjoint ... {
                Adjoint ApplyToEachCA_Qubit__AdjCtl_(X, inputQubits);
            }
            controlled (ctls, ...) {
                Controlled ApplyToEachCA_Qubit__AdjCtl_(ctls, (X, inputQubits));
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint ApplyToEachCA_Qubit__AdjCtl_(ctls, (X, inputQubits));
            }
        }
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            let register : Qubit[] = [qs[1], qs[2]];
            Controlled PrepareUniform([qs[0]], register);
            Controlled PrepareAllOnes([qs[0]], register);
            ReleaseQubitArray(qs);
        }
        operation ApplyToEachCA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        singleElementOperation(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled singleElementOperation(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint singleElementOperation(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation PrepareUniform(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                ApplyToEachCA_Qubit__AdjCtl__H_(inputQubits);
            }
            adjoint ... {
                Adjoint ApplyToEachCA_Qubit__AdjCtl__H_(inputQubits);
            }
            controlled (ctls, ...) {
                Controlled ApplyToEachCA_Qubit__AdjCtl__H_(ctls, inputQubits);
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint ApplyToEachCA_Qubit__AdjCtl__H_(ctls, inputQubits);
            }
        }
        operation PrepareAllOnes(inputQubits : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                ApplyToEachCA_Qubit__AdjCtl__X_(inputQubits);
            }
            adjoint ... {
                Adjoint ApplyToEachCA_Qubit__AdjCtl__X_(inputQubits);
            }
            controlled (ctls, ...) {
                Controlled ApplyToEachCA_Qubit__AdjCtl__X_(ctls, inputQubits);
            }
            controlled adjoint (ctls, ...) {
                Controlled Adjoint ApplyToEachCA_Qubit__AdjCtl__X_(ctls, inputQubits);
            }
        }
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            let register : Qubit[] = [qs[1], qs[2]];
            Controlled PrepareUniform([qs[0]], register);
            Controlled PrepareAllOnes([qs[0]], register);
            ReleaseQubitArray(qs);
        }
        operation ApplyToEachCA_Qubit__AdjCtl_(singleElementOperation : (Qubit => Unit is Adj + Ctl), register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        singleElementOperation(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint singleElementOperation(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled singleElementOperation(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint singleElementOperation(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ApplyToEachCA_Qubit__AdjCtl__X_(register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        X(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint X(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled X(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint X(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        operation ApplyToEachCA_Qubit__AdjCtl__H_(register : Qubit[]) : Unit is Adj + Ctl {
            body ... {
                {
                    let _array_id_46342 : Qubit[] = register;
                    let _len_id_46346 : Int = Length(_array_id_46342);
                    mutable _index_id_46351 : Int = 0;
                    while _index_id_46351 < _len_id_46346 {
                        let item : Qubit = _array_id_46342[_index_id_46351];
                        H(item);
                        _index_id_46351 += 1;
                    }

                }

            }
            adjoint ... {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46370 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46373 : Int = _range_id_46370::Start;
                        let _step_id_46378 : Int = _range_id_46370::Step;
                        let _end_id_46383 : Int = _range_id_46370::End;
                        while _step_id_46378 > 0 and _index_id_46373 <= _end_id_46383 or _step_id_46378 < 0 and _index_id_46373 >= _end_id_46383 {
                            let _index : Int = _index_id_46373;
                            let item : Qubit = _array[_index];
                            Adjoint H(item);
                            _index_id_46373 += _step_id_46378;
                        }

                    }

                }

            }
            controlled (ctls, ...) {
                {
                    let _array_id_46413 : Qubit[] = register;
                    let _len_id_46417 : Int = Length(_array_id_46413);
                    mutable _index_id_46422 : Int = 0;
                    while _index_id_46422 < _len_id_46417 {
                        let item : Qubit = _array_id_46413[_index_id_46422];
                        Controlled H(ctls, item);
                        _index_id_46422 += 1;
                    }

                }

            }
            controlled adjoint (ctls, ...) {
                {
                    let _array : Qubit[] = register;
                    {
                        let _range_id_46441 : Range = Length(_array) - 1..-1..0;
                        mutable _index_id_46444 : Int = _range_id_46441::Start;
                        let _step_id_46449 : Int = _range_id_46441::Step;
                        let _end_id_46454 : Int = _range_id_46441::End;
                        while _step_id_46449 > 0 and _index_id_46444 <= _end_id_46454 or _step_id_46449 < 0 and _index_id_46444 >= _end_id_46454 {
                            let _index : Int = _index_id_46444;
                            let item : Qubit = _array[_index];
                            Controlled Adjoint H(ctls, item);
                            _index_id_46444 += _step_id_46449;
                        }

                    }

                }

            }
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn cross_package_mapped_defunctionalizes() {
    let source = r#"
        open Std.Arrays;
        function Double(x : Int) : Int { x * 2 }
        @EntryPoint()
        operation Main() : Unit {
            let arr = [1, 2, 3];
            let _ = Mapped(Double, arr);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        function Double(x : Int) : Int {
            x * 2
        }
        operation Main() : Unit {
            let arr : Int[] = [1, 2, 3];
            let _ : Int[] = Mapped_Int__Int_(Double, arr);
        }
        function Mapped_Int__Int_(mapper : (Int -> Int), array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [mapper(element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        // entry
        Main()

        AFTER:
        // namespace test
        function Double(x : Int) : Int {
            x * 2
        }
        operation Main() : Unit {
            let arr : Int[] = [1, 2, 3];
            let _ : Int[] = Mapped_Int__Int__Double_(arr);
        }
        function Mapped_Int__Int_(mapper : (Int -> Int), array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [mapper(element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        function Mapped_Int__Int__Double_(array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [Double(element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn cross_package_for_each_defunctionalizes() {
    let source = r#"
        open Std.Arrays;
        operation Main() : Unit {
            use qs = Qubit[3];
            ForEach(H, qs);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ForEach_Qubit__Unit__AdjCtl_(H, qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ForEach_Qubit__Unit__AdjCtl_(action : (Qubit => Unit is Adj + Ctl), array : Qubit[]) : Unit[] {
            mutable output : Unit[] = [];
            {
                let _array_id_45473 : Qubit[] = array;
                let _len_id_45477 : Int = Length(_array_id_45473);
                mutable _index_id_45482 : Int = 0;
                while _index_id_45482 < _len_id_45477 {
                    let element : Qubit = _array_id_45473[_index_id_45482];
                    output += [action(element)];
                    _index_id_45482 += 1;
                }

            }

            output
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let qs : Qubit[] = AllocateQubitArray(3);
            ForEach_Qubit__Unit__AdjCtl__H_(qs);
            ReleaseQubitArray(qs);
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        operation ForEach_Qubit__Unit__AdjCtl_(action : (Qubit => Unit is Adj + Ctl), array : Qubit[]) : Unit[] {
            mutable output : Unit[] = [];
            {
                let _array_id_45473 : Qubit[] = array;
                let _len_id_45477 : Int = Length(_array_id_45473);
                mutable _index_id_45482 : Int = 0;
                while _index_id_45482 < _len_id_45477 {
                    let element : Qubit = _array_id_45473[_index_id_45482];
                    output += [action(element)];
                    _index_id_45482 += 1;
                }

            }

            output
        }
        operation ForEach_Qubit__Unit__AdjCtl__H_(array : Qubit[]) : Unit[] {
            mutable output : Unit[] = [];
            {
                let _array_id_45473 : Qubit[] = array;
                let _len_id_45477 : Int = Length(_array_id_45473);
                mutable _index_id_45482 : Int = 0;
                while _index_id_45482 < _len_id_45477 {
                    let element : Qubit = _array_id_45473[_index_id_45482];
                    output += [H(element)];
                    _index_id_45482 += 1;
                }

            }

            output
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn stdlib_hof_specialized_with_concrete_callable() {
    let source = r#"
        open Microsoft.Quantum.Arrays;

        operation Main() : Int[] {
            let arr = [1, 2, 3];
            Mapped(x -> x + 1, arr)
        }
        "#;
    check(
        source,
        &expect![[r#"
            <lambda>: input_ty=(Int,)
            Length: input_ty=(Int)[]
            Main: input_ty=Unit
            Mapped<Int, Int>{closure}: input_ty=(Int)[]"#]],
    );
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Int[] {
            let arr : Int[] = [1, 2, 3];
            Mapped_Int__Int_(/ * closure item = 2 captures = [] * / _lambda_, arr)
        }
        function _lambda_(x : Int, ) : Int {
            x + 1
        }
        function Mapped_Int__Int_(mapper : (Int -> Int), array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [mapper(element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Int[] {
            let arr : Int[] = [1, 2, 3];
            Mapped_Int__Int__closure_(arr)
        }
        function _lambda_(x : Int, ) : Int {
            x + 1
        }
        function Mapped_Int__Int_(mapper : (Int -> Int), array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [mapper(element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        function Mapped_Int__Int__closure_(array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [_lambda_(element, )];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn lambda_expression_sample_shape_has_no_defunctionalization_errors() {
    let source = r#"
        import Std.Arrays.*;

        operation Main() : Unit {
            let add = (x, y) -> x + y;
            let _ = add(2, 3);

            use control = Qubit();
            let cnotOnControl = q => CNOT(control, q);

            let intArray = [1, 2, 3, 4, 5];
            let _ = Fold(add, 0, intArray);
            let _ = Mapped(x -> x + 1, intArray);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        operation Main() : Unit {
            let add : ((Int, Int) -> Int) = / * closure item = 2 captures = [] * / _lambda_;
            let _ : Int = add(2, 3);
            let control : Qubit = __quantum__rt__qubit_allocate();
            let cnotOnControl : (Qubit => Unit) = / * closure item = 3 captures = [control] * / _lambda_;
            let intArray : Int[] = [1, 2, 3, 4, 5];
            let _ : Int = Fold_Int__Int_(add, 0, intArray);
            let _ : Int[] = Mapped_Int__Int_(/ * closure item = 4 captures = [] * / _lambda_, intArray);
            __quantum__rt__qubit_release(control);
        }
        function _lambda_((x : Int, y : Int), ) : Int {
            x + y
        }
        operation _lambda_(control : Qubit, q : Qubit) : Unit {
            CNOT(control, q)
        }
        function _lambda_(x : Int, ) : Int {
            x + 1
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        function Fold_Int__Int_(folder : ((Int, Int) -> Int), state : Int, array : Int[]) : Int {
            mutable current : Int = state;
            {
                let _array_id_45445 : Int[] = array;
                let _len_id_45449 : Int = Length(_array_id_45445);
                mutable _index_id_45454 : Int = 0;
                while _index_id_45454 < _len_id_45449 {
                    let element : Int = _array_id_45445[_index_id_45454];
                    current = folder(current, element);
                    _index_id_45454 += 1;
                }

            }

            current
        }
        function Mapped_Int__Int_(mapper : (Int -> Int), array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [mapper(element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        // entry
        Main()

        AFTER:
        // namespace test
        operation Main() : Unit {
            let _ : Int = _lambda_((2, 3), );
            let control : Qubit = __quantum__rt__qubit_allocate();
            let intArray : Int[] = [1, 2, 3, 4, 5];
            let _ : Int = Fold_Int__Int__closure_(0, intArray);
            let _ : Int[] = Mapped_Int__Int__closure_(intArray);
            __quantum__rt__qubit_release(control);
        }
        function _lambda_((x : Int, y : Int), ) : Int {
            x + y
        }
        operation _lambda_(control : Qubit, q : Qubit) : Unit {
            CNOT(control, q)
        }
        function _lambda_(x : Int, ) : Int {
            x + 1
        }
        function Length(a : Qubit[]) : Int {
            body intrinsic;
        }
        function Fold_Int__Int_(folder : ((Int, Int) -> Int), state : Int, array : Int[]) : Int {
            mutable current : Int = state;
            {
                let _array_id_45445 : Int[] = array;
                let _len_id_45449 : Int = Length(_array_id_45445);
                mutable _index_id_45454 : Int = 0;
                while _index_id_45454 < _len_id_45449 {
                    let element : Int = _array_id_45445[_index_id_45454];
                    current = folder(current, element);
                    _index_id_45454 += 1;
                }

            }

            current
        }
        function Mapped_Int__Int_(mapper : (Int -> Int), array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [mapper(element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        function Fold_Int__Int__closure_(state : Int, array : Int[]) : Int {
            mutable current : Int = state;
            {
                let _array_id_45445 : Int[] = array;
                let _len_id_45449 : Int = Length(_array_id_45445);
                mutable _index_id_45454 : Int = 0;
                while _index_id_45454 < _len_id_45449 {
                    let element : Int = _array_id_45445[_index_id_45454];
                    current = _lambda_((current, element), );
                    _index_id_45454 += 1;
                }

            }

            current
        }
        function Mapped_Int__Int__closure_(array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [_lambda_(element, )];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn partial_application_sample_shape_has_no_defunctionalization_errors() {
    let source = r#"
        import Std.Arrays.*;

        function Main() : Unit {
            let incrementByOne = Add(_, 1);
            let incrementByOneLambda = x -> Add(x, 1);

            let _ = incrementByOne(4);

            let sumAndAddOne = AddMany(_, _, _, 1);
            let sumAndAddOneLambda = (a, b, c) -> AddMany(a, b, c, 1);

            let intArray = [1, 2, 3, 4, 5];
            let _ = Mapped(Add(_, 1), intArray);
        }

        function Add(x : Int, y : Int) : Int {
            return x + y;
        }

        function AddMany(a : Int, b : Int, c : Int, d : Int) : Int {
            return a + b + c + d;
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_rewrite(
        source,
        &expect![[r#"
        BEFORE:
        // namespace test
        function Main() : Unit {
            let incrementByOne : (Int -> Int) = {
                let arg : Int = 1;
                / * closure item = 4 captures = [arg] * / _lambda_
            };
            let incrementByOneLambda : (Int -> Int) = / * closure item = 5 captures = [] * / _lambda_;
            let _ : Int = incrementByOne(4);
            let sumAndAddOne : ((Int, Int, Int) -> Int) = {
                let arg : Int = 1;
                / * closure item = 6 captures = [arg] * / _lambda_
            };
            let sumAndAddOneLambda : ((Int, Int, Int) -> Int) = / * closure item = 7 captures = [] * / _lambda_;
            let intArray : Int[] = [1, 2, 3, 4, 5];
            let _ : Int[] = Mapped_Int__Int_({
                let arg : Int = 1;
                / * closure item = 8 captures = [arg] * / _lambda_
            }, intArray);
        }
        function Add(x : Int, y : Int) : Int {
            return x + y;
        }
        function AddMany(a : Int, b : Int, c : Int, d : Int) : Int {
            return a + b + c + d;
        }
        function _lambda_(arg : Int, hole : Int) : Int {
            Add(hole, arg)
        }
        function _lambda_(x : Int, ) : Int {
            Add(x, 1)
        }
        function _lambda_(arg : Int, (hole : Int, hole : Int, hole : Int)) : Int {
            AddMany(hole, hole, hole, arg)
        }
        function _lambda_((a : Int, b : Int, c : Int), ) : Int {
            AddMany(a, b, c, 1)
        }
        function _lambda_(arg : Int, hole : Int) : Int {
            Add(hole, arg)
        }
        function Mapped_Int__Int_(mapper : (Int -> Int), array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [mapper(element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        // entry
        Main()

        AFTER:
        // namespace test
        function Main() : Unit {
            let _ : Int = _lambda_(1, 4);
            let intArray : Int[] = [1, 2, 3, 4, 5];
            let _ : Int[] = Mapped_Int__Int__closure_(intArray, 1);
        }
        function Add(x : Int, y : Int) : Int {
            return x + y;
        }
        function AddMany(a : Int, b : Int, c : Int, d : Int) : Int {
            return a + b + c + d;
        }
        function _lambda_(arg : Int, hole : Int) : Int {
            Add(hole, arg)
        }
        function _lambda_(x : Int, ) : Int {
            Add(x, 1)
        }
        function _lambda_(arg : Int, (hole : Int, hole : Int, hole : Int)) : Int {
            AddMany(hole, hole, hole, arg)
        }
        function _lambda_((a : Int, b : Int, c : Int), ) : Int {
            AddMany(a, b, c, 1)
        }
        function _lambda_(arg : Int, hole : Int) : Int {
            Add(hole, arg)
        }
        function Mapped_Int__Int_(mapper : (Int -> Int), array : Int[]) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [mapper(element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        function Length(a : Int[]) : Int {
            body intrinsic;
        }
        function Mapped_Int__Int__closure_(array : Int[], __capture_0 : Int) : Int[] {
            mutable mapped : Int[] = [];
            {
                let _array_id_45701 : Int[] = array;
                let _len_id_45705 : Int = Length(_array_id_45701);
                mutable _index_id_45710 : Int = 0;
                while _index_id_45710 < _len_id_45705 {
                    let element : Int = _array_id_45701[_index_id_45710];
                    mapped += [_lambda_(__capture_0, element)];
                    _index_id_45710 += 1;
                }

            }

            mapped
        }
        // entry
        Main()
    "#]],
    );
}

#[test]
fn cross_package_callable_value_defunctionalized() {
    let lib_source = indoc! {"
        namespace TestLib {
            function ApplyFunc(f: Int -> Int, x: Int) : Int { f(x) }
            function Double(x: Int) : Int { x * 2 }
            export ApplyFunc, Double;
        }
    "};

    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            ApplyFunc(Double, 5)
        }
    "};

    let (_store, _pkg_id) = crate::test_utils::compile_and_run_pipeline_to_with_library(
        lib_source,
        user_source,
        crate::test_utils::PipelineStage::Defunc,
    );
}

#[test]
fn cross_package_callable_value_semantic_equivalence() {
    let lib_source = indoc! {"
        namespace TestLib {
            function ApplyFunc(f: Int -> Int, x: Int) : Int { f(x) }
            function Double(x: Int) : Int { x * 2 }
            export ApplyFunc, Double;
        }
    "};

    let user_source = indoc! {"
        import TestLib.*;
        @EntryPoint()
        operation Main() : Int {
            ApplyFunc(Double, 5)
        }
    "};

    crate::test_utils::check_semantic_equivalence_with_library(lib_source, user_source);
}
