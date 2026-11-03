// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_fir::fir::{
    ItemKind, LocalItemId, PackageId, PackageStore, PackageStoreLookup, StoreItemId,
};
use qsc_fir_transforms::run_pipeline_with_diagnostics_and_optimize;
use qsc_fir_transforms::test_utils::{assert_no_pipeline_errors, compile_to_fir};
use qsc_qhdg::{Edge, EdgeKind, ExitKind, Graph, NodeId, PortKind, RegionKind, verify, vocabulary};

use super::{
    EffectFreedom, Error, IdentityCensus, Lowered, RetireError, emitted_names,
    lower_callable_with_census, retire_into_fir,
};
use crate::circuit_qhdg::{ReadItem, qubit_operands, read_program};

/// The local item id of a callable by name, at whatever pipeline state the
/// store is in.
fn callable_named(store: &PackageStore, package: PackageId, name: &str) -> LocalItemId {
    store
        .get(package)
        .items
        .iter()
        .find_map(|(id, item)| match &item.kind {
            ItemKind::Callable(decl) if &*decl.name.name == name => Some(id),
            ItemKind::Callable(_) | ItemKind::Ty(..) => None,
        })
        .unwrap_or_else(|| panic!("the pipeline should still hold a callable named '{name}'"))
}

/// Compiles `source`, runs the transform pipeline, and lowers one callable from
/// inside the pipeline's own optimization point.
///
/// Lowering from inside the point rather than from a store the pipeline has
/// finished with is the whole reason the point exists: it is the FIR the stage
/// will actually see, at the invariant level it advertises.
fn lower_at_the_stage_point(
    source: &str,
    callable: &str,
) -> (Result<Lowered, Error>, IdentityCensus) {
    let (mut store, package) = compile_to_fir(source);
    let mut lowered = None;
    let result =
        run_pipeline_with_diagnostics_and_optimize(&mut store, package, |store, package| {
            let item = callable_named(store, package, callable);
            lowered = Some(lower_callable_with_census(
                store,
                StoreItemId { package, item },
            ));
        });
    assert_no_pipeline_errors("fir_qhdg::lower_at_the_stage_point", &result.errors);
    lowered.expect("the optimization point should have been reached")
}

fn graph_of(source: &str, callable: &str) -> Lowered {
    let (lowered, _) = lower_at_the_stage_point(source, callable);
    lowered.expect("the callable should lower")
}

fn refusal_of(source: &str, callable: &str) -> (Error, IdentityCensus) {
    let (lowered, census) = lower_at_the_stage_point(source, callable);
    match lowered {
        Ok(_) => panic!("the callable should have been refused"),
        Err(error) => (error, census),
    }
}

/// Every occurrence of one operation name in a lowered graph.
fn count_of(lowered: &Lowered, operation: &str) -> usize {
    lowered
        .graph
        .nodes()
        .filter(|node| node.operation.0 == operation)
        .count()
}

/// A trivial entry point, so the pipeline has one to root its walk at.
const ENTRY: &str = r"
    @EntryPoint()
    operation Main() : Unit {}
";

#[test]
fn the_fir_producer_adds_nothing_to_the_shared_vocabulary() {
    // The acceptance criterion the shared vocabulary was defined for. Two
    // producers could have agreed by accident; a third that also needs no name
    // of its own is evidence that the names are a fact about the graph rather
    // than one producer's convention.
    vocabulary::accept_producer(emitted_names())
        .expect("every name this producer emits is in the shared vocabulary");
}

#[test]
fn a_straight_line_body_lowers_to_a_graph_that_verifies() {
    let lowered = graph_of(
        &format!(
            r"
            operation Twice(q : Qubit) : Unit {{
                H(q);
                H(q);
            }}
            {ENTRY}"
        ),
        "Twice",
    );

    assert!(
        verify(&lowered.graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&lowered.graph)
    );
    assert_eq!(count_of(&lowered, "quantum.h"), 2);
    assert_eq!(count_of(&lowered, "quantum.allocate"), 1);
    assert_eq!(count_of(&lowered, "quantum.release"), 1);
}

#[test]
fn a_body_is_lowered_with_its_own_parameter_still_a_parameter() {
    // The case neither other producer meets, and the one the FIR stage exists
    // for: the body is optimized before anything inlines it, so the wire its
    // operations name is the parameter rather than a resolved index.
    let (_, census) = lower_at_the_stage_point(
        &format!(
            r"
            operation Twice(q : Qubit) : Unit {{
                H(q);
                H(q);
            }}
            {ENTRY}"
        ),
        "Twice",
    );

    assert_eq!(
        census,
        IdentityCensus {
            resource: 0,
            binding: 2,
            opaque: 0
        }
    );
}

#[test]
fn a_reused_qubit_becomes_a_chain_rather_than_a_repeated_name() {
    let lowered = graph_of(
        &format!(
            r"
            operation Twice(q : Qubit) : Unit {{
                H(q);
                H(q);
            }}
            {ENTRY}"
        ),
        "Twice",
    );

    for port in lowered.graph.ports() {
        if port.kind != PortKind::Capability {
            continue;
        }
        let uses = lowered
            .graph
            .edges()
            .iter()
            .filter(|edge| edge.source == port.id && edge.kind == EdgeKind::AffineCapability)
            .count();
        assert!(
            uses <= 1,
            "a capability was consumed more than once: {port:?}"
        );
    }
}

#[test]
fn a_call_into_a_body_is_walked_at_the_call_site() {
    // `H` is a Q# operation whose body calls the intrinsic, so a producer that
    // stopped at the first call would emit nothing at all. Walking at the call
    // site is also what substitutes the caller's wire for the callee's
    // parameter.
    let lowered = graph_of(
        r"
        operation Twice(q : Qubit) : Unit {
            H(q);
            H(q);
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Twice(q);
            Twice(q);
        }
        ",
        "Main",
    );

    assert_eq!(
        count_of(&lowered, "quantum.h"),
        4,
        "two calls to a body holding two gates"
    );
}

#[test]
fn two_allocations_are_distinct_so_a_two_qubit_gate_lowers() {
    // The allocator returns a qubit that is not currently allocated, so this is
    // a fact about what the runtime does rather than an inference from how the
    // program was written.
    let lowered = graph_of(
        r"
        @EntryPoint()
        operation Main() : Unit {
            use a = Qubit();
            use b = Qubit();
            CNOT(a, b);
        }
        ",
        "Main",
    );

    assert!(verify(&lowered.graph).is_empty());
    assert_eq!(count_of(&lowered, "quantum.cx"), 1);
    assert_eq!(count_of(&lowered, "quantum.allocate"), 2);
}

