// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Many tests pair a primary assertion with a `check_rewrite` before/after
// snapshot, so the generated Q# pushes function bodies past the line limit.
#![allow(clippy::too_many_lines)]

use crate::package_assigners::PackageAssigners;

use super::*;
use expect_test::expect;

// A partial application whose captured argument is computed by an effectful
// call. The binding cannot be deleted, because `GetAngle` measures its qubit,
// but its callable value is consumed by the rewrite. Cleanup must drop that
// dead value instead of leaving a closure that is still the result of an
// arrow-typed block.
#[test]
fn retained_effectful_partial_application_binding_passes_invariants() {
    let source = r#"
        operation GetAngle(q : Qubit) : Double {
            X(q);
            0.0
        }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = Rx(GetAngle(q), _);
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
}

// An effectful producer whose value is a consumed closure cannot be deleted.
// `MakeOp` cannot be deleted because `X(q)` is observable, so consuming the
// closure it returns would leave the producer returning a stand-in to code the
// rewrite never redirected. Declining the specialization keeps the call site's
// dynamic dispatch and reports `DynamicCallable` instead of emitting a call
// that would fail at run time.
#[test]
fn effectful_producer_returning_consumed_closure_declines_to_dynamic() {
    let source = r#"
        operation MakeOp(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = MakeOp(q);
            ApplyOp(op, q);
        }
        "#;
    // An actionable diagnostic, not an internal assert. The diagnostic must
    // survive the fixpoint loop's per-iteration `DynamicCallable` retain; a
    // decline raised on one iteration and not re-derived on the last would be
    // dropped silently.
    check_errors(
        source,
        &expect!["callable argument could not be resolved statically"],
    );
    check_pipeline(source);
}

