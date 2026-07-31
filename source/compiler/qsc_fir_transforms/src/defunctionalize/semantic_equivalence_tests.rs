// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use indoc::formatdoc;
use proptest::prelude::*;

/// Regression for controlled dispatch of a *capturing* closure passed to a
/// higher-order operation whose callable parameter is **not** the first
/// argument. The HOF applies `Controlled op(ctls, q)`, so rewrite must nest the
/// closure's captures inside the base input tuple beneath the control register
/// (`([ctls], (q, capture0, capture1))`) rather than appending them as trailing
/// top-level siblings of `([ctls], q)`. A mis-placed capture would either crash
/// downstream control/input splitting or diverge from the original semantics.
///
/// The control qubit is prepared |1> so the controlled rotation actually fires;
/// the captured angles are threaded through a partial application so the closure
/// carries two ordered captures across the control boundary (exercising the
/// multi-capture nesting order, not just placement).
#[test]
fn controlled_capturing_closure_nonzero_param_slot_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation RotOp(a : Double, b : Double, q : Qubit) : Unit is Adj + Ctl {
                Rx(a, q);
                Rz(b, q);
            }
            operation ApplyCtl(ctls : Qubit[], op : Qubit => Unit is Ctl, q : Qubit) : Unit {
                Controlled op(ctls, q);
            }
            @EntryPoint()
            operation Main() : Result {
                use ctl = Qubit();
                use q = Qubit();
                X(ctl);
                let a = 3.141592653589793;
                let b = 1.5707963267948966;
                let op = RotOp(a, b, _);
                ApplyCtl([ctl], op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for recorded direct-rewrite cleanup. `GetOp(q)` performs `X(q)`
/// before returning the named callable `X`; direct dispatch consumes `op`, so
/// the cleanup must retain the now-unused immutable binding and its effect.
#[test]
fn recorded_direct_rewrite_cleanup_retains_effectful_callable_initializer() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetOp(q : Qubit) : (Qubit => Unit) {
                X(q);
                X
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = GetOp(q);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for the removal gate on a rewritten higher-order argument. The
/// factory is a pure `function`, so its consumed result makes the binding a
/// deletion candidate, but its body can still fail. Cleanup must keep the
/// binding so the division failure stays observable.
#[test]
fn rewritten_hof_arg_cleanup_retains_fallible_function_factory() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function GetOp(divisor : Int) : Qubit => Unit {
                let ignored = 1 / divisor;
                X
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = GetOp(0);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for demoting a dead callable binding that must still run. The
/// captured angle comes from `GetAngle`, which flips the qubit, so dropping the
/// binding outright would lose that effect. Cleanup keeps the evaluation and
/// discards only the consumed callable value.
#[test]
fn demoted_dead_callable_binding_retains_capture_effect() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetAngle(q : Qubit) : Double {
                X(q);
                0.0
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = Rx(GetAngle(q), _);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for `prune_dead_callable_locals_in_block`. The initializer is
/// intentionally unused and never passed to a higher-order operation, so it
/// must be retained by the global dead-local pruner solely for its `X(q)`.
#[test]
fn global_dead_local_pruner_retains_effectful_callable_initializer() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetOp(q : Qubit) : (Qubit => Unit) {
                X(q);
                X
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let unused = GetOp(q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for the orphaned-producer skip. `MakeOp` is a pure producer whose
/// callable result is consumed by specialization, so the binding in `Main`
/// disappears and `MakeOp` loses its only caller. Cleanup no longer visits it,
/// which leaves the closure in its body untouched to disappear with the item at
/// DCE.
///
/// The caller keeps observable evaluation on both sides of the consumed call,
/// and `ApplyOp` wraps the dispatched operation in its own gates, so the effect
/// trace pins the count and the order of `X`, `H`, `Rx`, `H`, `Y`. Structure
/// alone cannot catch a producer whose evaluation is dropped or replayed here;
/// the trace can.
#[test]
fn orphaned_producer_body_preserves_caller_evaluation_order() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation InnerOp(angle : Double, q : Qubit) : Unit {
                Rx(angle, q);
            }
            function MakeOp(angle : Double) : Qubit => Unit {
                return InnerOp(angle, _);
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                X(q);
                let op = MakeOp(1.5707963267948966);
                ApplyOp(op, q);
                Y(q);
                MResetZ(q)
            }
        }
    "#});
}

#[test]
fn producer_factory_unsafe_expressions_preserve_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation Mark(enabled : Bool, q : Qubit) : Unit {
                if enabled {
                    X(q);
                }
            }
            function Make(enabled : Bool) : Qubit => Unit {
                Mark(enabled, _)
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                mutable enabled = false;
                let op = Make(enabled);
                set enabled = true;
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function Choose(flag : Bool) : Qubit => Unit {
                if not flag {
                    X
                } else {
                    Z
                }
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let source = false;
                let op = Choose(source);
                ApplyOp(op, q);
                MResetZ(q)
            }
        }
    "#});
}

/// Regression for the aggregate-slot replacement: a consumed closure that is a
/// direct element of a UDT constructor's argument tuple. Both branches of
/// `Choose` are taken so each closure is specialized, and every read of the
/// arrow-typed `F` field is rewritten to a direct call. `Offset` is read too, so
/// the constructor call stays entry-reachable and cleanup replaces the closure
/// inside it. The closures capture nothing, so the replacement is a reference to
/// each closure's own target callable and the slot keeps its arrow type.
///
/// The caller drives quantum effects from the dispatched results, so the effect
/// trace pins how many times each specialized function ran and in what order:
/// `fT(2)` is 3 and `fF(2)` is 4, giving `X`, three `H`, `Z`, four `Y`. Dropping
/// or replaying a dispatch changes the gate counts, which structure alone would
/// not reveal. The returned sum independently pins the second pair of
/// applications and both surviving non-arrow fields.
#[test]
fn aggregate_slot_capture_free_replacement_preserves_dispatch_count_and_order() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            newtype Choice = (F : Int -> Int, Offset : Int);

            function Choose(flag : Bool) : Choice {
                if flag {
                    Choice(x -> x + 1, 100)
                } else {
                    Choice(x -> x * 2, 7)
                }
            }

            @EntryPoint()
            operation Main() : Int {
                use q = Qubit();
                let selectedT = Choose(true);
                let selectedF = Choose(false);
                let fT = selectedT::F;
                let fF = selectedF::F;
                X(q);
                for _ in 1..fT(2) {
                    H(q);
                }
                Z(q);
                for _ in 1..fF(2) {
                    Y(q);
                }
                Reset(q);
                fT(10) + fF(10) + selectedT::Offset + selectedF::Offset
            }
        }
    "#});
}

/// The capturing counterpart of the test above, and the shape that motivated
/// the synthesized stand-in. `Std.TableLookup.MakeAndChain` builds
/// `AndChain(depth, helper => AndChainOperation(ctls, helper, target))`: a
/// closure capturing two values, sitting directly in a UDT-constructor argument
/// tuple, in a body that stays entry-reachable. There is no capture-free target
/// to name, so cleanup must replace it with a fail-bodied stand-in of the same
/// arrow type.
///
/// `Select` is driven with the address register prepared |1>, so the lookup
/// resolves to `data[1] = [true]` and the returned measurement is deterministic.
/// The effect trace pins the whole gate sequence the library emits, so a
/// stand-in that was accidentally reachable, or a dispatch dropped or replayed
/// by the replacement, changes the trace even where the result would not.
#[test]
fn std_table_lookup_select_capturing_aggregate_slot_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use address = Qubit[1];
                use output = Qubit[1];
                X(address[0]);
                Std.TableLookup.Select([[false], [true]], address, output);
                let result = MResetZ(output[0]);
                ResetAll(address);
                result
            }
        }
    "#});
}