#[test]
fn two_parameters_cannot_be_told_apart_so_a_two_qubit_gate_is_declined() {
    // The ceiling this producer is built against, stated as a test rather than
    // as a comment. Two bindings that differ are `Unknown`, not `Distinct`, and
    // threading them as two chains would let a rewrite reorder across a wire it
    // could not see.
    let (error, census) = refusal_of(
        &format!(
            r"
            operation Pair(a : Qubit, b : Qubit) : Unit {{
                CNOT(a, b);
            }}
            {ENTRY}"
        ),
        "Pair",
    );

    assert_eq!(error, Error::AmbiguousIdentity { count: 2 });
    assert_eq!(census.binding, 2, "the decline is attributable to identity");
    assert_eq!(census.resource, 0);
}

#[test]
fn a_mutable_qubit_binding_is_opaque_and_the_decline_is_visible() {
    // Q# immutability is the whole discharge rule, so the case it does not
    // cover is the case that has to be observable. A `set` later in the body
    // would give this name a second wire.
    let (error, census) = refusal_of(
        r"
        @EntryPoint()
        operation Main() : Unit {
            use a = Qubit();
            mutable q = a;
            H(q);
        }
        ",
        "Main",
    );

    assert_eq!(error, Error::UnresolvedQubit);
    assert_eq!(census.opaque, 1);
}

#[test]
fn an_indexed_array_element_is_opaque_and_the_decline_is_visible() {
    // The other side of the same rule. No alias analysis exists at this level
    // anywhere in this tree, so two index expressions cannot be told apart and
    // neither can be told apart from a name.
    let (error, census) = refusal_of(
        &format!(
            r"
            operation OnFirst(qs : Qubit[]) : Unit {{
                H(qs[0]);
            }}
            {ENTRY}"
        ),
        "OnFirst",
    );

    assert_eq!(error, Error::UnresolvedQubit);
    assert_eq!(census.opaque, 1);
}

#[test]
fn a_functor_wrapped_call_resolves_to_the_gate_it_wraps() {
    // A resolved callable does not carry the functor its call site applied, so
    // a producer that resolved and stopped would emit `quantum.t` for
    // `Adjoint T` and be wrong by a quarter turn.
    let lowered = graph_of(
        &format!(
            r"
            operation AdjointT(q : Qubit) : Unit {{
                Adjoint T(q);
            }}
            {ENTRY}"
        ),
        "AdjointT",
    );

    assert_eq!(count_of(&lowered, "quantum.t_adj"), 1);
    assert_eq!(count_of(&lowered, "quantum.t"), 0);
}

#[test]
fn a_doubled_adjoint_is_the_callable_itself() {
    let lowered = graph_of(
        &format!(
            r"
            operation TwiceAdjoint(q : Qubit) : Unit {{
                Adjoint Adjoint T(q);
            }}
            {ENTRY}"
        ),
        "TwiceAdjoint",
    );

    assert_eq!(count_of(&lowered, "quantum.t"), 1);
    assert_eq!(count_of(&lowered, "quantum.t_adj"), 0);
}

#[test]
fn a_controlled_call_is_refused_by_name() {
    let (error, _) = refusal_of(
        &format!(
            r"
            operation ControlledX(ctl : Qubit, q : Qubit) : Unit {{
                Controlled X([ctl], q);
            }}
            {ENTRY}"
        ),
        "ControlledX",
    );

    assert_eq!(
        error,
        Error::ControlledCall {
            name: "X".to_string()
        }
    );
}

#[test]
fn an_intrinsic_the_vocabulary_has_no_term_for_refuses_by_name() {
    // The rule the shared vocabulary rests on. An unmapped operation is
    // refused where it is met, rather than defaulting to something that looks
    // like it.
    let (error, _) = refusal_of(
        &format!(
            r"
            operation Custom(q : Qubit) : Unit {{
                body intrinsic;
            }}

            operation UsesCustom(q : Qubit) : Unit {{
                Custom(q);
            }}
            {ENTRY}"
        ),
        "UsesCustom",
    );

    assert_eq!(
        error,
        Error::UnsupportedCallable {
            name: "Custom".to_string()
        }
    );
}

#[test]
fn a_measurement_is_refused_by_name_rather_than_given_an_invented_register() {
    // At FIR a measurement's result is a value bound to a local, not a write
    // into a declared register, and the vocabulary's store name carries a
    // register index. Minting one here would be this producer inventing a fact
    // the source does not hold.
    let (error, _) = refusal_of(
        &format!(
            r"
            operation Measured(q : Qubit) : Result {{
                M(q)
            }}
            {ENTRY}"
        ),
        "Measured",
    );

    assert_eq!(
        error,
        Error::ValuedCall {
            name: "M".to_string()
        }
    );
}

#[test]
fn an_expression_form_this_producer_does_not_model_refuses_by_name() {
    // Skipping an unrecognized form would drop whatever it contains from the
    // graph without saying so, which is the silent-loss failure this producer
    // is written to avoid.
    let (error, _) = refusal_of(
        &format!(
            r"
            operation Conditional(q : Qubit, flag : Bool) : Unit {{
                if flag {{
                    H(q);
                }}
            }}
            {ENTRY}"
        ),
        "Conditional",
    );

    assert_eq!(
        error,
        Error::UnsupportedExpression {
            kind: "conditional"
        }
    );
}

/// Whether the judgement clears one callable's whole body.
///
/// The body rather than a hand-picked expression, because what the producer
/// asks the judgement about is always something a body holds, and picking an
/// expression out of a store by hand would be asking a different question.
fn body_is_effect_free(source: &str, callable: &str) -> bool {
    let (mut store, package) = compile_to_fir(source);
    let mut cleared = None;
    let result =
        run_pipeline_with_diagnostics_and_optimize(&mut store, package, |store, package| {
            let item = callable_named(store, package, callable);
            let Some(qsc_fir::fir::Global::Callable(decl)) =
                store.get_global(StoreItemId { package, item })
            else {
                panic!("'{callable}' should still be a callable");
            };
            let qsc_fir::fir::CallableImpl::Spec(spec) = &decl.implementation else {
                panic!("'{callable}' should have a body to walk");
            };
            let block = spec.body.block;
            cleared = Some(EffectFreedom::new(store).block(package, block));
        });
    assert_no_pipeline_errors("fir_qhdg::body_is_effect_free", &result.errors);
    cleared.expect("the optimization point should have been reached")
}