#[test]
fn mutable_effectful_producer_residue_remains_authorized() {
    use qsc_fir::fir::StoreItemId;

    let loop_carried_source = r#"
        operation MakeOp(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Initial(q : Qubit) : Unit {
            H(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = Initial;
            for _ in 0..2 {
                op = MakeOp(q);
            }
            ApplyOp(op, q);
        }
        "#;
    let (mut loop_store, loop_package_id) = compile_to_monomorphized_fir(loop_carried_source);
    let loop_producer = loop_store
        .get(loop_package_id)
        .items
        .iter()
        .find_map(|(item_id, item)| {
            matches!(&item.kind, ItemKind::Callable(decl) if decl.name.name.as_ref() == "MakeOp")
                .then_some(StoreItemId::from((loop_package_id, item_id)))
        })
        .expect("loop-carried MakeOp should exist");
    let mut loop_assigners = PackageAssigners::new(&loop_store, loop_package_id);
    let loop_outcome = defunctionalize(&mut loop_store, loop_package_id, &mut loop_assigners);
    assert!(
        loop_outcome
            .diagnostics
            .iter()
            .any(super::super::Error::is_deferrable),
        "the loop-carried dynamic call should produce a deferrable diagnostic"
    );
    assert!(
        loop_outcome.residue_items.contains(&loop_producer),
        "a producer that reaches the dynamic call through the loop must remain authorized"
    );
    check_pipeline(loop_carried_source);

    let killed_source = r#"
        operation MakeOp(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Replacement(q : Qubit) : Unit {
            H(q);
        }
        operation LoopValue(q : Qubit) : Unit {
            X(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = MakeOp(q);
            op = Replacement;
            for _ in 0..2 {
                op = LoopValue;
            }
            ApplyOp(op, q);
        }
        "#;
    let (mut killed_store, killed_package_id) = compile_to_monomorphized_fir(killed_source);
    let killed_producer = killed_store
        .get(killed_package_id)
        .items
        .iter()
        .find_map(|(item_id, item)| {
            matches!(&item.kind, ItemKind::Callable(decl) if decl.name.name.as_ref() == "MakeOp")
                .then_some(StoreItemId::from((killed_package_id, item_id)))
        })
        .expect("killed MakeOp should exist");
    let mut killed_assigners = PackageAssigners::new(&killed_store, killed_package_id);
    let killed_outcome =
        defunctionalize(&mut killed_store, killed_package_id, &mut killed_assigners);
    assert!(
        killed_outcome
            .diagnostics
            .iter()
            .any(super::super::Error::is_deferrable),
        "the later loop should still produce a deferrable diagnostic"
    );
    assert!(
        !killed_outcome.residue_items.contains(&killed_producer),
        "a producer killed before the dynamic call must not remain authorized"
    );
}

#[test]
fn producer_origin_alias_survives_source_reassignment() {
    use qsc_fir::fir::StoreItemId;

    let source = r#"
        operation MakeOriginal(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation Replacement(q : Qubit) : Unit {
            H(q);
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable source = MakeOriginal(q);
            let alias = source;
            source = Replacement;
            ApplyOp(alias, q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let original_producer = store
        .get(package_id)
        .items
        .iter()
        .find_map(|(item_id, item)| {
            matches!(
                &item.kind,
                ItemKind::Callable(decl) if decl.name.name.as_ref() == "MakeOriginal"
            )
            .then_some(StoreItemId::from((package_id, item_id)))
        })
        .expect("MakeOriginal should exist");
    let mut assigners = PackageAssigners::new(&store, package_id);
    let outcome = defunctionalize(&mut store, package_id, &mut assigners);
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(super::super::Error::is_deferrable),
        "the aliased effectful producer should produce a deferrable diagnostic"
    );
    assert!(
        outcome.residue_items.contains(&original_producer),
        "the alias must retain the original producer after its source is reassigned"
    );

    let projection_source = r#"
        newtype Pair = (Selected : Qubit => Unit, Sibling : Qubit => Unit);
        operation MakeSelected(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        function MakeSibling() : Qubit => Unit {
            H
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let pair = Pair(MakeSelected(q), MakeSibling());
            let alias = pair::Selected;
            ApplyOp(alias, q);
        }
        "#;
    let (mut projection_store, projection_package_id) =
        compile_to_monomorphized_fir(projection_source);
    let (selected_producer, sibling_producer) = {
        let package = projection_store.get(projection_package_id);
        let find = |name: &str| {
            package
                .items
                .iter()
                .find_map(|(item_id, item)| {
                    matches!(
                        &item.kind,
                        ItemKind::Callable(decl) if decl.name.name.as_ref() == name
                    )
                    .then_some(StoreItemId::from((projection_package_id, item_id)))
                })
                .unwrap_or_else(|| panic!("{name} should exist"))
        };
        (find("MakeSelected"), find("MakeSibling"))
    };
    let projection_analysis =
        super::run_prepass_and_analysis(&mut projection_store, projection_package_id);
    assert!(
        projection_analysis
            .deferrable_residue_items
            .contains(&selected_producer),
        "a projected alias must retain the selected producer"
    );
    assert!(
        !projection_analysis
            .deferrable_residue_items
            .contains(&sibling_producer),
        "a projected alias must not authorize an unselected sibling producer"
    );

    let indexed_source = r#"
        operation MakeSelectedA(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeSelectedB(q : Qubit) : Qubit => Unit {
            Y(q);
            Ry(0.0, _)
        }
        function MakeSiblingA() : Qubit => Unit {
            H
        }
        function MakeSiblingB() : Qubit => Unit {
            X
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let pairs = [
                (MakeSelectedA(q), MakeSiblingA()),
                (MakeSelectedB(q), MakeSiblingB())
            ];
            let index = if MResetZ(q) == Zero { 0 } else { 1 };
            let (alias, _) = pairs[index];
            ApplyOp(alias, q);
        }
        "#;
    let (mut indexed_store, indexed_package_id) = compile_to_monomorphized_fir(indexed_source);
    let (selected_a, selected_b, sibling_a, sibling_b) = {
        let package = indexed_store.get(indexed_package_id);
        let find = |name: &str| {
            package
                .items
                .iter()
                .find_map(|(item_id, item)| {
                    matches!(
                        &item.kind,
                        ItemKind::Callable(decl) if decl.name.name.as_ref() == name
                    )
                    .then_some(StoreItemId::from((indexed_package_id, item_id)))
                })
                .unwrap_or_else(|| panic!("{name} should exist"))
        };
        (
            find("MakeSelectedA"),
            find("MakeSelectedB"),
            find("MakeSiblingA"),
            find("MakeSiblingB"),
        )
    };
    let indexed_analysis = super::run_prepass_and_analysis(&mut indexed_store, indexed_package_id);
    assert!(
        indexed_analysis
            .deferrable_residue_items
            .contains(&selected_a)
            && indexed_analysis
                .deferrable_residue_items
                .contains(&selected_b),
        "a dynamic index must retain every producer in the selected tuple field"
    );
    assert!(
        !indexed_analysis
            .deferrable_residue_items
            .contains(&sibling_a)
            && !indexed_analysis
                .deferrable_residue_items
                .contains(&sibling_b),
        "a dynamic index must not authorize producers from an unselected tuple field"
    );
}

#[test]
fn producer_origin_loop_alias_backedge_reaches_fixed_point() {
    use qsc_fir::fir::StoreItemId;

    for (context, dynamic_use) in [
        (
            "condition",
            "while { bridge = next; ApplyOp(head, q); count < 2 } {\n                head = bridge;\n                next = MakeOp(q);\n                count += 1;\n            }",
        ),
        (
            "body",
            "while count < 2 {\n                ApplyOp(head, q);\n                head = bridge;\n                bridge = next;\n                next = MakeOp(q);\n                count += 1;\n            }",
        ),
    ] {
        let source = format!(
            r#"
            operation MakeOp(q : Qubit) : Qubit => Unit {{
                X(q);
                Rx(0.0, _)
            }}
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {{
                op(target);
            }}
            operation Initial(q : Qubit) : Unit {{
                H(q);
            }}
            operation Main() : Unit {{
                use q = Qubit();
                mutable head = Initial;
                mutable bridge = Initial;
                mutable next = Initial;
                mutable count = 0;
                {dynamic_use}
            }}
            "#
        );
        let (mut store, package_id) = compile_to_monomorphized_fir(&source);
        let producer = store
            .get(package_id)
            .items
            .iter()
            .find_map(|(item_id, item)| {
                matches!(
                    &item.kind,
                    ItemKind::Callable(decl) if decl.name.name.as_ref() == "MakeOp"
                )
                .then_some(StoreItemId::from((package_id, item_id)))
            })
            .expect("MakeOp should exist");
        let analysis = super::run_prepass_and_analysis(&mut store, package_id);
        assert!(
            analysis.deferrable_residue_items.contains(&producer),
            "the {context} dynamic use must see the producer through the loop backedge"
        );
    }
}

#[test]
fn producer_summary_alias_excludes_auxiliary_and_terminates_returns() {
    use qsc_fir::fir::StoreItemId;

    let source = r#"
        operation MakeSelected(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeAuxiliary(q : Qubit) : Qubit => Unit {
            Y(q);
            Ry(0.0, _)
        }
        operation MakeUnused(q : Qubit) : Qubit => Unit {
            Z(q);
            Rz(0.0, _)
        }
        operation Forward(q : Qubit, choose : Bool) : Qubit => Unit {
            let auxiliary = MakeAuxiliary(q);
            mutable source = MakeSelected(q);
            let alias = source;
            source = auxiliary;
            if choose {
                return alias;
            } else {
                fail "no callable";
            }
            MakeUnused(q)
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = Forward(q, MResetZ(q) == Zero);
            ApplyOp(op, q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let (selected, auxiliary, unused) = {
        let package = store.get(package_id);
        let find = |name: &str| {
            package
                .items
                .iter()
                .find_map(|(item_id, item)| {
                    matches!(
                        &item.kind,
                        ItemKind::Callable(decl) if decl.name.name.as_ref() == name
                    )
                    .then_some(StoreItemId::from((package_id, item_id)))
                })
                .unwrap_or_else(|| panic!("{name} should exist"))
        };
        (
            find("MakeSelected"),
            find("MakeAuxiliary"),
            find("MakeUnused"),
        )
    };
    let analysis = super::run_prepass_and_analysis(&mut store, package_id);
    assert!(
        analysis.deferrable_residue_items.contains(&selected),
        "the returned alias must retain its definition-point producer"
    );
    assert!(
        !analysis.deferrable_residue_items.contains(&auxiliary),
        "an evaluated but ignored producer must not become returned lineage"
    );
    assert!(
        !analysis.deferrable_residue_items.contains(&unused),
        "Return and Fail must terminate producer lineage before unreachable fallthrough"
    );
}

#[test]
fn producer_summary_repeated_formals_preserve_argument_order() {
    use qsc_fir::fir::StoreItemId;

    let source = r#"
        operation MakeFirst(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeSecond(q : Qubit) : Qubit => Unit {
            Y(q);
            Ry(0.0, _)
        }
        operation MakeBeforeMutation(q : Qubit) : Qubit => Unit {
            Z(q);
            Rz(0.0, _)
        }
        operation MakeAfterMutation(q : Qubit) : Qubit => Unit {
            H(q);
            Rx(0.25, _)
        }
        function MakeIgnored() : Qubit => Unit {
            H
        }
        operation Forward(selected : Qubit => Unit, ignored : Qubit => Unit) : Qubit => Unit {
            selected
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let first = Forward(
                Forward(MakeFirst(q), MakeIgnored()),
                MakeIgnored()
            );
            let second = Forward(MakeSecond(q), MakeIgnored());
            mutable source = MakeBeforeMutation(q);
            let ordered = Forward(
                source,
                {
                    set source = MakeAfterMutation(q);
                    H
                }
            );
            ApplyOp(if MResetZ(q) == Zero { first } else { second }, q);
            ApplyOp(ordered, q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let (first, second, before_mutation, after_mutation, ignored) = {
        let package = store.get(package_id);
        let find = |name: &str| {
            package
                .items
                .iter()
                .find_map(|(item_id, item)| {
                    matches!(
                        &item.kind,
                        ItemKind::Callable(decl) if decl.name.name.as_ref() == name
                    )
                    .then_some(StoreItemId::from((package_id, item_id)))
                })
                .unwrap_or_else(|| panic!("{name} should exist"))
        };
        (
            find("MakeFirst"),
            find("MakeSecond"),
            find("MakeBeforeMutation"),
            find("MakeAfterMutation"),
            find("MakeIgnored"),
        )
    };
    let analysis = super::run_prepass_and_analysis(&mut store, package_id);
    for (name, producer) in [
        ("first", first),
        ("second", second),
        ("before-mutation", before_mutation),
    ] {
        assert!(
            analysis.deferrable_residue_items.contains(&producer),
            "the {name} selected actual must remain in returned lineage"
        );
    }
    assert!(
        !analysis.deferrable_residue_items.contains(&after_mutation),
        "a later argument effect must not replace an earlier actual snapshot"
    );
    assert!(
        !analysis.deferrable_residue_items.contains(&ignored),
        "an unreferenced repeated formal must not become returned lineage"
    );
}

#[test]
fn producer_summary_mutual_recursion_and_multi_converge() {
    use miette::Diagnostic;
    use qsc_fir::fir::StoreItemId;

    let source = r#"
        operation MakeA(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeB(q : Qubit) : Qubit => Unit {
            Y(q);
            Ry(0.0, _)
        }
        operation MakeIgnored(q : Qubit) : Qubit => Unit {
            Z(q);
            Rz(0.0, _)
        }
        operation ForwardA(q : Qubit, recurse : Bool, choose : Bool) : Qubit => Unit {
            let ignored = MakeIgnored(q);
            let next = if choose { ForwardA } else { ForwardB };
            next(q, false, choose)
        }
        operation ForwardB(q : Qubit, recurse : Bool, choose : Bool) : Qubit => Unit {
            if recurse {
                ForwardA(q, false, choose)
            } else {
                if choose { MakeA(q) } else { MakeB(q) }
            }
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = ForwardA(
                q,
                MResetZ(q) == Zero,
                MResetZ(q) == Zero
            );
            ApplyOp(op, q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let (make_a, make_b, ignored) = {
        let package = store.get(package_id);
        let find = |name: &str| {
            package
                .items
                .iter()
                .find_map(|(item_id, item)| {
                    matches!(
                        &item.kind,
                        ItemKind::Callable(decl) if decl.name.name.as_ref() == name
                    )
                    .then_some(StoreItemId::from((package_id, item_id)))
                })
                .unwrap_or_else(|| panic!("{name} should exist"))
        };
        (find("MakeA"), find("MakeB"), find("MakeIgnored"))
    };
    let analysis = super::run_prepass_and_analysis(&mut store, package_id);
    assert!(
        analysis.deferrable_residue_items.contains(&make_a)
            && analysis.deferrable_residue_items.contains(&make_b),
        "mutually recursive static candidates must converge to both returned producers"
    );
    assert!(
        !analysis.deferrable_residue_items.contains(&ignored),
        "recursive convergence must not authorize an auxiliary producer"
    );

    let (mut pipeline_store, pipeline_package_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&pipeline_store, pipeline_package_id);
    let outcome = defunctionalize(&mut pipeline_store, pipeline_package_id, &mut assigners);
    assert!(
        outcome.diagnostics.iter().all(|error| {
            error.code().is_none_or(|code| {
                code.to_string() != "Qdk.Qsc.Defunctionalize.UnsupportedProducerLineage"
            })
        }),
        "newly discovered pending summary keys must begin complete: {:?}",
        outcome.diagnostics
    );
}

#[test]
fn producer_summary_incomplete_lineage_is_fatal() {
    use miette::Diagnostic;

    let transient_source = r#"
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation ForwardArray(ops : (Qubit => Unit)[], target : Qubit) : Unit {
            ApplyOp(ops[0], target);
        }
        operation Main() : Unit {
            use q = Qubit();
            ForwardArray([H, X], q);
        }
        "#;
    let (mut transient_analysis_store, transient_analysis_package) =
        compile_to_monomorphized_fir(transient_source);
    let transient_analysis =
        super::run_prepass_and_analysis(&mut transient_analysis_store, transient_analysis_package);
    assert!(
        transient_analysis
            .call_sites
            .iter()
            .any(|site| matches!(site.callable_arg, ConcreteCallable::Dynamic)),
        "the unspecialized array parameter must create a transient dynamic site"
    );
    let (mut transient_store, transient_package) = compile_to_monomorphized_fir(transient_source);
    let mut transient_assigners = PackageAssigners::new(&transient_store, transient_package);
    let transient_outcome = defunctionalize(
        &mut transient_store,
        transient_package,
        &mut transient_assigners,
    );
    assert!(
        transient_outcome.diagnostics.iter().all(|error| {
            error.code().is_none_or(|code| {
                code.to_string() != "Qdk.Qsc.Defunctionalize.UnsupportedProducerLineage"
            })
        }),
        "transient incomplete evidence must be replaced after specialization: {:?}",
        transient_outcome.diagnostics
    );

    let terminal_source = r#"
        operation MakeComplete(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeCandidates(q : Qubit) : (Qubit => Unit)[] {
            Y(q);
            [H, X]
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let complete = MakeComplete(q);
            ApplyOp(complete, q);
            let candidates = MakeCandidates(q);
            let hof_index = if MResetZ(q) == Zero { 0 } else { 1 };
            ApplyOp(candidates[hof_index], q);
            let direct_index = if MResetZ(q) == Zero { 0 } else { 1 };
            candidates[direct_index](q);
        }
        "#;
    let (mut terminal_store, terminal_package) = compile_to_monomorphized_fir(terminal_source);
    let mut terminal_assigners = PackageAssigners::new(&terminal_store, terminal_package);
    let terminal_outcome = defunctionalize(
        &mut terminal_store,
        terminal_package,
        &mut terminal_assigners,
    );
    let unsupported: Vec<_> = terminal_outcome
        .diagnostics
        .iter()
        .filter(|error| {
            error.code().is_some_and(|code| {
                code.to_string() == "Qdk.Qsc.Defunctionalize.UnsupportedProducerLineage"
            })
        })
        .collect();
    assert_eq!(
        unsupported.len(),
        2,
        "only the terminal HOF and direct incomplete sites must become fatal: {:?}",
        terminal_outcome.diagnostics
    );
    assert!(
        unsupported
            .iter()
            .all(|error| matches!(error, super::super::Error::UnsupportedProducerLineage(_))),
        "the promoted diagnostics must use the fatal typed variant"
    );
    let mut unsupported_spans: Vec<_> = unsupported
        .iter()
        .map(|error| {
            let span = error.package_span().span;
            &terminal_source[span.lo as usize..span.hi as usize]
        })
        .collect();
    unsupported_spans.sort_unstable();
    assert_eq!(
        unsupported_spans,
        [
            "ApplyOp(candidates[hof_index], q)",
            "candidates[direct_index](q)",
        ],
        "fatal promotion must retain exact HOF and direct call-site identity"
    );
    let dynamic_count = terminal_outcome
        .diagnostics
        .iter()
        .filter(|error| {
            error
                .code()
                .is_some_and(|code| code.to_string() == "Qdk.Qsc.Defunctionalize.DynamicCallable")
        })
        .count();
    assert_eq!(
        dynamic_count, 1,
        "the complete dynamic site must remain deferrable without duplicate diagnostics"
    );
}

#[test]
fn producer_summary_ignores_symbolically_dead_dynamic_calls() {
    for (context, body) in [
        (
            "literal false branch",
            "if false { ApplyOp(ops[index], q); }",
        ),
        (
            "literal false loop",
            "while false { ApplyOp(ops[index], q); }",
        ),
        (
            "post-return fallthrough",
            "return (); ApplyOp(ops[index], q);",
        ),
    ] {
        let source = format!(
            r#"
            operation MakeCandidates(q : Qubit) : (Qubit => Unit)[] {{
                Y(q);
                [H, X]
            }}
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {{
                op(target);
            }}
            operation Main() : Unit {{
                use q = Qubit();
                let ops = MakeCandidates(q);
                let index = if MResetZ(q) == Zero {{ 0 }} else {{ 1 }};
                {body}
            }}
            "#
        );
        let (mut store, package_id) = compile_to_monomorphized_fir(&source);
        let analysis = super::run_prepass_and_analysis(&mut store, package_id);
        assert!(
            analysis.dynamic_site_ids.is_empty() && analysis.incomplete_site_ids.is_empty(),
            "the {context} must not be recorded as a dynamic or incomplete site: dynamic={:?}, incomplete={:?}",
            analysis.dynamic_site_ids,
            analysis.incomplete_site_ids
        );
    }
}

#[test]
fn producer_summary_literal_control_flow_ignores_unreachable_producers() {
    use qsc_fir::fir::StoreItemId;

    let source = r#"
        operation MakeGood(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeAndBad(q : Qubit) : Qubit => Unit {
            Y(q);
            Ry(0.0, _)
        }
        operation MakeOrBad(q : Qubit) : Qubit => Unit {
            Z(q);
            Rz(0.0, _)
        }
        operation MakeWhileBad(q : Qubit) : Qubit => Unit {
            H(q);
            Rx(0.5, _)
        }
        operation ForwardAnd(q : Qubit) : Qubit => Unit {
            mutable source = MakeGood(q);
            let ignored = false and { set source = MakeAndBad(q); true };
            source
        }
        operation ForwardOr(q : Qubit) : Qubit => Unit {
            mutable source = MakeGood(q);
            let ignored = true or { set source = MakeOrBad(q); false };
            source
        }
        operation ForwardWhile(q : Qubit) : Qubit => Unit {
            mutable source = MakeGood(q);
            while false {
                set source = MakeWhileBad(q);
            }
            source
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(ForwardAnd(q), q);
            ApplyOp(ForwardOr(q), q);
            ApplyOp(ForwardWhile(q), q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let (good, bad) = {
        let package = store.get(package_id);
        let find = |name: &str| {
            package
                .items
                .iter()
                .find_map(|(item_id, item)| {
                    matches!(
                        &item.kind,
                        ItemKind::Callable(decl) if decl.name.name.as_ref() == name
                    )
                    .then_some(StoreItemId::from((package_id, item_id)))
                })
                .unwrap_or_else(|| panic!("{name} should exist"))
        };
        (
            find("MakeGood"),
            [find("MakeAndBad"), find("MakeOrBad"), find("MakeWhileBad")],
        )
    };
    let analysis = super::run_prepass_and_analysis(&mut store, package_id);
    assert!(
        analysis.deferrable_residue_items.contains(&good),
        "the reachable producer must remain authorized"
    );
    for producer in bad {
        assert!(
            !analysis.deferrable_residue_items.contains(&producer),
            "an unreachable literal-control-flow producer must not be authorized"
        );
    }
}

#[test]
fn producer_summary_mixed_array_leaf_remains_incomplete() {
    use miette::Diagnostic;

    let source = r#"
        struct Choices {
            Candidates : (Qubit => Unit)[],
            Selected : Qubit => Unit,
        }
        operation MakeSelected(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeChoices(q : Qubit) : Choices {
            new Choices {
                Candidates = [H, X],
                Selected = MakeSelected(q),
            }
        }
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let choices = MakeChoices(q);
            let index = if MResetZ(q) == Zero { 0 } else { 1 };
            ApplyOp(choices.Candidates[index], q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&store, package_id);
    let outcome = defunctionalize(&mut store, package_id, &mut assigners);
    assert!(
        outcome.diagnostics.iter().any(|error| {
            error.code().is_some_and(|code| {
                code.to_string() == "Qdk.Qsc.Defunctionalize.UnsupportedProducerLineage"
            })
        }),
        "the unsupported selected array leaf must fail closed: {:?}",
        outcome.diagnostics
    );
}

#[test]
fn producer_summary_array_repeat_preserves_nonzero_element_lineage() {
    use qsc_fir::fir::StoreItemId;

    for (context, size, index, expected_owner, expected_incomplete) in [
        ("static zero", "2", "0", true, false),
        ("static nonzero", "2", "1", true, false),
        ("negative last", "2", "-1", true, false),
        ("negative first", "2", "-2", true, false),
        ("negative out of bounds", "2", "-3", false, false),
        (
            "dynamic",
            "2",
            "if MResetZ(q) == Zero { 0 } else { 1 }",
            true,
            false,
        ),
        (
            "dynamic extent",
            "if MResetZ(q) == Zero { 1 } else { 2 }",
            "0",
            true,
            true,
        ),
        (
            "dynamic extent negative",
            "if MResetZ(q) == Zero { 1 } else { 2 }",
            "-1",
            true,
            true,
        ),
        ("known out of bounds", "2", "2", false, false),
        ("zero-length", "0", "0", false, false),
    ] {
        let source = format!(
            r#"
            operation MakeOp(q : Qubit) : Qubit => Unit {{
                X(q);
                Rx(0.0, _)
            }}
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {{
                op(target);
            }}
            operation Main() : Unit {{
                use q = Qubit();
                let ops = [MakeOp(q), size = {size}];
                ApplyOp(ops[{index}], q);
            }}
            "#
        );
        let (mut store, package_id) = compile_to_monomorphized_fir(&source);
        let producer = store
            .get(package_id)
            .items
            .iter()
            .find_map(|(item_id, item)| {
                matches!(
                    &item.kind,
                    ItemKind::Callable(decl) if decl.name.name.as_ref() == "MakeOp"
                )
                .then_some(StoreItemId::from((package_id, item_id)))
            })
            .expect("MakeOp should exist");
        let analysis = super::run_prepass_and_analysis(&mut store, package_id);
        assert_eq!(
            analysis.deferrable_residue_items.contains(&producer),
            expected_owner,
            "the {context} index into a repeat has the wrong producer ownership"
        );
        assert_eq!(
            !analysis.incomplete_site_ids.is_empty(),
            expected_incomplete,
            "the {context} index into a repeat has the wrong completeness"
        );
    }

    for (context, selected, expect_a, expect_b) in [
        (
            "physical negative first",
            "[MakeA(q), MakeB(q)][-2]",
            true,
            false,
        ),
        (
            "physical negative last",
            "[MakeA(q), MakeB(q)][-1]",
            false,
            true,
        ),
        (
            "physical negative out of bounds",
            "[MakeA(q), MakeB(q)][-3]",
            false,
            false,
        ),
        (
            "array of repeats",
            "[[MakeA(q), size = 2], [MakeB(q), size = 2]][1][0]",
            false,
            true,
        ),
        (
            "repeat of array",
            "[[MakeA(q), MakeB(q)], size = 2][1][1]",
            false,
            true,
        ),
        (
            "nested repeats",
            "[[MakeA(q), size = 2], size = 2][1][1]",
            true,
            false,
        ),
        (
            "joined repeat extents at shared index",
            "(if MResetZ(q) == Zero { [MakeA(q), size = 2] } else { [MakeB(q), size = 3] })[1]",
            true,
            true,
        ),
        (
            "joined repeat extents at longer-only index",
            "(if MResetZ(q) == Zero { [MakeA(q), size = 2] } else { [MakeB(q), size = 3] })[2]",
            false,
            true,
        ),
    ] {
        let source = format!(
            r#"
            operation MakeA(q : Qubit) : Qubit => Unit {{
                X(q);
                Rx(0.0, _)
            }}
            operation MakeB(q : Qubit) : Qubit => Unit {{
                Y(q);
                Ry(0.0, _)
            }}
            operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {{
                op(target);
            }}
            operation Main() : Unit {{
                use q = Qubit();
                let selected = {selected};
                ApplyOp(selected, q);
            }}
            "#
        );
        let (mut store, package_id) = compile_to_monomorphized_fir(&source);
        let (make_a, make_b) = {
            let package = store.get(package_id);
            let find = |name: &str| {
                package
                    .items
                    .iter()
                    .find_map(|(item_id, item)| {
                        matches!(
                            &item.kind,
                            ItemKind::Callable(decl) if decl.name.name.as_ref() == name
                        )
                        .then_some(StoreItemId::from((package_id, item_id)))
                    })
                    .unwrap_or_else(|| panic!("{name} should exist"))
            };
            (find("MakeA"), find("MakeB"))
        };
        let analysis = super::run_prepass_and_analysis(&mut store, package_id);
        assert_eq!(
            analysis.deferrable_residue_items.contains(&make_a),
            expect_a,
            "the {context} selected the wrong MakeA lineage"
        );
        assert_eq!(
            analysis.deferrable_residue_items.contains(&make_b),
            expect_b,
            "the {context} selected the wrong MakeB lineage"
        );
    }
}

#[test]
fn negative_index_callable_dispatch_preserves_semantics() {
    for source in [
        r#"
        operation Main() : Result {
            use q = Qubit();
            let ops = [I, X];
            ops[-2](q);
            MResetZ(q)
        }
        "#,
        r#"
        operation Main() : Result {
            use q = Qubit();
            let ops = [I, X];
            ops[-1](q);
            MResetZ(q)
        }
        "#,
        r#"
        operation Main() : Result {
            use q = Qubit();
            let ops = [X, size = 2];
            ops[-1](q);
            MResetZ(q)
        }
        "#,
        r#"
        operation Main() : Result {
            use q = Qubit();
            let ops = [I, X];
            let index = if MResetZ(q) == Zero { -2 } else { -2 };
            ops[index](q);
            MResetZ(q)
        }
        "#,
        r#"
        operation Main() : Result {
            use q = Qubit();
            let ops = [I, X];
            ops[-3](q);
            MResetZ(q)
        }
        "#,
    ] {
        crate::test_utils::check_semantic_equivalence(source);
    }
}

#[test]
fn producer_summary_independent_complete_direct_site_is_not_suppressed() {
    use miette::Diagnostic;

    let source = r#"
        operation MakeHof(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeDirect(q : Qubit) : Qubit => Unit {
            Y(q);
            Ry(0.0, _)
        }
        operation Host(op : Qubit => Unit, target : Qubit) : Unit {
            let independent = MakeDirect(target);
            independent(target);
            let subordinate = op;
            subordinate(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let hof = MakeHof(q);
            Host(hof, q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&store, package_id);
    let outcome = defunctionalize(&mut store, package_id, &mut assigners);
    let dynamic_count = outcome
        .diagnostics
        .iter()
        .filter(|error| {
            error
                .code()
                .is_some_and(|code| code.to_string() == "Qdk.Qsc.Defunctionalize.DynamicCallable")
        })
        .count();
    assert_eq!(
        dynamic_count, 2,
        "independent complete HOF and direct sites must both remain diagnosed: {:?}",
        outcome.diagnostics,
    );
}

#[test]
fn producer_summary_direct_site_causality_survives_specialized_clone() {
    use miette::Diagnostic;

    let source = r#"
        operation MakeDirect(q : Qubit) : Qubit => Unit {
            Y(q);
            Ry(0.0, _)
        }
        operation Host(op : Qubit => Unit, target : Qubit) : Unit {
            let subordinate = op;
            subordinate(target);
            let independent = MakeDirect(target);
            independent(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            Host(H, q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&store, package_id);
    let outcome = defunctionalize(&mut store, package_id, &mut assigners);
    let dynamic_count = outcome
        .diagnostics
        .iter()
        .filter(|error| {
            error
                .code()
                .is_some_and(|code| code.to_string() == "Qdk.Qsc.Defunctionalize.DynamicCallable")
        })
        .count();
    assert_eq!(
        dynamic_count, 1,
        "the independent direct site copied into a specialized clone must remain diagnosed: {:?}",
        outcome.diagnostics
    );
}

#[test]
fn producer_summary_formal_callable_results_remain_subordinate() {
    use miette::Diagnostic;

    let factory_source = r#"
        operation Build(q : Qubit, value : Unit) : Qubit => Unit {
            Rx(0.0, _)
        }
        operation MakeFactory(q : Qubit) : Unit => (Qubit => Unit) {
            X(q);
            Build(q, _)
        }
        operation Host(factory : Unit => (Qubit => Unit), target : Qubit) : Unit {
            let generated = factory(());
            generated(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let factory = MakeFactory(q);
            Host(factory, q);
        }
        "#;
    let partial_source = r#"
        operation Body(value : Int, target : Qubit) : Unit {
            if value > 0 { X(target); }
        }
        operation MakeOps(q : Qubit) : ((Int, Qubit) => Unit)[] {
            Y(q);
            [Body, Body]
        }
        operation Host(op : (Int, Qubit) => Unit, target : Qubit) : Unit {
            let partial = op(1, _);
            partial(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let ops = MakeOps(q);
            let index = if MResetZ(q) == Zero { 0 } else { 1 };
            Host(ops[index], q);
        }
        "#;
    for (context, source, expected) in [
        ("formal factory result", factory_source, (1, 0)),
        ("formal partial application", partial_source, (0, 1)),
    ] {
        let (mut store, package_id) = compile_to_monomorphized_fir(source);
        let mut assigners = PackageAssigners::new(&store, package_id);
        let outcome = defunctionalize(&mut store, package_id, &mut assigners);
        let dynamic_count = outcome
            .diagnostics
            .iter()
            .filter(|error| {
                error.code().is_some_and(|code| {
                    code.to_string() == "Qdk.Qsc.Defunctionalize.DynamicCallable"
                })
            })
            .count();
        let unsupported_count = outcome
            .diagnostics
            .iter()
            .filter(|error| {
                error.code().is_some_and(|code| {
                    code.to_string() == "Qdk.Qsc.Defunctionalize.UnsupportedProducerLineage"
                })
            })
            .count();
        let spans: Vec<_> = outcome
            .diagnostics
            .iter()
            .map(|error| {
                let span = error.package_span().span;
                &source[span.lo as usize..span.hi as usize]
            })
            .collect();
        assert_eq!(
            (dynamic_count, unsupported_count),
            expected,
            "the {context} must leave only the diagnosed HOF root: {spans:?}; {:?}",
            outcome.diagnostics
        );
    }
}

#[test]
fn producer_summary_mixed_formal_and_independent_lineage_is_not_suppressed() {
    use miette::Diagnostic;

    for (context, independent, expected) in [
        (
            "complete independent producer",
            "MakeDirect(target)",
            (2, 0),
        ),
        (
            "incomplete independent producer",
            "MakeCandidates(target)[if MResetZ(target) == Zero { 0 } else { 1 }]",
            (1, 1),
        ),
    ] {
        let source = format!(
            r#"
            operation MakeRoot(q : Qubit) : Qubit => Unit {{
                X(q);
                Rx(0.0, _)
            }}
            operation MakeDirect(q : Qubit) : Qubit => Unit {{
                Y(q);
                Ry(0.0, _)
            }}
            operation MakeCandidates(q : Qubit) : (Qubit => Unit)[] {{
                Z(q);
                [H, X]
            }}
            operation Host(op : Qubit => Unit, target : Qubit) : Unit {{
                let selected = if MResetZ(target) == Zero {{ op }} else {{ {independent} }};
                selected(target);
            }}
            operation Main() : Unit {{
                use q = Qubit();
                let root = MakeRoot(q);
                Host(root, q);
            }}
            "#
        );
        let (mut store, package_id) = compile_to_monomorphized_fir(&source);
        let mut assigners = PackageAssigners::new(&store, package_id);
        let outcome = defunctionalize(&mut store, package_id, &mut assigners);
        let dynamic_count = outcome
            .diagnostics
            .iter()
            .filter(|error| {
                error.code().is_some_and(|code| {
                    code.to_string() == "Qdk.Qsc.Defunctionalize.DynamicCallable"
                })
            })
            .count();
        let unsupported_count = outcome
            .diagnostics
            .iter()
            .filter(|error| {
                error.code().is_some_and(|code| {
                    code.to_string() == "Qdk.Qsc.Defunctionalize.UnsupportedProducerLineage"
                })
            })
            .count();
        assert_eq!(
            (dynamic_count, unsupported_count),
            expected,
            "the {context} must preserve the independent direct diagnostic: {:?}",
            outcome.diagnostics
        );
    }
}

#[test]
fn producer_summary_multi_parameter_rows_preserve_causality_and_multiplicity() {
    use miette::Diagnostic;

    let two_incomplete = r#"
        operation MakeCandidates(q : Qubit) : (Qubit => Unit)[] {
            X(q);
            [H, X]
        }
        operation PairApply(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
            first(target);
            second(target);
        }
        operation Host(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
            PairApply(first, second, target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let candidates = MakeCandidates(q);
            let first = if MResetZ(q) == Zero { 0 } else { 1 };
            let second = if MResetZ(q) == Zero { 0 } else { 1 };
            Host(candidates[first], candidates[second], q);
        }
        "#;
    let mixed_child = r#"
        operation MakeRoot(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeIndependent(q : Qubit) : Qubit => Unit {
            Y(q);
            Ry(0.0, _)
        }
        operation PairApply(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
            first(target);
            second(target);
        }
        operation Host(first : Qubit => Unit, target : Qubit) : Unit {
            let independent = MakeIndependent(target);
            PairApply(first, independent, target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let root = MakeRoot(q);
            Host(root, q);
        }
        "#;
    let mixed_root = r#"
        operation MakeComplete(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeCandidates(q : Qubit) : (Qubit => Unit)[] {
            Y(q);
            [H, X]
        }
        operation Host(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
            first(target);
            second(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let complete = MakeComplete(q);
            let candidates = MakeCandidates(q);
            let index = if MResetZ(q) == Zero { 0 } else { 1 };
            Host(complete, candidates[index], q);
        }
        "#;
    for (context, source, expected) in [
        ("two incomplete root rows", two_incomplete, (0, 2)),
        ("mixed child rows", mixed_child, (3, 0)),
        ("mixed root rows", mixed_root, (1, 1)),
    ] {
        let (mut store, package_id) = compile_to_monomorphized_fir(source);
        let mut assigners = PackageAssigners::new(&store, package_id);
        let outcome = defunctionalize(&mut store, package_id, &mut assigners);
        let dynamic_count = outcome
            .diagnostics
            .iter()
            .filter(|error| {
                error.code().is_some_and(|code| {
                    code.to_string() == "Qdk.Qsc.Defunctionalize.DynamicCallable"
                })
            })
            .count();
        let unsupported_count = outcome
            .diagnostics
            .iter()
            .filter(|error| {
                error.code().is_some_and(|code| {
                    code.to_string() == "Qdk.Qsc.Defunctionalize.UnsupportedProducerLineage"
                })
            })
            .count();
        assert_eq!(
            (dynamic_count, unsupported_count),
            expected,
            "the {context} must retain exact row multiplicity: {:?}",
            outcome.diagnostics
        );
    }
}

#[test]
fn unrelated_local_id_collision_is_not_treated_as_hof_parameter() {
    use qsc_fir::fir::StoreItemId;

    let source = r#"
        operation MakeCandidates(q : Qubit) : (Qubit => Unit)[] {
            X(q);
            [H, X]
        }
        operation Marker(
            firstDummy : Int,
            secondDummy : Int,
            thirdDummy : Int,
            fourthDummy : Int,
            forwarded : Qubit => Unit,
            target : Qubit
        ) : Unit {
            forwarded(target);
        }
        operation Plain(target : Qubit) : Unit {
            I(target);
        }
        operation Independent(target : Qubit) : Unit {
            let candidates = MakeCandidates(target);
            let index = if MResetZ(target) == Zero { 0 } else { 1 };
            mutable selected = Plain;
            while MResetZ(target) == Zero {
                set selected = candidates[index];
            }
            selected(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let candidates = MakeCandidates(q);
            let index = if MResetZ(q) == Zero { 0 } else { 1 };
            Marker(0, 0, 0, 0, candidates[index], q);
            Independent(q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let independent_item = {
        let package = store.get(package_id);
        let item = |name: &str| {
            package
                .items
                .iter()
                .find_map(|(item_id, item)| {
                    let ItemKind::Callable(declaration) = &item.kind else {
                        return None;
                    };
                    (declaration.name.name.as_ref() == name).then_some(item_id)
                })
                .unwrap_or_else(|| panic!("{name} should exist"))
        };
        item("Independent")
    };
    let producer = store
        .get(package_id)
        .items
        .iter()
        .find_map(|(item_id, item)| {
            matches!(
                &item.kind,
                ItemKind::Callable(decl) if decl.name.name.as_ref() == "MakeCandidates"
            )
            .then_some(StoreItemId::from((package_id, item_id)))
        })
        .expect("MakeCandidates should exist");
    let analysis = super::run_prepass_and_analysis(&mut store, package_id);
    let package = store.get(package_id);
    let ItemKind::Callable(independent) = &package.get_item(independent_item).kind else {
        panic!("Independent should remain callable");
    };
    let mut selected_site = None;
    let mut selected_callee = None;
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &independent.implementation,
        &mut |expression_id, expression| {
            let span = expression.span;
            if &source[span.lo as usize..span.hi as usize] != "selected(target)" {
                return;
            }
            let qsc_fir::fir::ExprKind::Call(callee, _) = expression.kind else {
                return;
            };
            let qsc_fir::fir::ExprKind::Var(qsc_fir::fir::Res::Local(local), _) =
                package.get_expr(callee).kind
            else {
                return;
            };
            selected_site = Some((package_id, expression_id));
            selected_callee = Some(local);
        },
    );
    let selected_site = selected_site.expect("Independent selected(target) should exist");
    let selected_callee = selected_callee.expect("selected(target) should have a local callee");
    assert!(
        analysis
            .callable_params
            .iter()
            .any(|parameter| parameter.param_var == selected_callee),
        "fixture must collide with a live HOF parameter LocalVarId"
    );
    assert_eq!(
        analysis.dynamic_site_owners.get(&selected_site),
        Some(&StoreItemId::from((package_id, independent_item))),
        "the colliding original occurrence must retain its package-qualified owner"
    );
    assert!(
        analysis.deferrable_residue_items.contains(&producer),
        "the unrelated direct site must retain its producer owner"
    );
}

#[test]
fn producer_summary_multi_parameter_rows_survive_specialized_clone() {
    use miette::Diagnostic;

    let source = r#"
        operation MakeFirst(q : Qubit) : Qubit => Unit {
            X(q);
            Rx(0.0, _)
        }
        operation MakeSecond(q : Qubit) : Qubit => Unit {
            Y(q);
            Ry(0.0, _)
        }
        operation Host(first : Qubit => Unit, second : Qubit => Unit, target : Qubit) : Unit {
            first(target);
            second(target);
        }
        operation Outer(seed : Qubit => Unit, target : Qubit) : Unit {
            seed(target);
            let first = MakeFirst(target);
            let second = MakeSecond(target);
            Host(first, second, target);
        }
        operation Main() : Unit {
            use q = Qubit();
            Outer(H, q);
        }
        "#;
    let (mut store, package_id) = compile_to_monomorphized_fir(source);
    let mut assigners = PackageAssigners::new(&store, package_id);
    let outcome = defunctionalize(&mut store, package_id, &mut assigners);
    let dynamic_count = outcome
        .diagnostics
        .iter()
        .filter(|error| {
            error
                .code()
                .is_some_and(|code| code.to_string() == "Qdk.Qsc.Defunctionalize.DynamicCallable")
        })
        .count();
    assert_eq!(
        dynamic_count, 2,
        "both independent producer rows in the specialized clone must remain diagnosed: {:?}",
        outcome.diagnostics
    );
}

// The decline is a property of the producer's observability, not of producers
// in general. Removing the effect from `MakeOp` makes the producing call
// deletable, so the closure is consumed and the call site specializes with no
// diagnostic. Without this pair the test above would also pass if the gate
// declined every producer-returned closure.
#[test]
fn pure_producer_returning_consumed_closure_still_specializes() {
    let source = r#"
        function MakeOp() : Qubit => Unit is Adj + Ctl {
            Rx(0.0, _)
        }
        operation ApplyOp(op : Qubit => Unit is Adj + Ctl, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let op = MakeOp();
            ApplyOp(op, q);
        }
        "#;
    check_errors(source, &expect!["(no error)"]);
    check_invariants(source);
}

#[test]
fn branch_local_capture_applied_outside_scope_declines_to_dynamic() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, target : Qubit) : Unit {
            op(target);
        }
        operation Main() : Unit {
            use q = Qubit();
            let flag = MResetZ(q) == One;
            mutable op = H;
            if flag {
                let angle = 0.5;
                op = Rx(angle, _);
            }
            ApplyOp(op, q);
        }
        "#;
    check_errors(
        source,
        &expect!["callable argument could not be resolved statically"],
    );

    let direct_source = r#"
        operation Main() : Unit {
            use q = Qubit();
            let flag = MResetZ(q) == One;
            mutable op = H;
            if flag {
                let angle = 0.5;
                set op = Rx(angle, _);
            }
            op(q);
        }
        "#;
    let (mut fir_store, fir_pkg_id) = compile_to_monomorphized_fir(direct_source);
    let result = super::run_prepass_and_analysis(&mut fir_store, fir_pkg_id);
    let package = fir_store.get(fir_pkg_id);
    let direct_sites = result
        .direct_call_sites
        .iter()
        .filter(|site| {
            let span = package.get_expr(site.call_expr_id).span;
            &direct_source[span.lo as usize..span.hi as usize] == "op(q)"
        })
        .count();
    let unresolved_sites = result
        .unresolved_direct_call_sites
        .iter()
        .filter(|site| {
            let span = package.get_expr(site.expr).span;
            &direct_source[span.lo as usize..span.hi as usize] == "op(q)"
        })
        .count();
    assert_eq!(
        direct_sites, 0,
        "an inadmissible candidate should prevent every direct-site record"
    );
    assert_eq!(
        unresolved_sites, 1,
        "the rejected direct Multi should have one unresolved route"
    );
    check_errors(
        direct_source,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn invariants_single_hof() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn invariants_closure_with_captures() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                ApplyOp_Empty_(/ * closure item = 3 captures = [angle] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let angle : Double = 1.;
                ApplyOp_Empty__closure_(q, angle);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Double) : Unit {
                _lambda_3(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn invariants_functor_composition() {
    let source = r#"
        operation ApplyAdj(op : Qubit => Unit is Adj, q : Qubit) : Unit {
            Adjoint op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyAdj(S, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyAdj(op : (Qubit => Unit), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyAdj_AdjCtl_(S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Adjoint op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyAdj(op : (Qubit => Unit), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyAdj_AdjCtl__S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyAdj_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                Adjoint op(q);
            }
            operation ApplyAdj_AdjCtl__S_(q : Qubit) : Unit {
                Adjoint S(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn guarded_reassignment_callable_resolves_with_fallthrough_to_initial() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            if true { op = X; }
            ApplyOp(op, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                }

                ApplyOp_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if true {
                    op = X;
                }

                if true {
                    ApplyOp_AdjCtl__X_(q)
                } else {
                    ApplyOp_AdjCtl__H_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn error_returned_not_panicked() {
    let (mut store, package_id) = compile_to_monomorphized_fir(
        r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            mutable op = H;
            for _ in 0..3 { set op = X; }
            ApplyOp(op, q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&store, package_id);
    let errors = defunctionalize(&mut store, package_id, &mut assigners).diagnostics;
    assert!(
        !errors.is_empty(),
        "expected errors to be returned, not a panic"
    );
}

#[test]
fn deferrable_residue_owners_exclude_unrelated_callable() {
    use qsc_fir::fir::{CallableImpl, CallableKind, ExprKind, StmtKind, StoreItemId};
    use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, Prim, Ty};

    let (mut store, package_id) = compile_to_monomorphized_fir(
        r#"
        function Identity(value : Int) : Int { value }
        operation Unrelated() : Unit {
            let decoy = 1;
        }
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            Unrelated();
            mutable op = H;
            for _ in 0..3 { set op = X; }
            ApplyOp(op, q);
        }
        "#,
    );
    let (identity_item, unrelated_item, unrelated_init) = {
        let package = store.get(package_id);
        let mut identity_item = None;
        let mut unrelated_item = None;
        let mut unrelated_init = None;
        for (item_id, item) in &package.items {
            let ItemKind::Callable(decl) = &item.kind else {
                continue;
            };
            match decl.name.name.as_ref() {
                "Identity" => identity_item = Some(item_id),
                "Unrelated" => {
                    unrelated_item = Some(item_id);
                    let CallableImpl::Spec(spec) = &decl.implementation else {
                        panic!("Unrelated should have a body");
                    };
                    let stmt_id = *package
                        .get_block(spec.body.block)
                        .stmts
                        .first()
                        .expect("Unrelated should have a local binding");
                    unrelated_init = Some(match package.get_stmt(stmt_id).kind {
                        StmtKind::Local(_, _, expr_id) => expr_id,
                        StmtKind::Expr(..) | StmtKind::Semi(..) | StmtKind::Item(_) => {
                            panic!("Unrelated should start with a local binding")
                        }
                    });
                }
                _ => {}
            }
        }
        (
            identity_item.expect("Identity should exist"),
            unrelated_item.expect("Unrelated should exist"),
            unrelated_init.expect("Unrelated should have an initializer"),
        )
    };

    let unrelated_expr = store
        .get_mut(package_id)
        .exprs
        .get_mut(unrelated_init)
        .expect("Unrelated initializer should exist");
    unrelated_expr.ty = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Function,
        input: Box::new(Ty::Prim(Prim::Int)),
        output: Box::new(Ty::Prim(Prim::Int)),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
    unrelated_expr.kind = ExprKind::Closure(Vec::new(), identity_item);

    let mut assigners = PackageAssigners::new(&store, package_id);
    let outcome = defunctionalize(&mut store, package_id, &mut assigners);
    assert!(
        outcome
            .diagnostics
            .iter()
            .any(super::super::Error::is_deferrable),
        "the organic dynamic call should produce a deferrable diagnostic"
    );
    assert!(
        !outcome
            .residue_items
            .contains(&StoreItemId::from((package_id, unrelated_item))),
        "a diagnostic in Main must not authorize unrelated callable residue"
    );
}

#[test]
fn dynamic_entry_and_fixpoint_residue_remain_authorized() {
    use qsc_fir::fir::{CallableImpl, CallableKind, ExprKind, ItemKind, StmtKind, StoreItemId};
    use qsc_fir::ty::{Arrow, FunctorSet, FunctorSetValue, Prim, Ty};

    let (mut entry_store, entry_package_id) =
        crate::test_utils::compile_to_monomorphized_fir_with_entry(
            r#"
            namespace Test {
                operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
                    op(q);
                }
            }
            "#,
            r#"{
                use q = Qubit();
                mutable op = H;
                for _ in 0..3 { set op = X; }
                Test.ApplyOp(op, q);
            }"#,
        );
    let apply_item = collect_reachable_from_entry(&entry_store, entry_package_id)
        .into_iter()
        .find(|store_id| {
            matches!(
                &entry_store
                    .get(store_id.package)
                    .get_item(store_id.item)
                    .kind,
                ItemKind::Callable(decl) if decl.name.name.starts_with("ApplyOp")
            )
        })
        .expect("reachable ApplyOp should exist");
    let mut entry_assigners = PackageAssigners::new(&entry_store, entry_package_id);
    let entry_outcome = defunctionalize(&mut entry_store, entry_package_id, &mut entry_assigners);
    assert!(
        entry_outcome.entry_has_residue,
        "a dynamic call in the entry should authorize entry residue"
    );
    assert!(
        entry_outcome.residue_items.contains(&apply_item),
        "a dynamic HOF call in the entry should authorize the reachable HOF residue"
    );

    let (mut fixpoint_store, fixpoint_package_id) = compile_to_monomorphized_fir(
        r#"
        function Identity(value : Int) : Int { value }
        function Main() : Int {
            let decoy = 1;
            0
        }
        "#,
    );
    let (identity_item, main_item, decoy_init) = {
        let package = fixpoint_store.get(fixpoint_package_id);
        let mut identity_item = None;
        let mut main_item = None;
        let mut decoy_init = None;
        for (item_id, item) in &package.items {
            let ItemKind::Callable(decl) = &item.kind else {
                continue;
            };
            match decl.name.name.as_ref() {
                "Identity" => identity_item = Some(item_id),
                "Main" => {
                    main_item = Some(item_id);
                    let CallableImpl::Spec(spec) = &decl.implementation else {
                        panic!("Main should have a body");
                    };
                    let stmt_id = *package
                        .get_block(spec.body.block)
                        .stmts
                        .first()
                        .expect("Main should have a local binding");
                    decoy_init = Some(match package.get_stmt(stmt_id).kind {
                        StmtKind::Local(_, _, expr_id) => expr_id,
                        StmtKind::Expr(..) | StmtKind::Semi(..) | StmtKind::Item(_) => {
                            panic!("Main should start with a local binding")
                        }
                    });
                }
                _ => {}
            }
        }
        (
            identity_item.expect("Identity should exist"),
            main_item.expect("Main should exist"),
            decoy_init.expect("Main should have a local initializer"),
        )
    };
    let decoy_expr = fixpoint_store
        .get_mut(fixpoint_package_id)
        .exprs
        .get_mut(decoy_init)
        .expect("decoy initializer should exist");
    decoy_expr.ty = Ty::Arrow(Box::new(Arrow {
        kind: CallableKind::Function,
        input: Box::new(Ty::Prim(Prim::Int)),
        output: Box::new(Ty::Prim(Prim::Int)),
        functors: FunctorSet::Value(FunctorSetValue::Empty),
    }));
    decoy_expr.kind = ExprKind::Closure(Vec::new(), identity_item);

    let mut fixpoint_assigners = PackageAssigners::new(&fixpoint_store, fixpoint_package_id);
    let fixpoint_outcome = defunctionalize(
        &mut fixpoint_store,
        fixpoint_package_id,
        &mut fixpoint_assigners,
    );
    assert!(
        fixpoint_outcome
            .diagnostics
            .iter()
            .any(|error| matches!(error, super::super::Error::FixpointNotReached(..))),
        "an otherwise unexplained closure should produce FixpointNotReached"
    );
    assert!(
        fixpoint_outcome
            .residue_items
            .contains(&StoreItemId::from((fixpoint_package_id, main_item))),
        "FixpointNotReached should authorize every terminal remaining owner"
    );
}

#[test]
fn error_multiple_dynamic_sites_collected() {
    let (mut store, package_id) = compile_to_monomorphized_fir(
        r#"
        operation Apply1(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Apply2(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
        operation Main() : Unit {
            use q = Qubit();
            mutable f = H;
            for _ in 0..3 { set f = X; }
            Apply1(f, q);
            mutable g = X;
            for _ in 0..3 { set g = H; }
            Apply2(g, q);
        }
        "#,
    );
    let mut assigners = PackageAssigners::new(&store, package_id);
    let errors = defunctionalize(&mut store, package_id, &mut assigners).diagnostics;
    assert_eq!(
        errors.len(),
        2,
        "expected both dynamic callable sites to be collected"
    );
    for error in &errors {
        assert!(
            matches!(error, super::super::Error::DynamicCallable(_)),
            "expected DynamicCallable error, got {error:?}"
        );
        assert!(
            !error.to_string().is_empty(),
            "each error should have a display message"
        );
    }
}

#[test]
fn nested_hof_call_chain_passes_invariants() {
    let source = r#"
        operation ApplyInner(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation ApplyOuter(op : Qubit => Unit, q : Qubit) : Unit {
            ApplyInner(op, q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOuter(H, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyInner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter(op : (Qubit => Unit), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOuter_AdjCtl_(H, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyInner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyInner(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter(op : (Qubit => Unit), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOuter_AdjCtl__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyInner_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOuter_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                ApplyInner_Empty_(op, q);
            }
            operation ApplyOuter_AdjCtl__H_(q : Qubit) : Unit {
                ApplyInner_Empty__H_(q);
            }
            operation ApplyInner_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn hof_inside_for_loop_passes_invariants() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            for _ in 0..3 {
                ApplyOp(H, q);
            }
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_87 : Unit = {
                    let _range_id_39 : Range = 0..3;
                    mutable _index_id_42 : Int = _range_id_39.Start;
                    let _step_id_47 : Int = _range_id_39.Step;
                    let _end_id_52 : Int = _range_id_39.End;
                    while ((_step_id_47 > 0) and (_index_id_42 <= _end_id_52)) or ((_step_id_47 < 0) and (_index_id_42 >= _end_id_52)) {
                        let _ : Int = _index_id_42;
                        ApplyOp_AdjCtl_(H, q);
                        _index_id_42 += _step_id_47;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_87
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_87 : Unit = {
                    let _range_id_39 : Range = 0..3;
                    mutable _index_id_42 : Int = _range_id_39.Start;
                    let _step_id_47 : Int = _range_id_39.Step;
                    let _end_id_52 : Int = _range_id_39.End;
                    while ((_step_id_47 > 0) and (_index_id_42 <= _end_id_52)) or ((_step_id_47 < 0) and (_index_id_42 >= _end_id_52)) {
                        let _ : Int = _index_id_42;
                        ApplyOp_AdjCtl__H_(q);
                        _index_id_42 += _step_id_47;
                    }

                };
                __quantum__rt__qubit_release(q);
                _generated_ident_87
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn function_callable_argument_defunctionalizes() {
    let source = r#"
        function ApplyFn(f : Int -> Int, x : Int) : Int {
            f(x)
        }
        function Double(x : Int) : Int { x * 2 }
        @EntryPoint()
        operation Main() : Unit {
            let _ = ApplyFn(Double, 5);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            function ApplyFn(f : (Int -> Int), x : Int) : Int {
                f(x)
            }
            function Double(x : Int) : Int {
                x * 2
            }
            operation Main() : Unit {
                let _ : Int = ApplyFn(Double, 5);
            }
            // entry
            Main()

            AFTER:
            function ApplyFn(f : (Int -> Int), x : Int) : Int {
                f(x)
            }
            function Double(x : Int) : Int {
                x * 2
            }
            operation Main() : Unit {
                let _ : Int = ApplyFn_Double_(5);
            }
            function ApplyFn_Double_(x : Int) : Int {
                Double(x)
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn explicit_functor_specializations_defunctionalize() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit is Adj + Ctl, q : Qubit) : Unit is Adj + Ctl {
            body ... { op(q); }
            adjoint ... { Adjoint op(q); }
            controlled (ctls, ...) { Controlled op(ctls, q); }
            controlled adjoint (ctls, ...) { Controlled Adjoint op(ctls, q); }
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(S, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(S, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__S_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    op(q);
                }
                adjoint ... {
                    Adjoint op(q);
                }
                controlled (ctls, ...) {
                    Controlled op(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint op(ctls, q);
                }
            }
            operation ApplyOp_AdjCtl__S_(q : Qubit) : Unit is Adj + Ctl {
                body ... {
                    S(q);
                }
                adjoint ... {
                    Adjoint S(q);
                }
                controlled (ctls, ...) {
                    Controlled S(ctls, q);
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint S(ctls, q);
                }
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn full_pipeline_preserves_post_all_invariants() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(X, q);
            let angle = 1.0;
            ApplyOp(q1 => Rx(angle, q1), q);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
                ApplyOp_AdjCtl_(X, q);
                let angle : Double = 1.;
                ApplyOp_Empty_(/ * closure item = 3 captures = [angle] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                ApplyOp_AdjCtl__X_(q);
                let angle : Double = 1.;
                ApplyOp_Empty__closure_(q, angle);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(angle : Double, q1 : Qubit) : Unit {
                Rx(angle, q1)
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation ApplyOp_Empty__closure_(q : Qubit, __capture_0 : Double) : Unit {
                _lambda_3(__capture_0, q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn invariant_no_closure_expressions_remain() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(q1 => H(q1), q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty_(/ * closure item = 3 captures = [] * / _lambda_3, q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_Empty__H_(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_3(q1 : Qubit, ) : Unit {
                H(q1)
            }
            operation ApplyOp_Empty_(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_Empty__H_(q : Qubit) : Unit {
                H(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn invariant_no_arrow_params_remain_in_specialized_callables() {
    let source = r#"
        operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }
        operation Main() : Unit {
            use q = Qubit();
            ApplyOp(H, q);
            ApplyOp(X, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl_(H, q);
                ApplyOp_AdjCtl_(X, q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation ApplyOp(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                ApplyOp_AdjCtl__H_(q);
                ApplyOp_AdjCtl__X_(q);
                __quantum__rt__qubit_release(q);
            }
            operation ApplyOp_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation ApplyOp_AdjCtl__H_(q : Qubit) : Unit {
                H(q);
            }
            operation ApplyOp_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn five_branch_conditional_callable_resolves_successfully() {
    let source = r#"
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {
            op(q);
        }

        operation Main() : Unit {
            use q = Qubit();
            let n = 2;
            mutable op = H;
            if n == 0 {
                op = X;
            } elif n == 1 {
                op = Y;
            } elif n == 2 {
                op = Z;
            } elif n == 3 {
                op = S;
            } else {
                op = T;
            }
            Apply(op, q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 2;
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if n == 0 {
                    op = X;
                } else if n == 1 {
                    op = Y;
                } else if n == 2 {
                    op = Z;
                } else if n == 3 {
                    op = S;
                } else {
                    op = T;
                }

                Apply_AdjCtl_(op, q);
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            // entry
            Main()

            AFTER:
            operation Apply(op : (Qubit => Unit), q : Qubit) : Unit {
                op(q);
            }
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 2;
                mutable op : (Qubit => Unit is Adj + Ctl) = H;
                if n == 0 {
                    op = X;
                } else if n == 1 {
                    op = Y;
                } else if n == 2 {
                    op = Z;
                } else if n == 3 {
                    op = S;
                } else {
                    op = T;
                }

                if n == 0 {
                    Apply_AdjCtl__X_(q)
                } else if n == 1 {
                    Apply_AdjCtl__Y_(q)
                } else if n == 2 {
                    Apply_AdjCtl__Z_(q)
                } else if n == 3 {
                    Apply_AdjCtl__S_(q)
                } else {
                    Apply_AdjCtl__T_(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation Apply_AdjCtl_(op : (Qubit => Unit is Adj + Ctl), q : Qubit) : Unit {
                op(q);
            }
            operation Apply_AdjCtl__X_(q : Qubit) : Unit {
                X(q);
            }
            operation Apply_AdjCtl__Y_(q : Qubit) : Unit {
                Y(q);
            }
            operation Apply_AdjCtl__Z_(q : Qubit) : Unit {
                Z(q);
            }
            operation Apply_AdjCtl__S_(q : Qubit) : Unit {
                S(q);
            }
            operation Apply_AdjCtl__T_(q : Qubit) : Unit {
                T(q);
            }
            // entry
            Main()
        "#]],
    );
}

/// A statically-known callable array with more than `MULTI_CAP` (1000) distinct
/// elements exceeds the per-set candidate bound during indexed-dispatch
/// resolution, so the analysis widens to `Dynamic` (top of the lattice) instead
/// of building a per-index dispatch chain. The higher-order `Apply(op, q)` call
/// over the loop element then surfaces the actionable `DynamicCallable`
/// diagnostic. Arrays at or below the cap resolve to a per-index dispatch
/// instead, so exercising the widen-to-`Dynamic` path requires more than 1000
/// distinct elements. A flat array literal is used (rather than a deeply nested
/// `if`/`elif` chain) to avoid overflowing the parser stack at this width.
#[test]
fn callable_array_exceeding_multi_cap_degrades_to_dynamic() {
    use std::fmt::Write as _;

    // One distinct callable per element; more than `MULTI_CAP` (1000) elements
    // forces indexed-dispatch resolution to widen the candidate set to
    // `Dynamic`.
    const ELEMENTS: usize = 1001;

    let mut defs = String::new();
    let mut elems = String::new();
    for i in 0..ELEMENTS {
        writeln!(defs, "        operation Op{i}(q : Qubit) : Unit {{}}").expect("write succeeds");
        if i > 0 {
            elems.push_str(", ");
        }
        write!(elems, "Op{i}").expect("write succeeds");
    }

    let source = format!(
        r#"
{defs}
        operation Apply(op : Qubit => Unit, q : Qubit) : Unit {{
            op(q);
        }}

        operation Main() : Unit {{
            use q = Qubit();
            let ops = [{elems}];
            for op in ops {{
                Apply(op, q);
            }}
        }}
        "#
    );

    check_errors(
        &source,
        &expect!["callable argument could not be resolved statically"],
    );
}

/// A direct (non-HOF) call `f(q)` whose callee `f` is forced to `Dynamic` by a
/// loop reassignment (loop reassignment is treated as unresolvable regardless
/// of the candidate count) surfaces the actionable `DynamicCallable` diagnostic
/// at the call site, rather than only the less-specific `FixpointNotReached`.
#[test]
fn direct_call_unresolvable_callable_emits_dynamic_callable_diagnostic() {
    let source = r#"
        operation Foo(q : Qubit) : Unit {}
        operation Bar(q : Qubit) : Unit {}
        operation Main() : Unit {
            use q = Qubit();
            mutable f = Foo;
            for _ in 0..2 {
                f = Bar;
            }
            f(q);
        }
        "#;
    check_errors(
        source,
        &expect!["callable argument could not be resolved statically"],
    );
    check_pipeline(source);

    check_errors(
        r#"
        operation Main() : Unit {
            use q = Qubit();
            let op = target => H(target);
            op(q);
        }
        "#,
        &expect!["(no error)"],
    );

    let (fir_store, fir_pkg_id) = compile_to_monomorphized_fir(
        r#"
        operation Main() : Unit {
            use q = Qubit();
            let op = Rx(0.5, _);
            op(q);
        }
        "#,
    );
    let package = fir_store.get(fir_pkg_id);
    let lifted_item = package
        .items
        .iter()
        .find_map(|(item_id, item)| match &item.kind {
            ItemKind::Callable(decl) if decl.name.name.starts_with(".lambda") => Some(item_id),
            _ => None,
        })
        .expect("partial application should produce a lifted lambda");
    let callable = ConcreteCallable::Global {
        item_id: ItemId {
            package: fir_pkg_id,
            item: lifted_item,
        },
        functor: FunctorApp::default(),
    };
    let recovery = defunc_analysis::resolve_direct_lifted_lambda_captures(
        package,
        &fir_store,
        &Default::default(),
        package.entry.expect("entry expression should exist"),
        &callable,
        fir_pkg_id,
    );
    assert!(
        recovery.is_none(),
        "a known lifted target with an unrecoverable occurrence must not become capture-free"
    );
}

/// Direct-path (non-HOF) analogue of
/// `callable_array_exceeding_multi_cap_degrades_to_dynamic`: a statically-known
/// callable array with more than `MULTI_CAP` (1000) distinct elements exceeds
/// the per-set candidate bound during indexed-dispatch resolution, so the
/// analysis widens to `Dynamic` instead of building a per-index dispatch chain.
/// The direct call `op(q)` over the loop element then surfaces the actionable
/// `DynamicCallable` diagnostic. A flat array literal is used (rather than a
/// deeply nested `if`/`elif` chain) to avoid overflowing the parser stack at
/// this width.
#[test]
fn direct_callable_array_exceeding_multi_cap_degrades_to_dynamic() {
    use std::fmt::Write as _;

    // One distinct callable per element; more than `MULTI_CAP` (1000) elements
    // forces indexed-dispatch resolution to widen the candidate set to
    // `Dynamic`.
    const ELEMENTS: usize = 1001;

    let mut defs = String::new();
    let mut elems = String::new();
    for i in 0..ELEMENTS {
        writeln!(defs, "        operation Op{i}(q : Qubit) : Unit {{}}").expect("write succeeds");
        if i > 0 {
            elems.push_str(", ");
        }
        write!(elems, "Op{i}").expect("write succeeds");
    }

    let source = format!(
        r#"
{defs}
        operation Main() : Unit {{
            use q = Qubit();
            let ops = [{elems}];
            for op in ops {{
                op(q);
            }}
        }}
        "#
    );

    check_errors(
        &source,
        &expect!["callable argument could not be resolved statically"],
    );
}

#[test]
fn controlled_functor_count_saturates_without_overflow() {
    let source = r#"
        operation Foo(q : Qubit) : Unit is Ctl {
            body ... { H(q); }
            controlled (cs, ...) { Controlled H(cs, q); }
        }
        operation ApplyCtl1(q : Qubit, c1 : Qubit) : Unit {
            Controlled Foo([c1], q);
        }
        operation ApplyCtl2(q : Qubit, c1 : Qubit, c2 : Qubit) : Unit {
            Controlled Foo([c1, c2], q);
        }
        operation ApplyCtl3(q : Qubit, c1 : Qubit, c2 : Qubit, c3 : Qubit) : Unit {
            Controlled Foo([c1, c2, c3], q);
        }
        @EntryPoint()
        operation Main() : Unit {
            use (q, c1, c2, c3) = (Qubit(), Qubit(), Qubit(), Qubit());
            ApplyCtl1(q, c1);
            ApplyCtl2(q, c1, c2);
            ApplyCtl3(q, c1, c2, c3);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Foo(q : Qubit) : Unit is Ctl {
                body ... {
                    H(q);
                }
                controlled (cs, ...) {
                    Controlled H(cs, q);
                }
            }
            operation ApplyCtl1(q : Qubit, c1 : Qubit) : Unit {
                Controlled Foo([c1], q);
            }
            operation ApplyCtl2(q : Qubit, c1 : Qubit, c2 : Qubit) : Unit {
                Controlled Foo([c1, c2], q);
            }
            operation ApplyCtl3(q : Qubit, c1 : Qubit, c2 : Qubit, c3 : Qubit) : Unit {
                Controlled Foo([c1, c2, c3], q);
            }
            operation Main() : Unit {
                let _generated_ident_126 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_128 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_130 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_132 : Qubit = __quantum__rt__qubit_allocate();
                let (q : Qubit, c1 : Qubit, c2 : Qubit, c3 : Qubit) = (_generated_ident_126, _generated_ident_128, _generated_ident_130, _generated_ident_132);
                ApplyCtl1(q, c1);
                ApplyCtl2(q, c1, c2);
                ApplyCtl3(q, c1, c2, c3);
                __quantum__rt__qubit_release(_generated_ident_132);
                __quantum__rt__qubit_release(_generated_ident_130);
                __quantum__rt__qubit_release(_generated_ident_128);
                __quantum__rt__qubit_release(_generated_ident_126);
            }
            // entry
            Main()

            AFTER:
            operation Foo(q : Qubit) : Unit is Ctl {
                body ... {
                    H(q);
                }
                controlled (cs, ...) {
                    Controlled H(cs, q);
                }
            }
            operation ApplyCtl1(q : Qubit, c1 : Qubit) : Unit {
                Controlled Foo([c1], q);
            }
            operation ApplyCtl2(q : Qubit, c1 : Qubit, c2 : Qubit) : Unit {
                Controlled Foo([c1, c2], q);
            }
            operation ApplyCtl3(q : Qubit, c1 : Qubit, c2 : Qubit, c3 : Qubit) : Unit {
                Controlled Foo([c1, c2, c3], q);
            }
            operation Main() : Unit {
                let _generated_ident_126 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_128 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_130 : Qubit = __quantum__rt__qubit_allocate();
                let _generated_ident_132 : Qubit = __quantum__rt__qubit_allocate();
                let (q : Qubit, c1 : Qubit, c2 : Qubit, c3 : Qubit) = (_generated_ident_126, _generated_ident_128, _generated_ident_130, _generated_ident_132);
                ApplyCtl1(q, c1);
                ApplyCtl2(q, c1, c2);
                ApplyCtl3(q, c1, c2, c3);
                __quantum__rt__qubit_release(_generated_ident_132);
                __quantum__rt__qubit_release(_generated_ident_130);
                __quantum__rt__qubit_release(_generated_ident_128);
                __quantum__rt__qubit_release(_generated_ident_126);
            }
            // entry
            Main()
        "#]],
    );
}

#[test]
fn newtype_ctor_callable_field_cleanup() {
    // Pins the cleanup behavior for closures inside legacy-`newtype` UDT
    // constructor argument subtrees. The UDT-ctor guard in
    // `cleanup_consumed_closures` lets these closures be replaced after
    // their specialized callable is produced, ensuring convergence.
    //
    // This is the aggregate-slot position: `Choice`'s first field keeps its
    // `Int -> Int` type whatever replaces the closure, and no invariant walks
    // that slot. The snapshot therefore pins the *well-typed* replacement —
    // `Choice(_lambda_4, 100)`, a reference to the closure's own capture-free
    // target — rather than the `Choice((), 100)` this used to produce, which
    // put a `Unit` under an arrow-typed slot.
    //
    // Uses both `Choose(true)` and `Choose(false)` so each conditional
    // branch is specialized at least once; otherwise a literal-conditioned
    // projection leaves the unused branch's closure as dead-code and
    // convergence cannot succeed independently of the UDT-ctor guard.
    let source = r#"
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
          function Main() : Int {
            let selectedT = Choose(true);
            let selectedF = Choose(false);
            let fT = selectedT::F;
            let fF = selectedF::F;
            fT(10) + fF(10) + selectedT::Offset + selectedF::Offset
          }
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype Choice = ((Int -> Int), Int);
            function Choose(flag : Bool) : __UDT_Item_1__Package_2_ {
                if flag {
                    Choice(/ * closure item = 4 captures = [] * / _lambda_4, 100)
                } else {
                    Choice(/ * closure item = 5 captures = [] * / _lambda_5, 7)
                }

            }
            function Main() : Int {
                let selectedT : __UDT_Item_1__Package_2_ = Choose(true);
                let selectedF : __UDT_Item_1__Package_2_ = Choose(false);
                let fT : (Int -> Int) = selectedT::F;
                let fF : (Int -> Int) = selectedF::F;
                ((fT(10) + fF(10)) + selectedT::Offset) + selectedF::Offset
            }
            function _lambda_4(x : Int, ) : Int {
                x + 1
            }
            function _lambda_5(x : Int, ) : Int {
                x * 2
            }
            // entry
            Main()

            AFTER:
            newtype Choice = ((Int -> Int), Int);
            function Choose(flag : Bool) : __UDT_Item_1__Package_2_ {
                if flag {
                    Choice(_lambda_4, 100)
                } else {
                    Choice(_lambda_5, 7)
                }

            }
            function Main() : Int {
                let selectedT : __UDT_Item_1__Package_2_ = Choose(true);
                let selectedF : __UDT_Item_1__Package_2_ = Choose(false);
                ((if true {
                    _lambda_4(10)
                } else {
                    _lambda_5(10)
                } + if false {
                    _lambda_4(10)
                } else {
                    _lambda_5(10)
                }) + selectedT::Offset) + selectedF::Offset
            }
            function _lambda_4(x : Int, ) : Int {
                x + 1
            }
            function _lambda_5(x : Int, ) : Int {
                x * 2
            }
            // entry
            Main()
        "#]],
    );
}

// A select-style operation whose first parameter is a struct (UDT) is
// partially applied into a closure, then forwarded as `selectOp` through a
// factory that dispatches it via `Controlled selectOp([control], (systems,
// ancilla))`. Specialization must thread the captured struct through the
// controlled-dispatch layer so the rewritten call reads
// `Controlled _lambda_8([control], (__capture_0, (systems, ancilla)))` rather
// than dropping the capture and passing `(systems, ancilla)` directly. Dropping
// the capture would leave the call shape inconsistent with the specialized
// callee's input and trip the post-arg_promote call-shape invariant. This test
// drives the full pipeline (`check_pipeline`) so the shape is validated through
// argument promotion, and pairs it with a rewrite snapshot showing the threaded
// struct capture. It exercises the controlled struct-capture-threading path via
// the source `Main` entry; it does not reproduce the injected-closure entry
// rooting that a compiled-from-Python entry expression would produce.
#[test]
fn struct_capture_select_op_threads_through_controlled_dispatch_pipeline() {
    let source = r#"
        struct PauliSelectParams {
            paulis : Pauli[][],
            qubitIndices : Int[],
            signs : Int[]
        }

        operation ApplySelect(params : PauliSelectParams, systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
            if Length(params.signs) != 0 {
                X(systems[0]);
            }
        }

        operation ApplyPrepare(systems : Qubit[]) : Unit is Adj + Ctl {}

        function MakeControlledPrepSelPrepOp(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : (Qubit, Qubit[]) => Unit {
            (control, allQubits) => {
                let systems = allQubits[0..numSystemQubits - 1];
                let ancilla = allQubits[numSystemQubits...];
                for _ in 0..power - 1 {
                    Controlled prepareOp([control], systems);
                    Controlled selectOp([control], (systems, ancilla));
                }
            }
        }

        operation MakeControlledPrepSelPrepCircuit(
            prepareOp : Qubit[] => Unit is Adj + Ctl,
            selectOp : (Qubit[], Qubit[]) => Unit is Adj + Ctl,
            numSystemQubits : Int,
            power : Int
        ) : Unit {
            use control = Qubit();
            use systems = Qubit[numSystemQubits + 1];
            let op = MakeControlledPrepSelPrepOp(prepareOp, selectOp, numSystemQubits, power);
            op(control, systems);
        }

        operation Main() : Unit {
            let params = new PauliSelectParams {
                paulis = [[PauliX]],
                qubitIndices = [0],
                signs = [1]
            };
            let sel = ApplySelect(params, _, _);
            MakeControlledPrepSelPrepCircuit(ApplyPrepare, sel, 1, 1);
        }
        "#;
    check_pipeline(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            newtype PauliSelectParams = (Pauli[][], Int[], Int[]);
            operation ApplySelect(params : __UDT_Item_1__Package_2_, systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    if Length(params::signs) != 0 {
                        X(systems[0]);
                    }

                }
                adjoint ... {
                    if Length(params::signs) != 0 {
                        Adjoint X(systems[0]);
                    }

                }
                controlled (ctls, ...) {
                    if Length(params::signs) != 0 {
                        Controlled X(ctls, systems[0]);
                    }

                }
                controlled adjoint (ctls, ...) {
                    if Length(params::signs) != 0 {
                        Controlled Adjoint X(ctls, systems[0]);
                    }

                }
            }
            operation ApplyPrepare(systems : Qubit[]) : Unit is Adj + Ctl {
                body ... {}
                adjoint ... {}
                controlled (ctls, ...) {}
                controlled adjoint (ctls, ...) {}
            }
            function MakeControlledPrepSelPrepOp(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 7 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation MakeControlledPrepSelPrepCircuit(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                let op : ((Qubit, Qubit[]) => Unit) = MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp, selectOp, numSystemQubits, power);
                op(control, systems);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            operation Main() : Unit {
                let params : __UDT_Item_1__Package_2_ = new PauliSelectParams {
                    paulis = [[PauliX]],
                    qubitIndices = [0],
                    signs = [1]
                };
                let sel : ((Qubit[], Qubit[]) => Unit is Adj + Ctl) = {
                    let arg : __UDT_Item_1__Package_2_ = params;
                    / * closure item = 8 captures = [arg] * / _lambda_8
                };
                MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl_(ApplyPrepare, sel, 1, 1);
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346.Start;
                        let _step_id_354 : Int = _range_id_346.Step;
                        let _end_id_359 : Int = _range_id_346.End;
                        while ((_step_id_354 > 0) and (_index_id_349 <= _end_id_359)) or ((_step_id_354 < 0) and (_index_id_349 >= _end_id_359)) {
                            let _ : Int = _index_id_349;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation _lambda_8(arg : __UDT_Item_1__Package_2_, (hole : Qubit[], hole_1 : Qubit[])) : Unit is Adj + Ctl {
                body ... {
                    ApplySelect(arg, hole, hole_1)
                }
                adjoint ... {
                    Adjoint ApplySelect(arg, hole, hole_1)
                }
                controlled (ctls, ...) {
                    Controlled ApplySelect(ctls, (arg, hole, hole_1))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplySelect(ctls, (arg, hole, hole_1))
                }
            }
            function MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 10 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346.Start;
                        let _step_id_354 : Int = _range_id_346.Step;
                        let _end_id_359 : Int = _range_id_346.End;
                        while ((_step_id_354 > 0) and (_index_id_349 <= _end_id_359)) or ((_step_id_354 < 0) and (_index_id_349 >= _end_id_359)) {
                            let _ : Int = _index_id_349;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                let op : ((Qubit, Qubit[]) => Unit) = MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp, selectOp, numSystemQubits, power);
                op(control, systems);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            // entry
            Main()

            AFTER:
            newtype PauliSelectParams = (Pauli[][], Int[], Int[]);
            operation ApplySelect(params : __UDT_Item_1__Package_2_, systems : Qubit[], ancilla : Qubit[]) : Unit is Adj + Ctl {
                body ... {
                    if Length(params::signs) != 0 {
                        X(systems[0]);
                    }

                }
                adjoint ... {
                    if Length(params::signs) != 0 {
                        Adjoint X(systems[0]);
                    }

                }
                controlled (ctls, ...) {
                    if Length(params::signs) != 0 {
                        Controlled X(ctls, systems[0]);
                    }

                }
                controlled adjoint (ctls, ...) {
                    if Length(params::signs) != 0 {
                        Controlled Adjoint X(ctls, systems[0]);
                    }

                }
            }
            operation ApplyPrepare(systems : Qubit[]) : Unit is Adj + Ctl {
                body ... {}
                adjoint ... {}
                controlled (ctls, ...) {}
                controlled adjoint (ctls, ...) {}
            }
            function MakeControlledPrepSelPrepOp(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 7 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation MakeControlledPrepSelPrepCircuit(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                let op : ((Qubit, Qubit[]) => Unit) = MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp, selectOp, numSystemQubits, power);
                op(control, systems);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            operation Main() : Unit {
                let params : __UDT_Item_1__Package_2_ = new PauliSelectParams {
                    paulis = [[PauliX]],
                    qubitIndices = [0],
                    signs = [1]
                };
                MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl__ApplyPrepare__closure_(1, 1, params);
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit), selectOp : ((Qubit[], Qubit[]) => Unit), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346.Start;
                        let _step_id_354 : Int = _range_id_346.Step;
                        let _end_id_359 : Int = _range_id_346.End;
                        while ((_step_id_354 > 0) and (_index_id_349 <= _end_id_359)) or ((_step_id_354 < 0) and (_index_id_349 >= _end_id_359)) {
                            let _ : Int = _index_id_349;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation _lambda_8(arg : __UDT_Item_1__Package_2_, (hole : Qubit[], hole_1 : Qubit[])) : Unit is Adj + Ctl {
                body ... {
                    ApplySelect(arg, hole, hole_1)
                }
                adjoint ... {
                    Adjoint ApplySelect(arg, hole, hole_1)
                }
                controlled (ctls, ...) {
                    Controlled ApplySelect(ctls, (arg, hole, hole_1))
                }
                controlled adjoint (ctls, ...) {
                    Controlled Adjoint ApplySelect(ctls, (arg, hole, hole_1))
                }
            }
            function MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 10 captures = [prepareOp, selectOp, numSystemQubits, power] * / _lambda_7
            }
            operation _lambda_7(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346.Start;
                        let _step_id_354 : Int = _range_id_346.Step;
                        let _end_id_359 : Int = _range_id_346.End;
                        while ((_step_id_354 > 0) and (_index_id_349 <= _end_id_359)) or ((_step_id_354 < 0) and (_index_id_349 >= _end_id_359)) {
                            let _ : Int = _index_id_349;
                            Controlled prepareOp([control], systems);
                            Controlled selectOp([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl_(prepareOp : (Qubit[] => Unit is Adj + Ctl), selectOp : ((Qubit[], Qubit[]) => Unit is Adj + Ctl), numSystemQubits : Int, power : Int) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                _lambda_7(prepareOp, selectOp, numSystemQubits, power, (control, systems));
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            operation MakeControlledPrepSelPrepCircuit_AdjCtl__AdjCtl__ApplyPrepare__closure_(numSystemQubits : Int, power : Int, __capture_0 : __UDT_Item_1__Package_2_) : Unit {
                let control : Qubit = __quantum__rt__qubit_allocate();
                let systems : Qubit[] = AllocateQubitArray(numSystemQubits + 1);
                _lambda_7_ApplyPrepare__closure_(numSystemQubits, power, (control, systems), __capture_0);
                ReleaseQubitArray(systems);
                __quantum__rt__qubit_release(control);
            }
            function MakeControlledPrepSelPrepOp_AdjCtl__AdjCtl__ApplyPrepare__closure_(numSystemQubits : Int, power : Int, __capture_0 : __UDT_Item_1__Package_2_) : ((Qubit, Qubit[]) => Unit) {
                / * closure item = 14 captures = [__capture_0, numSystemQubits, power] * / _lambda_7
            }
            operation _lambda_7(__capture_0 : __UDT_Item_1__Package_2_, numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[])) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346.Start;
                        let _step_id_354 : Int = _range_id_346.Step;
                        let _end_id_359 : Int = _range_id_346.End;
                        while ((_step_id_354 > 0) and (_index_id_349 <= _end_id_359)) or ((_step_id_354 < 0) and (_index_id_349 >= _end_id_359)) {
                            let _ : Int = _index_id_349;
                            Controlled ApplyPrepare([control], systems);
                            Controlled _lambda_8([control], (systems, ancilla));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            operation _lambda_7_ApplyPrepare__closure_(numSystemQubits : Int, power : Int, (control : Qubit, allQubits : Qubit[]), __capture_0 : __UDT_Item_1__Package_2_) : Unit {
                {
                    let systems : Qubit[] = allQubits[0..numSystemQubits - 1];
                    let ancilla : Qubit[] = allQubits[numSystemQubits...];
                    {
                        let _range_id_346 : Range = 0..power - 1;
                        mutable _index_id_349 : Int = _range_id_346.Start;
                        let _step_id_354 : Int = _range_id_346.Step;
                        let _end_id_359 : Int = _range_id_346.End;
                        while ((_step_id_354 > 0) and (_index_id_349 <= _end_id_359)) or ((_step_id_354 < 0) and (_index_id_349 >= _end_id_359)) {
                            let _ : Int = _index_id_349;
                            Controlled ApplyPrepare([control], systems);
                            Controlled _lambda_8([control], (__capture_0, (systems, ancilla)));
                            _index_id_349 += _step_id_354;
                        }

                    }

                }

            }
            // entry
            Main()
        "#]],
    );
}

// A direct (non-higher-order) call over a callable local that mixes an
// intrinsic candidate with a partial application dispatches over both
// candidates. The partial application `Rx(0.0, _)` lowers to a closure-tailed
// block bound to `op`; once `op(q)` becomes a branch dispatch, that binding is
// dead. Removing the dead binding keeps closure cleanup from stranding an
// arrow-typed block with no producing tail, which would otherwise violate the
// non-Unit block-tail invariant. This pairs the invariant check with a rewrite
// snapshot showing the dead `op` binding removed and the dispatch inlining the
// specialized callees.
#[test]
fn width2_mixed_direct_dispatch_removes_dead_partial_app_binding() {
    let source = r#"
        operation Main() : Unit {
            use q = Qubit();
            let n = 1;
            mutable op = X;
            if n == 0 {
                op = Rx(0.0, _);
            }
            op(q);
        }
        "#;
    check_invariants(source);
    check_rewrite(
        source,
        &expect![[r#"
            BEFORE:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 1;
                mutable op : (Qubit => Unit is Adj + Ctl) = X;
                if n == 0 {
                    op = {
                        let arg : Double = 0.;
                        / * closure item = 2 captures = [arg] * / _lambda_2
                    };
                }

                op(q);
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_2(arg : Double, hole : Qubit) : Unit is Adj + Ctl {
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
            // entry
            Main()

            AFTER:
            operation Main() : Unit {
                let q : Qubit = __quantum__rt__qubit_allocate();
                let n : Int = 1;
                if n == 0 {}

                if n == 0 {
                    _lambda_2(0., q)
                } else {
                    X(q)
                };
                __quantum__rt__qubit_release(q);
            }
            operation _lambda_2(arg : Double, hole : Qubit) : Unit is Adj + Ctl {
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
            // entry
            Main()
        "#]],
    );
}

// Indexed-array analogue of the mixed direct dispatch: a callable-array literal
// mixing an intrinsic with a partial application is indexed inside a loop and
// dispatched directly. Once the indexed read is rewritten into a branch
// dispatch, both the indexed local and the now-dead source array (whose element
// holds a closure) are removed, so no dead arrow-typed binding remains.
#[test]
fn indexed_callable_array_mixed_direct_dispatch_passes_invariants() {
    let source = r#"
        operation Main() : Unit {
            use q = Qubit();
            let ops = [X, Rx(0.0, _)];
            for i in 0..1 {
                let op = ops[i];
                op(q);
            }
        }
        "#;
    check_invariants(source);
}

// Pure partial-application direct dispatch: both candidates are partial
// applications, confirming the fix keys on a consumed partial-application
// residual in a reachable block rather than on a mixed candidate set. The
// write-only `op` binding and its reassignment are removed once the dispatch
// consumes them.
#[test]
fn pure_partial_app_direct_dispatch_passes_invariants() {
    let source = r#"
        operation Main() : Unit {
            use q = Qubit();
            let c = true;
            mutable op = Rx(0.0, _);
            if c {
                op = Ry(0.0, _);
            }
            op(q);
        }
        "#;
    check_invariants(source);
}