/// `EvaluationDisposition::Discarded`. `MakeOp` is a pure, total factory, so
/// deleting the consumed binding drops an evaluation that was never observable.
///
/// The trace pins `Y`, `H`, `X`, `H`, `Z`: the surrounding gates fix where the
/// dispatch lands in the order, and `ApplyOp`'s own `H` pair fixes how many
/// times it ran. A dropped, duplicated, or reordered dispatch changes the
/// sequence even though the returned value would not.
#[test]
fn discarded_disposition_drops_only_unobservable_evaluation() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function MakeOp() : Qubit => Unit {
                X
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Y(q);
                let op = MakeOp();
                ApplyOp(op, q);
                Z(q);
                MResetZ(q)
            }
        }
    "#});
}

/// Exercises `EvaluationDisposition::Relocated`. `GetAngle` flips the qubit
/// while computing the captured angle, and the rewrite splices that initializer
/// into the specialized call, so deleting the binding *moves* the flip rather
/// than dropping it.
///
/// The trace pins `Y`, `X`, `H`, `Rx`, `H`, `Z`. Dropping the binding without
/// relocating loses the `X`; retaining it after relocation runs the `X` twice.
/// Both are invisible to structure and to the returned value, and both change
/// this sequence.
#[test]
fn relocated_disposition_moves_capture_evaluation_exactly_once() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetAngle(q : Qubit) : Double {
                X(q);
                0.0
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Y(q);
                let op = Rx(GetAngle(q), _);
                ApplyOp(op, q);
                Z(q);
                MResetZ(q)
            }
        }
    "#});
}