#[test]
fn a_body_reaching_a_gate_through_one_level_of_indirection_is_not_cleared() {
    // The case a shallow judgement would get wrong. `Outer` holds no gate of
    // its own and calls something classical-looking; the gate is one call
    // further down. Clearing this would let the producer walk past a `H` and
    // then leave it in the FIR.
    assert!(
        !body_is_effect_free(
            &format!(
                r"
                operation Inner(q : Qubit) : Unit {{
                    H(q);
                }}
                operation Outer(q : Qubit) : Unit {{
                    Inner(q);
                }}
                {ENTRY}"
            ),
            "Outer"
        ),
        "a call chain reaching a gate is not effect-free"
    );
}

#[test]
fn a_body_that_allocates_a_qubit_is_never_cleared() {
    // The boundary that keeps the standing non-goal intact. Allocation is a
    // quantum effect, so `use q = Qubit()` is not classical content however
    // classical the rest of the body looks, and the array form that leads into
    // alias analysis is refused for the same reason.
    assert!(
        !body_is_effect_free(
            &format!(
                r"
                operation Allocates() : Unit {{
                    use q = Qubit();
                }}
                {ENTRY}"
            ),
            "Allocates"
        ),
        "an allocation is a quantum effect"
    );
    assert!(
        !body_is_effect_free(
            &format!(
                r"
                operation AllocatesMany(n : Int) : Unit {{
                    use qs = Qubit[n];
                }}
                {ENTRY}"
            ),
            "AllocatesMany"
        ),
        "and so is an array allocation"
    );
}

#[test]
fn a_recursive_classical_call_terminates_rather_than_looping() {
    // The judgement walks callee bodies, so a body that reaches itself would
    // walk forever without the guard. Reaching this assertion at all is half
    // the check; the answer being "cannot prove" is the other half.
    assert!(
        !body_is_effect_free(
            &format!(
                r"
                operation SelfCall(q : Qubit) : Unit {{
                    SelfCall(q);
                }}
                {ENTRY}"
            ),
            "SelfCall"
        ),
        "a callable reached from inside its own body cannot be proved effect-free"
    );
}

#[test]
fn an_intrinsic_function_is_cleared_and_an_intrinsic_operation_is_not() {
    // An intrinsic has no body to walk, so it clears only when it is named as
    // classical. `Length` is; `M` is a quantum primitive whatever its signature
    // suggests, and no intrinsic operation clears.
    assert!(
        body_is_effect_free(
            &format!(
                r"
                operation UsesLength(values : Int[]) : Unit {{
                    let count = Length(values);
                }}
                {ENTRY}"
            ),
            "UsesLength"
        ),
        "an intrinsic function performs no quantum operation"
    );
    assert!(
        !body_is_effect_free(
            &format!(
                r"
                operation Measures(q : Qubit) : Unit {{
                    let r = M(q);
                }}
                {ENTRY}"
            ),
            "Measures"
        ),
        "an intrinsic operation is not cleared, whatever it returns"
    );
}

#[test]
fn an_intrinsic_function_that_observes_quantum_state_is_not_cleared() {
    // Being a Q# function proves the callee applies no gate and allocates no
    // qubit. It does not prove the callee ignores the state, and `DumpMachine`
    // is a declared function that reads it. Clearing one would let a rewrite
    // cancel a pair straddling the point the program is observed at, which is
    // the same reordering hazard `Fail` and `Return` are refused for.
    assert!(
        !body_is_effect_free(
            &format!(
                r"
                operation Observes(q : Qubit) : Unit {{
                    Std.Diagnostics.DumpMachine();
                }}
                {ENTRY}"
            ),
            "Observes"
        ),
        "an intrinsic function that reads quantum state is not cleared"
    );
    assert!(
        !body_is_effect_free(
            &format!(
                r"
                operation Reports(q : Qubit) : Unit {{
                    Std.Diagnostics.DumpRegister([q]);
                }}
                {ENTRY}"
            ),
            "Reports"
        ),
        "and neither is the register form, which observes the same state through an argument"
    );
}

#[test]
fn a_classical_call_reaching_an_observation_is_refused_rather_than_cleared() {
    // The composed shape the clearance opened. `Trace` returns a value, so the
    // call reaches the `ValuedCall` rescue, and the rescue asks the judgement
    // about the whole call. If the judgement cleared it the two gates would be
    // left adjacent, `InverseCancellation` would remove them, and the
    // observation between them would run against a state the source never
    // reaches. The refusal is what keeps that pair apart.
    let (error, _) = refusal_of(
        &format!(
            r"
            function Trace() : Int {{
                Std.Diagnostics.DumpMachine();
                0
            }}
            operation Straddles(q : Qubit) : Unit {{
                H(q);
                let _ = Trace();
                H(q);
            }}
            {ENTRY}"
        ),
        "Straddles",
    );

    assert_eq!(
        error,
        Error::ValuedCall {
            name: "Trace".to_string()
        }
    );
}

#[test]
fn a_body_reaching_a_fail_through_a_call_is_not_cleared() {
    // Divergence one call down, so the answer has to come from the walk. `Boom`
    // holds the `fail` as its whole body rather than behind an `if`, because an
    // `if` is refused on its own and would let this pass without the `Fail` arm
    // doing anything.
    assert!(
        !body_is_effect_free(
            &format!(
                r#"
                function Boom() : Unit {{
                    fail "boom";
                }}
                operation Diverges() : Unit {{
                    Boom();
                }}
                {ENTRY}"#
            ),
            "Diverges"
        ),
        "a call chain reaching a `fail` is not effect-free"
    );
}

#[test]
fn an_intrinsic_function_that_can_fail_is_not_cleared() {
    // The same property refused by an unrelated mechanism, which is why this is
    // a second test rather than a second assertion above. `Fact` fails in its Q#
    // body, but `collapse_simulatable_intrinsics` rewrites its
    // `@SimulatableIntrinsic` declaration to `CallableImpl::Intrinsic` before
    // the stage runs, so there is no body left for the walk to see. Only its
    // absence from `classical_intrinsic` refuses it, and adding it there would
    // break this test while leaving the `fail` test above passing.
    assert!(
        !body_is_effect_free(
            &format!(
                r#"
                operation Asserts(ok : Bool) : Unit {{
                    Std.Diagnostics.Fact(ok, "ok");
                }}
                {ENTRY}"#
            ),
            "Asserts"
        ),
        "an intrinsic function that can fail is not cleared"
    );
}

#[test]
fn a_classical_prelude_beside_a_gate_is_accepted() {
    // The forms an ordinary body puts around its gates: a binary operator, a
    // field access, an index, a range, a non-empty tuple, and the four
    // assignment variants. None of them contributes a quantum node.
    let lowered = graph_of(
        &format!(
            r"
            operation Prelude(q : Qubit, values : Int[]) : Unit {{
                let count = Length(values);
                let doubled = count * 2;
                let span = 0..doubled;
                let pair = (count, doubled);
                let first = values[0];
                let start = span::Start;
                mutable running = first;
                set running = start;
                set running += doubled;
                H(q);
            }}
            {ENTRY}"
        ),
        "Prelude",
    );

    assert_eq!(count_of(&lowered, "quantum.h"), 1);
    assert_eq!(
        lowered.graph.nodes().count(),
        3,
        "the classical prelude contributes no node: one gate, one declaration, \
         one release"
    );
}

#[test]
fn a_classical_form_holding_a_gate_is_refused_by_its_own_name() {
    // An otherwise accepted classical form must refuse by name when it holds
    // quantum effects the clearance judgement cannot exclude.
    let refused = |body: &str| {
        refusal_of(
            &format!(
                r"
                operation Flag(q : Qubit) : Bool {{
                    H(q);
                    return true;
                }}
                operation Holds(q : Qubit) : Unit {{
                    {body}
                }}
                {ENTRY}"
            ),
            "Holds",
        )
        .0
    };

    assert_eq!(
        refused("mutable flag = false; set flag = Flag(q);"),
        Error::UnsupportedExpression { kind: "assignment" },
        "an assignment whose value reaches a gate"
    );
    assert_eq!(
        refused("let both = Flag(q) and Flag(q);"),
        Error::UnsupportedExpression {
            kind: "binary operator"
        },
        "a binary operator whose operand reaches a gate"
    );
    assert_eq!(
        refused("let pair = (1, Flag(q));"),
        Error::UnsupportedExpression { kind: "tuple" },
        "a tuple holding a call that reaches a gate"
    );
}

#[test]
fn a_valued_call_that_reaches_a_gate_is_still_refused() {
    // The other half of the boundary the user's decision drew. `Length` clears
    // because its callee performs no quantum operation; a call that returns a
    // value and performs one does not, and the graph would have nothing to
    // receive it with either way.
    let (error, _) = refusal_of(
        &format!(
            r"
            operation Flag(q : Qubit) : Bool {{
                H(q);
                return true;
            }}
            operation Holds(q : Qubit) : Unit {{
                let flag = Flag(q);
            }}
            {ENTRY}"
        ),
        "Holds",
    );

    assert_eq!(
        error,
        Error::ValuedCall {
            name: "Flag".to_string()
        }
    );
}

#[test]
fn the_four_assignment_variants_are_admitted_together() {
    // `expr_kind_name` maps all four to one string, so admitting some and
    // declining others would produce a decline indistinguishable from an
    // unadmitted form. They are admitted by one rule rather than four, which is
    // what makes the shared name honest.
    let lowered = graph_of(
        &format!(
            r"
            struct Point {{ X : Int }}
            operation Assigns(q : Qubit, values : Int[]) : Unit {{
                mutable plain = 0;
                set plain = 1;
                set plain += 2;
                mutable point = new Point {{ X = 0 }};
                set point w/= X <- 3;
                mutable array = values;
                set array w/= 0 <- 4;
                H(q);
            }}
            {ENTRY}"
        ),
        "Assigns",
    );

    assert_eq!(count_of(&lowered, "quantum.h"), 1);
}

#[test]
fn both_desugared_for_shapes_reach_their_loop() {
    // The reason the classical-statement phase exists. Neither `for` shape
    // reaches its `while` through the prelude the desugaring puts in front of
    // it: the range shape refuses at the range literal, and the array shape
    // refuses at the call to `Length`. They refuse for different reasons, so
    // each is asserted, and reaching the loop is now demonstrated by lowering
    // it rather than by refusing at it.
    for (callable, source) in [
        (
            "OverRange",
            r"
            operation OverRange(q : Qubit) : Unit {
                for _ in 1..4 {
                    H(q);
                }
            }",
        ),
        (
            "OverArray",
            r"
            operation OverArray(q : Qubit, values : Int[]) : Unit {
                for _ in values {
                    H(q);
                }
            }",
        ),
    ] {
        let lowered = graph_of(&format!("{source}\n{ENTRY}"), callable);
        assert_eq!(
            count_of(&lowered, vocabulary::THETA),
            1,
            "'{callable}' reaches its while and lowers it into a theta"
        );
        assert_eq!(count_of(&lowered, "quantum.h"), 1);
    }
}

#[test]
fn a_decline_is_still_attributed_to_the_reason_it_happened() {
    // The census is reported alongside a refusal so a decline is attributable
    // to identity rather than inferred from an absence of benefit. Widening
    // what the producer walks must not cost that.
    let (error, census) = refusal_of(
        &format!(
            r"
            operation Opaque(q : Qubit) : Unit {{
                let count = 1;
                mutable other = q;
                H(q);
            }}
            {ENTRY}"
        ),
        "Opaque",
    );

    assert_eq!(error, Error::UnresolvedQubit);
    assert_eq!(census.opaque, 1, "the decline names identity, not a form");
}

/// The one region a theta's body sits in, with the node that owns it.
fn sole_theta_region(lowered: &Lowered) -> (RegionKind, Option<NodeId>, bool) {
    let theta = lowered
        .graph
        .nodes()
        .find(|node| node.operation.0 == vocabulary::THETA)
        .expect("the loop should have produced a theta node");
    let region = lowered
        .graph
        .regions()
        .find(|region| region.kind == RegionKind::Theta)
        .expect("the loop should have produced a theta region");
    let declares_backedge = region
        .interface
        .exits
        .iter()
        .any(|(kind, _)| *kind == ExitKind::Backedge);
    (
        region.kind,
        region.owner.filter(|owner| *owner == theta.id),
        declares_backedge,
    )
}