/// `EvaluationDisposition::Replayed` by branch dispatch. The binding is a
/// static callable selection, so deleting it is sound only because
/// `branch_split_direct_call_rewrite` emits the same `if` tree at the replaced
/// call site.
///
/// The selecting condition is a measurement, which makes the replay observable:
/// the condition is not safe to discard, so the binding reaches the replay rule
/// rather than the discard rule, and the trace records where and how often the
/// measurement ran. Replaying it twice, dropping it, or moving it across the
/// surrounding `X` and `Z` all change the sequence, and none of those changes
/// alters the returned value or the transformed program's structure in a way a
/// snapshot would flag.
#[test]
fn replayed_disposition_reruns_the_branch_selection() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use flag = Qubit();
                use q = Qubit();
                X(flag);
                X(q);
                let op = if MResetZ(flag) == One { Y } else { Z };
                ApplyOp(op, q);
                Z(q);
                MResetZ(q)
            }
        }
    "#});
}

/// `EvaluationDisposition::Replayed` by index dispatch at the *argument*
/// position, the one rule that differs between the two consumption sites. The
/// rewrite resolves `ops[1]` statically and calls the selected callable
/// directly, so the selection is replayed and only the bounds check is elided.
///
/// The trace pins `Z`, `H`, `Y`, `H`. Selecting the wrong element swaps `Y` for
/// `X`; dropping the dispatch removes it entirely.
#[test]
fn replayed_index_selection_at_argument_position_preserves_dispatch() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let ops = [X, Y];
                Z(q);
                ApplyOp(ops[1], q);
                MResetZ(q)
            }
        }
    "#});
}

#[test]
fn indexed_dispatch_preserves_out_of_range_failures() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyAt(ops : (Qubit => Unit)[], idx : Int, q : Qubit) : Unit {
                ops[idx](q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                ApplyAt([Z, X], 2, q);
                MResetZ(q)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let ops = [Z, X];
                ops[2](q);
                MResetZ(q)
            }
        }
    "#});
}