#[test]
fn a_while_lowers_into_a_theta_that_verifies_and_is_shaped_the_way_the_reader_expects() {
    // The buildable half of this work. A graph that verifies is not the same as
    // a graph shaped the way a reader expects, so the kind, the owner, and the
    // back edge are each asserted rather than inferred from the absence of a
    // diagnostic.
    let lowered = graph_of(
        &format!(
            r"
            operation Repeats(q : Qubit) : Unit {{
                for _ in 1..4 {{
                    H(q);
                    T(q);
                }}
            }}
            {ENTRY}"
        ),
        "Repeats",
    );

    assert!(
        verify(&lowered.graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&lowered.graph)
    );

    let (kind, owner, declares_backedge) = sole_theta_region(&lowered);
    assert_eq!(kind, RegionKind::Theta, "the body repeats in a theta");
    assert!(
        owner.is_some(),
        "and the theta node owns it, because a body no node owns is a region \
         nothing enters"
    );
    assert!(
        declares_backedge,
        "the back edge is declared on the region, not merely present as an edge"
    );
    assert_eq!(count_of(&lowered, "quantum.h"), 1);
    assert_eq!(count_of(&lowered, "quantum.t"), 1);
}

#[test]
fn a_theta_this_producer_emits_reads_back_as_the_loop_it_is() {
    // The reader's only obligation for a theta is `check_loop_interface`, which
    // is structural and capability-typed. This is a conformance check on the
    // producer's output rather than a dependency of the stage: the stage's own
    // path is produce, rewrite, retire, and never constructs a reader.
    let lowered = graph_of(
        &format!(
            r"
            operation Repeats(q : Qubit) : Unit {{
                for _ in 1..4 {{
                    H(q);
                    T(q);
                }}
            }}
            {ENTRY}"
        ),
        "Repeats",
    );

    let read = read_program(&lowered.graph).expect("a FIR-origin theta reads back");
    let [ReadItem::Loop(loop_item)] = read.items.as_slice() else {
        panic!("the whole body is one loop: {:?}", read.items);
    };
    let gates: Vec<&str> = loop_item
        .body
        .iter()
        .filter_map(|item| match item {
            ReadItem::Instruction(instruction) => Some(instruction.operation.as_str()),
            _ => None,
        })
        .collect();
    assert!(
        gates.contains(&"quantum.h") && gates.contains(&"quantum.t"),
        "the body's operations are present after the read: {gates:?}"
    );
}

#[test]
fn the_producer_still_adds_no_vocabulary_name_now_that_it_emits_regions() {
    // The one-vocabulary requirement, checked against the producer's own
    // declared list rather than against what a test happened to exercise. A
    // theta and the carry that hands a wire through one are names the other
    // producers already needed.
    vocabulary::accept_producer(emitted_names())
        .expect("every name this producer emits is in the shared vocabulary");
}

#[test]
fn a_loop_whose_condition_is_not_provably_classical_is_refused_by_name() {
    // The condition never enters the graph, and it is still put to the
    // judgement, because a condition holding a call would otherwise be walked
    // past in silence.
    let (error, _) = refusal_of(
        &format!(
            r"
            operation Flag(q : Qubit) : Bool {{
                H(q);
                return false;
            }}
            operation Loops(q : Qubit) : Unit {{
                while Flag(q) {{
                    T(q);
                }}
            }}
            {ENTRY}"
        ),
        "Loops",
    );

    assert_eq!(
        error,
        Error::UnsupportedLoop {
            reason: "its condition is not provably free of quantum effect".to_string()
        }
    );
}

#[test]
fn a_qubit_allocated_inside_a_loop_body_is_refused_where_it_is_met() {
    // The release runs at the end of the callable, outside the region the
    // allocation would sit in. Refused where it is met rather than left to fail
    // verification, because "probably refuses somewhere downstream" is not the
    // posture this producer holds.
    let (error, _) = refusal_of(
        &format!(
            r"
            operation AllocatesInside() : Unit {{
                for _ in 1..4 {{
                    use q = Qubit();
                    H(q);
                }}
            }}
            {ENTRY}"
        ),
        "AllocatesInside",
    );

    assert_eq!(
        error,
        Error::UnsupportedLoop {
            reason: "it allocates a qubit the release would leave the body without".to_string()
        }
    );
}

#[test]
fn a_nested_while_produces_a_nested_theta_whose_wires_thread_the_outer_interface() {
    // Stated and asserted rather than excluded. An inner region's carried wires
    // reach it through the outer region's interface, so a nested loop is the
    // case that would show a crossing rule that only worked one level deep.
    let lowered = graph_of(
        &format!(
            r"
            operation Nested(q : Qubit) : Unit {{
                for _ in 1..4 {{
                    for _ in 1..2 {{
                        H(q);
                    }}
                    T(q);
                }}
            }}
            {ENTRY}"
        ),
        "Nested",
    );

    assert!(
        verify(&lowered.graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&lowered.graph)
    );
    assert_eq!(
        count_of(&lowered, vocabulary::THETA),
        2,
        "one theta per loop, nested"
    );
    let regions: Vec<_> = lowered
        .graph
        .regions()
        .filter(|region| region.kind == RegionKind::Theta)
        .collect();
    assert_eq!(regions.len(), 2);
    let theta_ids: Vec<_> = regions.iter().map(|region| region.id).collect();
    assert!(
        regions.iter().any(|region| region
            .parent
            .is_some_and(|parent| theta_ids.contains(&parent))),
        "the inner theta's region sits inside the outer one"
    );
}

#[test]
fn a_wire_the_loop_body_never_touches_is_handed_through_rather_than_declining_the_callable() {
    // The alternative was refusing every callable whose loop touches fewer
    // qubits than it holds. A carry costs one node and an adjacency across the
    // loop; refusing costs every other cancellation in the same body.
    let lowered = graph_of(
        &format!(
            r"
            operation TouchesOne() : Unit {{
                use a = Qubit();
                use b = Qubit();
                for _ in 1..4 {{
                    H(a);
                }}
                T(b);
            }}
            {ENTRY}"
        ),
        "TouchesOne",
    );

    assert!(
        verify(&lowered.graph).is_empty(),
        "expected a clean graph: {:?}",
        verify(&lowered.graph)
    );
    assert_eq!(
        count_of(&lowered, vocabulary::CARRY),
        1,
        "the untouched wire is handed through the body once"
    );
}