#[test]
fn indexed_dispatch_preserves_duplicate_physical_positions() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use flag = Qubit();
                use target = Qubit();
                X(flag);
                let index = if MResetZ(flag) == One { 1 } else { 0 };
                let ops = [I, I, X];
                ops[index](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyAt(ops : (Qubit => Unit)[], index : Int, target : Qubit) : Unit {
                ops[index](target);
            }
            @EntryPoint()
            operation Main() : Result {
                use flag = Qubit();
                use target = Qubit();
                X(flag);
                let index = if MResetZ(flag) == One { 1 } else { 0 };
                ApplyAt([I, I, X], index, target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
fn indexed_dispatch_preserves_singleton_bounds_and_effectful_index_evaluation() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyAt(ops : (Qubit => Unit)[], index : Int, target : Qubit) : Unit {
                ops[index](target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                ApplyAt([X], 1, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let ops = [X];
                ops[1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyAt(ops : (Qubit => Unit)[], index : Int, target : Qubit) : Unit {
                ops[index](target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                ApplyAt([Z], {
                    X(target);
                    0
                }, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let ops = [Z];
                ops[{
                    X(target);
                    0
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let ops = [I, Z];
                for index in 1..1 {
                    let op = ops[{
                        X(target);
                        index
                    }];
                    op(target);
                }
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable ops = [I, Z];
                set ops = [Z, I];
                ops[{
                    X(target);
                    0
                }](target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
#[allow(clippy::too_many_lines)]
fn indexed_struct_field_source_preserves_semantics() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [Z] };
                config.Ops[{
                    X(target);
                    0
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [I, Z] };
                config.Ops[{
                    X(target);
                    1
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [X] };
                config.Ops[1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [X] };
                ApplyOp(config.Ops[1], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [Z] };
                ApplyOp(config.Ops[{
                    X(target);
                    0
                }], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyValue(value : Int, target : Qubit) : Unit {
                if value == 1 {
                    Z(target);
                }
            }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let value = 1;
                let config = new Config { Ops = [ApplyValue(value, _)] };
                ApplyOp(config.Ops[{
                    X(target);
                    0
                }], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [I, X] };
                ApplyOp(config.Ops[1], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [I, X] };
                ApplyOp(config.Ops[2], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            struct Outer { Inner : Config }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let outer = new Outer {
                    Inner = new Config { Ops = [I, Z] }
                };
                outer.Inner.Ops[{
                    X(target);
                    1
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let base = new Config { Ops = [X] };
                let config = new Config { ...base, Ops = [I, X] };
                config.Ops[1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable config = new Config { Ops = [X, I] };
                set config w/= Ops <- [I, X];
                config.Ops[0](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            newtype Wrapped = (Ops : (Qubit => Unit is Adj + Ctl)[]);
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let wrapped = Wrapped([I, X]);
                wrapped::Ops[1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [I, X] };
                ApplyBoth(config.Ops[1], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [X] };
                ApplyBoth(config.Ops[1], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let config = new Config { Ops = [X] };
                ApplyBoth(config.Ops[{
                    X(target);
                    1
                }], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use (flag, target) = (Qubit(), Qubit());
                X(flag);
                let index = if MResetZ(flag) == One { 1 } else { 0 };
                let config = new Config { Ops = [I, X] };
                X(target);
                ApplyBoth(config.Ops[index], I, target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
#[allow(clippy::too_many_lines)]
fn unresolved_indexed_struct_field_source_declines_atomically() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [Z] }, 0);
                config.Ops[{
                    X(target);
                    ignored
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [I, Z] }, 0);
                config.Ops[{
                    X(target);
                    ignored + 1
                }](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [X] }, 0);
                config.Ops[ignored + 1](target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [X] }, 0);
                ApplyOp(config.Ops[ignored + 1], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [I, Z] }, 0);
                ApplyOp(config.Ops[{
                    X(target);
                    ignored + 1
                }], target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let (config, ignored) = (new Config { Ops = [X] }, 0);
                ApplyBoth(config.Ops[ignored + 1], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyBoth(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
                first(target);
                second(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use (flag, target) = (Qubit(), Qubit());
                X(flag);
                let index = if MResetZ(flag) == One { 1 } else { 0 };
                let (config, ignored) = (new Config { Ops = [I, X] }, 0);
                X(target);
                ApplyBoth(config.Ops[index + ignored], I, target);
                MResetZ(target)
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            struct Config { Ops : (Qubit => Unit)[] }
            operation ApplyValue(value : Int, target : Qubit) : Unit {
                if value == 1 {
                    Z(target);
                }
            }
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                let value = 1;
                let (config, ignored) = (
                    new Config { Ops = [ApplyValue(value, _)] },
                    0
                );
                ApplyOp(config.Ops[{
                    X(target);
                    ignored
                }], target);
                MResetZ(target)
            }
        }
    "#});
}

/// `EvaluationDisposition::Retained`. `GetOp` applies `X` before returning the
/// named callable it produces, and nothing relocates or replays that `X`, so
/// the binding must survive even though its callable value is consumed.
///
/// The trace pins `Y`, `X`, `H`, `Z`, `H`, `Y`. Deleting the binding drops the
/// leading `X`; hoisting it past the surrounding gates reorders the sequence.
#[test]
fn retained_disposition_keeps_observable_producer_evaluation_in_place() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation GetOp(q : Qubit) : (Qubit => Unit) {
                X(q);
                Z
            }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                H(q);
                op(q);
                H(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                Y(q);
                let op = GetOp(q);
                ApplyOp(op, q);
                Y(q);
                MResetZ(q)
            }
        }
    "#});
}

/// The recursive self-call slot deleted by `remove_arg_at_path` can only hold a
/// global item reference or a closure, so the deletion discards nothing
/// observable. `Repeat`'s self-call forwards the named `H`, which is exactly the
/// slot shape `assert_discarded_slot_is_pure` states, and running the pipeline
/// exercises that assertion.
///
/// The trace pins `X`, four `H`, then `Y`. Dropping or duplicating a recursion
/// step changes the number of `H`s. The count is even so the four gates compose
/// to the identity and the measured result stays deterministic, which keeps the
/// value comparison meaningful alongside the trace comparison.
#[test]
fn recursive_self_call_slot_removal_preserves_recursion_count() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation Repeat(op : Qubit => Unit, n : Int, q : Qubit) : Unit {
                if n > 0 {
                    op(q);
                    Repeat(H, n - 1, q);
                }
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                X(q);
                Repeat(H, 4, q);
                Y(q);
                MResetZ(q)
            }
        }
    "#});
}

/// Generates syntactically valid Q# programs exercising defunctionalization's
/// key code paths: lambda arguments, partial application, and direct callable
/// references passed to higher-order functions.
fn defunc_pattern_strategy() -> impl Strategy<Value = String> {
    let val = || 0..50i64;

    prop_oneof![
        // 1. Lambda passed as argument to a higher-order function.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(x -> x + {a}, {b})
                }}
            }}
        "}),
        // 2. Partial application of a two-argument function.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function Add(x : Int, y : Int) : Int {{ x + y }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(Add({a}, _), {b})
                }}
            }}
        "}),
        // 3. Direct callable reference as argument.
        val().prop_map(|a| formatdoc! {"
            namespace Test {{
                function Double(x : Int) : Int {{ x * 2 }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(Double, {a})
                }}
            }}
        "}),
        // 4. Nested higher-order calls: function returning a lambda.
        (val(), val()).prop_map(|(a, b)| formatdoc! {"
            namespace Test {{
                function MakeAdder(n : Int) : Int -> Int {{ x -> x + n }}
                function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                function Main() : Int {{
                    Apply(MakeAdder({a}), {b})
                }}
            }}
        "}),
    ]
}

/// Generates programs with multi-capture closures where the captures have
/// distinct values and are used in non-commutative operations, ensuring
/// capture ordering is exercised.
fn multi_capture_strategy() -> impl Strategy<Value = String> {
    // Use distinct non-zero values so swapped captures produce a different result.
    (2..20i64, 1..10i64)
        .prop_filter("a must differ from b", |(a, b)| a != b && *b != 0)
        .prop_flat_map(|(a, b)| {
            prop_oneof![
                // Two captures used in non-commutative subtraction.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            Apply(x -> a - b + x, 0)
                        }}
                    }}
                "}),
                // Two captures used in non-commutative division.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            Apply(x -> a / b + x, 0)
                        }}
                    }}
                "}),
                // Three captures in position-sensitive expression.
                Just(formatdoc! {"
                    namespace Test {{
                        function Apply(f : Int -> Int, x : Int) : Int {{ f(x) }}
                        function Main() : Int {{
                            let a = {a};
                            let b = {b};
                            let c = 1;
                            Apply(x -> (a - b) * c + x, 0)
                        }}
                    }}
                "}),
            ]
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    #[test]
    fn proptest_defunctionalize_preserves_semantics(source in defunc_pattern_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(30))]
    #[test]
    fn proptest_multi_capture_ordering_preserves_semantics(source in multi_capture_strategy()) {
        crate::test_utils::check_semantic_equivalence(&source);
    }
}

/// Regression for the `Multi ⊔ Multi` (nested dispatch on both sides) join: a
/// callable-valued local is selected by an outer dynamic `if` whose *both*
/// branches are themselves dynamic conditionals, and the *same* callable (`X`)
/// reaches the local from both branches under different guards.
///
/// The lattice merge must not deduplicate the false-branch occurrence of `X`
/// by callable identity — doing so drops the `!outer && rb` dispatch arm and
/// makes that path fall through to the outer default (`Z`) instead of applying
/// `X`. The fixture pins `outer == false` (`a` stays |0>) and the false-branch
/// inner guard `rb == One` (`b` is |1>), so the dropped arm is exactly the path
/// taken: the original applies `X(q)` (measuring `One`) while the buggy rewrite
/// applies `Z(q)` (measuring `Zero`), diverging in both return value and effect
/// trace. The guards are pure reads of pre-measured `Result` locals so the
/// fixture isolates the lattice merge from condition-hoisting concerns.
#[test]
fn multi_multi_shared_callable_across_branches_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                // a stays |0> so the outer guard is false; b is |1> so the
                // false-branch inner guard is true — the dispatch arm the
                // identity-dedup would drop.
                X(b);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let op = if ra == One {
                             if rb == One { X } else { Y }
                         } else {
                             if rb == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for the `Single ⊔ Multi` join: a callable-valued local is
/// selected by an outer dynamic `if` whose *true* branch is a single concrete
/// callable (`X`) and whose *false* branch is itself a dynamic conditional that
/// can also yield `X` (under its own guard).
///
/// The lattice merge must not deduplicate the true-branch `X` against the
/// occurrence already present in the false-branch `Multi` — doing so drops the
/// `outer` dispatch arm and reroutes the `outer == true` path through the
/// false-branch's inner guards instead of unconditionally applying `X`. The
/// fixture pins `outer == true` (`a` is |1>) and the false-branch inner guard
/// `rb == One` false (`b` stays |0>), so the dropped arm is exactly the path
/// taken: the original applies `X(q)` (measuring `One`) while the buggy rewrite
/// falls through to the false-branch default `Z(q)` (measuring `Zero`),
/// diverging in both return value and effect trace. The guards are pure reads
/// of pre-measured `Result` locals so the fixture isolates the lattice merge
/// from condition-hoisting concerns.
#[test]
fn single_multi_shared_callable_across_branches_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                // a is |1> so the outer guard is true — op must be the
                // true-branch `X`; b stays |0> so the false-branch inner guard
                // is false, the arm the identity-dedup would route through.
                X(a);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let op = if ra == One {
                             X
                         } else {
                             if rb == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Regression for the `Multi ⊔ Multi` join's "unmodified variable" fast path: a
/// callable-valued local is selected by an outer dynamic `if` whose *both*
/// branches are dynamic conditionals that yield the *same set of callables*
/// (`X`/`Z`) but under *different* inner guards (`rb` in the true branch, `rc`
/// in the false branch).
///
/// The merge must not treat the two branches as an unmodified variable just
/// because the callable identities coincide — the guards differ, so keeping the
/// true-branch chain drops the outer condition and reroutes the `outer == false`
/// path through the true branch's `rb` guard instead of the false branch's `rc`
/// guard. The fixture pins `outer == false` (`a` stays |0>), `rb == One`
/// (`b` is |1>), and `rc == Zero` (`c` stays |0>): the original applies `Z(q)`
/// (measuring `Zero`) while the buggy rewrite applies `X(q)` (measuring `One`),
/// diverging in both return value and effect trace.
#[test]
fn multi_multi_same_callables_different_guards_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                use b = Qubit();
                use c = Qubit();
                // a stays |0> (outer guard false); b is |1> (rb == One);
                // c stays |0> (rc == Zero).
                X(b);
                let ra = MResetZ(a);
                let rb = MResetZ(b);
                let rc = MResetZ(c);
                let op = if ra == One {
                             if rb == One { X } else { Z }
                         } else {
                             if rc == One { X } else { Z }
                         };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

/// Probe: a conditional callable is bound from a guard variable that is then
/// mutated before the callable is applied. The original captures the callable
/// value at binding time (guard true -> `X`); a defunctionalization that
/// re-evaluates the guard at the apply site would read the mutated guard
/// (now false -> `Z`) and diverge.
///
/// The safe-degradation regression asserting the pipeline rejects this rather
/// than silently miscompiling lives in
/// `defunctionalize::tests::guard_var_reassigned_after_binding_degrades_to_dynamic`.
#[test]
fn guard_var_never_reassigned_after_binding_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj, q : Qubit) : Unit is Adj {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                use a = Qubit();
                X(a);
                let ra = MResetZ(a);
                // `flag` is mutable but never reassigned after the binding, so
                // hoisting its read to the apply site is safe and dispatch is
                // preserved.
                mutable flag = ra == One;
                let op = if flag { X } else { Z };
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});

    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            operation ApplyOp(op : Qubit => Unit is Adj + Ctl, target : Qubit) : Unit {
                op(target);
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable angle = 0.0;
                let op = Rx(angle + 0.0, _);
                set angle = 3.141592653589793;
                ApplyOp(op, target);
                MResetZ(target)
            }
        }
    "#});
}

/// Regression for the effectful-producer decline gate's *accept* side. The
/// producer is a pure `function`, so its call is deletable and the closure it
/// returns is still consumed; the gate must let that through unchanged. The
/// captured angle is observable in the final state, so an evaluation the
/// rewrite dropped, duplicated, or reordered while relocating the capture would
/// diverge.
///
/// The decline side cannot have an equivalence test: a declined shape reports a
/// fatal `DynamicCallable`, so there is no transformed program to compare. It is
/// pinned by
/// `defunctionalize::tests::invariants::effectful_producer_returning_consumed_closure_declines_to_dynamic`.
#[test]
fn pure_producer_returned_closure_consumption_is_equivalent() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function MakeRot(angle : Double) : Qubit => Unit is Adj + Ctl {
                Rx(angle, _)
            }
            operation ApplyOp(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit {
                op(q);
            }
            @EntryPoint()
            operation Main() : Result {
                use q = Qubit();
                let op = MakeRot(3.141592653589793);
                ApplyOp(op, q);
                return MResetZ(q);
            }
        }
    "#});
}

#[test]
fn capture_admissibility_producer_nested_mutable_snapshot() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            function MakeRot(angle : Double) : Qubit => Unit is Adj + Ctl {
                Rx(angle, _)
            }
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable angle = 0.0;
                let op = MakeRot(angle + 0.0);
                set angle = 3.141592653589793;
                op(target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
fn capture_admissibility_direct_mutable_snapshot() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable angle = 0.0;
                let op = Rx(angle + 0.0, _);
                set angle = 3.141592653589793;
                op(target);
                MResetZ(target)
            }
        }
    "#});
}

#[test]
fn capture_admissibility_loop_mutable_snapshot() {
    crate::test_utils::check_semantic_equivalence(indoc::indoc! {r#"
        namespace Test {
            @EntryPoint()
            operation Main() : Result {
                use target = Qubit();
                mutable angle = 0.0;
                let op = Rx(angle + 0.0, _);
                for _ in 0..0 {
                    set angle = 3.141592653589793;
                }
                op(target);
                MResetZ(target)
            }
        }
    "#});
}