#[test]
fn a_node_names_the_statement_it_was_lowered_from() {
    // The address a FIR edit uses. The execution graph is regenerated from FIR
    // rather than edited, so a change has to name a statement rather than an
    // ordinal.
    let lowered = graph_of(
        &format!(
            r"
            operation Twice(q : Qubit) : Unit {{
                H(q);
                H(q);
            }}
            {ENTRY}"
        ),
        "Twice",
    );

    let mut indices: Vec<usize> = lowered.sites.values().map(|site| site.index).collect();
    indices.sort_unstable();
    assert_eq!(
        indices,
        vec![0, 1],
        "each gate names the statement that reached it, and the declaration and \
         release name none"
    );
}

/// A body whose three gates thread two qubits the body allocated itself.
const THREADED: &str = r"
    @EntryPoint()
    operation Main() : Unit {
        use a = Qubit();
        use b = Qubit();
        H(a);
        CNOT(a, b);
        T(b);
    }
    ";

#[test]
fn a_fir_origin_graph_reads_back_without_an_origin_branch() {
    let lowered = graph_of(THREADED, "Main");
    let read =
        read_program(&lowered.graph).expect("the reader accepts the third origin as it stands");

    assert_eq!(read.qubit_count, 2);
    assert_eq!(read.clbit_count, 0);
    let steps: Vec<(&str, &[u32])> = read
        .items
        .iter()
        .map(|item| match item {
            ReadItem::Instruction(instruction) => (
                instruction.operation.as_str(),
                instruction.qubits.as_slice(),
            ),
            ReadItem::Conditional(_) | ReadItem::Loop(_) => {
                panic!("this producer emits no nested region: {item:?}")
            }
        })
        .collect();
    assert_eq!(
        steps,
        vec![
            ("quantum.h", &[0u32][..]),
            ("quantum.cx", &[0, 1][..]),
            ("quantum.t", &[1][..]),
        ],
        "the reader resolves the wires the producer threaded rather than renumbering them"
    );
}

#[test]
fn the_reader_accounts_for_every_node_the_fir_producer_emitted() {
    // Reading returning `Ok` is not the claim. A region the walk never descends
    // into vanishes from the read program with no diagnostic, so what is
    // asserted is that the count read back matches the count emitted.
    let lowered = graph_of(THREADED, "Main");
    let read =
        read_program(&lowered.graph).expect("the reader accepts the third origin as it stands");

    let emitted = lowered
        .graph
        .nodes()
        .filter(|node| {
            node.operation.0 != "quantum.allocate" && node.operation.0 != "quantum.release"
        })
        .count();
    assert_eq!(emitted, 3, "three gates, and the rest is qubit bookkeeping");
    assert_eq!(read.items.len(), emitted);
}

#[test]
fn the_reader_reports_the_same_operands_the_walk_resolved() {
    // The surface a pass reaches the graph through. It is a second walk over
    // the same nodes, so it is where the two could disagree about a qubit
    // number without anything failing.
    let lowered = graph_of(THREADED, "Main");
    let operands =
        qubit_operands(&lowered.graph).expect("the reader accepts the third origin as it stands");

    let named: Vec<(&str, Vec<u32>)> = lowered
        .graph
        .nodes()
        .filter_map(|node| {
            let id = lowered.graph.stable_id(node.id)?;
            Some((node.operation.0.as_str(), operands.get(&id)?.clone()))
        })
        .filter(|(operation, _)| {
            *operation != "quantum.allocate" && *operation != "quantum.release"
        })
        .collect();
    assert_eq!(
        named,
        vec![
            ("quantum.h", vec![0]),
            ("quantum.cx", vec![0, 1]),
            ("quantum.t", vec![1]),
        ]
    );
}

/// Runs the whole pipeline over `source`, letting `optimize` see the FIR at the
/// stage point, and returns the FIR the pipeline finished with.
///
/// The pipeline is the return leg for this origin. Item dead-code elimination,
/// node collection, the execution-graph rebuild, and the `PostItemDce` and
/// `PostAll` invariant walks all run after the optimization point, and those
/// walks assert, so a run that returns at all is a run whose edit survived
/// them. `PostAll` is where rebuilt `exec_graph_range`s are checked in bounds.
fn pipeline_output(source: &str, optimize: impl FnOnce(&mut PackageStore, PackageId)) -> String {
    let (mut store, package) = compile_to_fir(source);
    let result = run_pipeline_with_diagnostics_and_optimize(&mut store, package, optimize);
    assert_no_pipeline_errors("fir_qhdg::pipeline_output", &result.errors);
    // `Debug` rather than `Display`, which prints no `exec_graph_range` at all
    // and would call a package with stale ranges identical to one without.
    format!("{:?}", store.get(package))
}

/// Lowers one callable from inside the stage point, panicking on a refusal.
fn lower_in_place(store: &PackageStore, package: PackageId, callable: &str) -> Lowered {
    let item = callable_named(store, package, callable);
    lower_callable_with_census(store, StoreItemId { package, item })
        .0
        .unwrap_or_else(|error| panic!("'{callable}' should lower: {error}"))
}

/// Retires the first node with this operation, re-threading its capability the
/// way a rewrite would.
fn retire_gate(graph: &mut Graph, operation: &str) {
    let node = graph
        .nodes()
        .find(|node| node.operation.0 == operation)
        .unwrap_or_else(|| panic!("the body holds a {operation}"))
        .id;
    let (inputs, outputs) = {
        let node = graph.nodes().find(|found| found.id == node).expect("live");
        (node.inputs.clone(), node.outputs.clone())
    };
    let capability = |graph: &Graph, ports: &[qsc_qhdg::PortId]| {
        ports
            .iter()
            .copied()
            .find(|port| {
                graph
                    .port(*port)
                    .is_some_and(|port| port.kind == PortKind::Capability)
            })
            .expect("a gate holds a capability")
    };
    let input = capability(graph, &inputs);
    let output = capability(graph, &outputs);
    let source = graph
        .edges()
        .iter()
        .find(|edge| edge.target == input)
        .expect("the capability came from somewhere")
        .source;
    let target = graph
        .edges()
        .iter()
        .find(|edge| edge.source == output)
        .expect("the capability went somewhere")
        .target;

    graph.retire_node(node).expect("the node was live");
    graph
        .connect(Edge {
            source,
            target,
            kind: EdgeKind::AffineCapability,
        })
        .expect("the chain closes over the gap");
}

/// A body called from the entry, so item dead-code elimination keeps it and a
/// retirement is visible in the FIR the pipeline finishes with.
const RETIRABLE: &str = r"
    operation Body(q : Qubit) : Unit {
        H(q);
        T(q);
        H(q);
    }
    @EntryPoint()
    operation Main() : Unit {
        use q = Qubit();
        Body(q);
    }
    ";

#[test]
fn the_identity_round_trip_is_byte_identical() {
    // Compare against a run that never entered the stage, so this covers the
    // edit path and every pipeline stage after it rather than the edit alone.
    let untouched = pipeline_output(RETIRABLE, |_, _| {});
    let round_tripped = pipeline_output(RETIRABLE, |store, package| {
        let lowered = lower_in_place(store, package, "Body");
        let removed = retire_into_fir(store, &lowered.graph, &lowered)
            .expect("an untouched graph carries back");
        assert_eq!(removed, 0, "an untouched graph retires nothing");
    });

    assert_eq!(round_tripped, untouched);
}

#[test]
fn a_retired_gate_leaves_the_fir_and_nothing_else_does() {
    let untouched = pipeline_output(RETIRABLE, |_, _| {});
    let after_retirement = pipeline_output(RETIRABLE, |store, package| {
        let mut lowered = lower_in_place(store, package, "Body");
        assert_eq!(count_of(&lowered, "quantum.h"), 2);
        retire_gate(&mut lowered.graph, "quantum.h");
        assert!(
            verify(&lowered.graph).is_empty(),
            "the retirement re-threaded the chain, so the return leg is fed a \
             graph a rewrite could actually have produced: {:?}",
            verify(&lowered.graph)
        );

        let removed =
            retire_into_fir(store, &lowered.graph, &lowered).expect("a retirement carries back");
        assert_eq!(removed, 1, "one statement, not the whole body");

        // Re-lowering the edited FIR is what says the edit reached FIR rather
        // than only the graph, and it says so in the producer's own terms.
        let again = lower_in_place(store, package, "Body");
        assert_eq!(count_of(&again, "quantum.h"), 1, "one h left");
        assert_eq!(count_of(&again, "quantum.t"), 1, "the t was not touched");
    });

    // The identity result above would look the same if the return leg copied
    // its input and never consulted the graph. This is what distinguishes the
    // two.
    assert_ne!(after_retirement, untouched);
}

/// Lowers `callable`, retires the first `operation` node without re-threading,
/// and returns what the return leg refused.
fn refused_retirement(source: &str, callable: &str, operation: &str) -> RetireError {
    let (mut store, package) = compile_to_fir(source);
    let mut refusal = None;
    let result =
        run_pipeline_with_diagnostics_and_optimize(&mut store, package, |store, package| {
            let mut lowered = lower_in_place(store, package, callable);
            let node = lowered
                .graph
                .nodes()
                .find(|node| node.operation.0 == operation)
                .unwrap_or_else(|| panic!("the body holds a {operation}"))
                .id;
            lowered.graph.retire_node(node).expect("the node was live");
            refusal = Some(
                retire_into_fir(store, &lowered.graph, &lowered)
                    .expect_err("the return leg should have refused"),
            );
        });
    assert_no_pipeline_errors("fir_qhdg::refused_retirement", &result.errors);
    refusal.expect("the optimization point should have been reached")
}

#[test]
fn a_call_bound_to_a_name_refuses_rather_than_dropping_the_binding() {
    let refusal = refused_retirement(
        &format!(
            r"
            operation Bound(q : Qubit) : Unit {{
                let done = H(q);
            }}
            {ENTRY}"
        ),
        "Bound",
        "quantum.h",
    );

    assert!(
        matches!(refusal, RetireError::BoundValue { .. }),
        "a gate returns unit, so the binding is well-typed and only its \
         position says the statement cannot vanish: {refusal:?}"
    );
}

#[test]
fn a_call_standing_as_the_block_value_refuses() {
    let refusal = refused_retirement(
        &format!(
            r"
            operation Tail(q : Qubit) : Unit {{
                H(q)
            }}
            {ENTRY}"
        ),
        "Tail",
        "quantum.h",
    );

    assert!(
        matches!(refusal, RetireError::BlockValue { .. }),
        "the block's value is the call, and unit typing would have let this \
         through: {refusal:?}"
    );
}

/// The distinct blocks the sites of one lowering name.
fn sited_blocks(lowered: &Lowered) -> std::collections::BTreeSet<qsc_fir::fir::BlockId> {
    lowered.sites.values().map(|site| site.block).collect()
}

#[test]
fn a_nested_block_of_the_lowered_callable_names_its_own_statements() {
    // The address a retirement inside a loop body needs. Under the rule this
    // replaced, every node in the body named the enclosing `while` statement,
    // so retiring one gate there was a partial retirement of that statement and
    // abandoned the whole graph.
    let lowered = graph_of(
        &format!(
            r"
            operation Repeats(q : Qubit) : Unit {{
                T(q);
                for _ in 1..4 {{
                    H(q);
                }}
            }}
            {ENTRY}"
        ),
        "Repeats",
    );

    assert!(
        sited_blocks(&lowered).len() > 1,
        "the loop body's statements name the body's block rather than the \
         statement that reached it"
    );
}

#[test]
fn a_node_built_in_a_callee_body_still_names_the_call_that_reached_it() {
    // The half of the rule that must not move. Two call sites would otherwise
    // share one site, and a removal at that site would edit a body every caller
    // shares rather than the one being optimized.
    let lowered = graph_of(
        &format!(
            r"
            operation Body(q : Qubit) : Unit {{
                H(q);
                T(q);
            }}
            operation Calls(q : Qubit) : Unit {{
                Body(q);
            }}
            {ENTRY}"
        ),
        "Calls",
    );

    assert_eq!(
        sited_blocks(&lowered).len(),
        1,
        "both gates name the one call statement that reached them"
    );
    let indices: std::collections::BTreeSet<usize> =
        lowered.sites.values().map(|site| site.index).collect();
    assert_eq!(
        indices.len(),
        1,
        "and they name the same statement, not two"
    );
}

#[test]
fn a_while_inside_a_callee_keeps_the_call_statements_site() {
    // The case a rule keyed on nesting rather than on the call frame would get
    // wrong. A `while` inside a callee is a nested block too, and giving it its
    // own site would name a block of another package, which the return leg
    // resolves against this one.
    let lowered = graph_of(
        &format!(
            r"
            operation Body(q : Qubit) : Unit {{
                for _ in 1..4 {{
                    H(q);
                }}
            }}
            operation Calls(q : Qubit) : Unit {{
                Body(q);
            }}
            {ENTRY}"
        ),
        "Calls",
    );

    assert_eq!(
        count_of(&lowered, vocabulary::THETA),
        1,
        "the callee's loop still lowers"
    );
    assert_eq!(
        sited_blocks(&lowered).len(),
        1,
        "and every node it built names the one call statement, including the \
         theta and the gates inside its body"
    );
}

/// A body whose loop holds a pair with a gate on each side of it, called from
/// the entry so item dead-code elimination keeps it.
const INTERIOR_PAIR_IN_A_LOOP: &str = r"
    operation Body(q : Qubit) : Unit {
        for _ in 1..4 {
            T(q);
            H(q);
            H(q);
            S(q);
        }
    }
    @EntryPoint()
    operation Main() : Unit {
        use q = Qubit();
        Body(q);
    }
    ";

#[test]
fn a_retirement_inside_a_loop_body_removes_exactly_the_statement_it_names() {
    // Statement attribution must identify the gate inside its loop body.
    let untouched = pipeline_output(INTERIOR_PAIR_IN_A_LOOP, |_, _| {});
    let after = pipeline_output(INTERIOR_PAIR_IN_A_LOOP, |store, package| {
        let mut lowered = lower_in_place(store, package, "Body");
        assert_eq!(count_of(&lowered, "quantum.h"), 2);
        retire_gate(&mut lowered.graph, "quantum.h");

        let removed = retire_into_fir(store, &lowered.graph, &lowered)
            .expect("a retirement inside a loop body carries back");
        assert_eq!(removed, 1, "one statement of the body, not the loop");

        let again = lower_in_place(store, package, "Body");
        assert_eq!(count_of(&again, "quantum.h"), 1, "one h left");
        assert_eq!(count_of(&again, "quantum.t"), 1, "the t was not touched");
        assert_eq!(count_of(&again, "quantum.s"), 1, "nor was the s");
    });

    assert_ne!(after, untouched);
}

#[test]
fn a_store_the_return_leg_declines_is_byte_identical_to_the_one_it_was_handed() {
    // Every refusal is decided before the first write, and this is what says so
    // rather than assuming it. The retirement below names a gate inside a
    // callee-side loop body, which is the case the attribution rule keeps
    // pointed at the call statement.
    const CALLEE_SIDE_LOOP: &str = r"
        operation Body(q : Qubit) : Unit {
            for _ in 1..4 {
                T(q);
                H(q);
                H(q);
                S(q);
            }
        }
        operation Wrapper(q : Qubit) : Unit {
            Body(q);
            T(q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            Wrapper(q);
        }
        ";

    let untouched = pipeline_output(CALLEE_SIDE_LOOP, |_, _| {});
    let declined = pipeline_output(CALLEE_SIDE_LOOP, |store, package| {
        let mut lowered = lower_in_place(store, package, "Wrapper");
        retire_gate(&mut lowered.graph, "quantum.h");

        // The retired gate is inside a loop reached through a call, so it names
        // the call statement, which still has live nodes. The return leg
        // refuses the whole graph rather than removing a call whose other gates
        // survive.
        let refusal = retire_into_fir(store, &lowered.graph, &lowered)
            .expect_err("a partial retirement of a call statement is refused");
        assert!(
            matches!(refusal, RetireError::PartiallyRetired { .. }),
            "{refusal:?}"
        );
    });

    assert_eq!(
        declined, untouched,
        "a store the return leg declined is a store it did not modify"
    );
}

#[test]
fn retiring_one_gate_of_a_called_body_refuses_rather_than_taking_the_rest() {
    // Every node of a walked body is attributed to the call that reached it, so
    // this asks for one gate of `Inner` to go and the other to stay. Removing
    // the call statement would take both and keeping it would take neither.
    let refusal = refused_retirement(
        &format!(
            r"
            operation Inner(q : Qubit) : Unit {{
                H(q);
                X(q);
            }}
            operation Outer(q : Qubit) : Unit {{
                Inner(q);
            }}
            {ENTRY}"
        ),
        "Outer",
        "quantum.h",
    );

    assert!(
        matches!(refusal, RetireError::PartiallyRetired { .. }),
        "{refusal:?}"
    );
}

#[test]
fn retiring_a_node_no_statement_produced_refuses_rather_than_doing_nothing() {
    // A parameter's declaration and the release that closes its chain are built
    // outside any statement. Nothing addresses them, so a retirement there has
    // no edit to become, and reporting that is the alternative to silently
    // performing its opposite.
    let refusal = refused_retirement(
        &format!(
            r"
            operation Twice(q : Qubit) : Unit {{
                H(q);
                H(q);
            }}
            {ENTRY}"
        ),
        "Twice",
        "quantum.release",
    );

    assert_eq!(refusal, RetireError::Unaddressed);
}

#[test]
fn a_node_the_lowering_did_not_produce_is_refused_rather_than_ignored() {
    let (mut store, package) = compile_to_fir(RETIRABLE);
    let mut refusal = None;
    let result =
        run_pipeline_with_diagnostics_and_optimize(&mut store, package, |store, package| {
            let mut lowered = lower_in_place(store, package, "Body");
            let region = lowered
                .graph
                .regions()
                .find(|region| region.kind == RegionKind::ModuleOmega)
                .expect("the graph has a module region")
                .id;
            lowered
                .graph
                .add_node(region, "quantum.x", Vec::new())
                .expect("a node can be added");
            refusal = retire_into_fir(store, &lowered.graph, &lowered).err();
        });
    assert_no_pipeline_errors("fir_qhdg::added_node", &result.errors);

    // The return leg only removes. A node a pass introduced has no statement to
    // become, and carrying on would drop it silently.
    assert_eq!(refusal, Some(RetireError::Introduced));
}
