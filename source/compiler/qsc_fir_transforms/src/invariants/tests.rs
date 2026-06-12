// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;
use crate::invariants::test_utils::{
    clear_external_body_exec_graph, clear_external_copy_update_field_range,
    compile_external_copy_update_to_exec_graph_rebuild, convert_last_body_expr_to_semi,
    external_copy_update_spec_id, inject_arrow_param, inject_binding_type_mismatch,
    inject_call_argument_shape_mismatch, inject_callable_input_tuple_pattern_arity_mismatch,
    inject_callable_output_type, inject_closure_expr, inject_cross_spec_local_reference,
    inject_dangling_stmt_expr_id, inject_dangling_stmt_id, inject_functor_param_arrow,
    inject_initializer_self_reference, inject_local_tuple_pattern_arity_mismatch,
    inject_nested_non_tuple_field_path_target, inject_nested_tuple_bound_arrow_local,
    inject_nested_tuple_eq_in_if_branch, inject_non_copy_struct,
    inject_non_tuple_field_path_target, inject_non_unit_assignment_expression_type,
    inject_stale_local_var, inject_stale_local_var_in_callable, inject_tuple_arity_mismatch,
    inject_ty_param, inject_udt_callable_output, inject_udt_expr_type,
    inject_udt_expr_type_in_callable,
};
use crate::test_utils::{PipelineStage, assert_panics_with, compile_and_run_pipeline_to};

use qsc_fir::fir::LocalVarId;
use qsc_fir::ty::Prim;

/// Simple Q# source with a local variable binding.
const SIMPLE_LOCAL_VAR: &str = r#"
    namespace Test {
        @EntryPoint()
        function Main() : Int {
            let x = 42;
            x
        }
    }
"#;

const SIMPLE_ASSIGNMENT: &str = r#"
    namespace Test {
        @EntryPoint()
        function Main() : Int {
            mutable x = 1;
            x = 2;
            x
        }
    }
"#;

/// Q# with a struct field access to ensure `Field::Path` survives the full pipeline.
const STRUCT_FIELD_ACCESS: &str = r#"
    namespace Test {
        struct Pair { Fst: Int, Snd: Double }
        @EntryPoint()
        function Main() : (Int, Double) {
            let p = new Pair { Fst = 1, Snd = 2.0 };
            (p.Fst, p.Snd)
        }
    }
"#;

const STRUCT_FIELD_ACCESS_INSIDE_IF: &str = r#"
    namespace Test {
        @EntryPoint()
        function Main() : (Int, Double) {
            if true {
                (1, 2.0)
            } else {
                (0, 0.0)
            }
        }
    }
"#;

const PROMOTED_CALLABLE_INPUT: &str = r#"
    namespace Test {
        struct Pair { Fst: Int, Snd: Int }

        function Foo(p : Pair) : Int {
            p.Fst + p.Snd
        }

        @EntryPoint()
        function Main() : Int {
            Foo(new Pair { Fst = 1, Snd = 2 })
        }
    }
"#;

const PROMOTED_CALLABLE_VARIABLE_ARG: &str = r#"
    namespace Test {
        struct Pair { Fst: Int, Snd: Int }

        function Foo(p : Pair) : Int {
            p.Fst + p.Snd
        }

        @EntryPoint()
        function Main() : Int {
            let pair = new Pair { Fst = 1, Snd = 2 };
            Foo(pair)
        }
    }
"#;

const FUNCTOR_PROMOTED_CALLABLE_VARIABLE_ARG: &str = r#"
    namespace Test {
        struct Pair { Fst: Int, Snd: Int }

        operation Foo(p : Pair) : Unit is Ctl {
            body ... {
                let _ = p.Fst + p.Snd;
            }
            controlled (cs, ...) {
                let _ = p.Fst + p.Snd;
            }
        }

        @EntryPoint()
        operation Main() : Unit {
            use q = Qubit();
            let pair = new Pair { Fst = 1, Snd = 2 };
            Controlled Foo([q], pair);
        }
    }
"#;

const NESTED_TUPLE_LITERAL_INSIDE_IF: &str = r#"
    namespace Test {
        @EntryPoint()
        function Main() : ((Int, Int), (Int, Int)) {
            if true {
                ((1, 2), (3, 4))
            } else {
                ((5, 6), (7, 8))
            }
        }
    }
"#;

const SIMULATABLE_INTRINSIC_BODY: &str = r#"
    namespace Test {
        @SimulatableIntrinsic()
        operation MyMeasurement(q : Qubit) : Result {
            let r = M(q);
            r
        }

        @EntryPoint()
        operation Main() : Result {
            use q = Qubit();
            MyMeasurement(q)
        }
    }
"#;

#[test]
fn invariant_passes_with_valid_local_var() {
    let (store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Mono);
    check(&store, pkg_id, InvariantLevel::PostMono);
}

#[test]
fn post_udt_erase_passes_when_no_udt_types() {
    let (store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    check(&store, pkg_id, InvariantLevel::PostUdtErase);
}

#[test]
fn post_udt_erase_allows_copy_update_struct() {
    let source = r#"
        namespace Test {
            struct Pair { Fst: Int, Snd: Int }
            @EntryPoint()
            function Main() : Int {
                let p = new Pair { Fst = 1, Snd = 2 };
                let q = new Pair { ...p, Fst = 10 };
                q.Fst
            }
        }
    "#;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    check(&store, pkg_id, InvariantLevel::PostUdtErase);
}

#[test]
fn integration_post_udt_erase_invariant_passes() {
    let source = r#"
        namespace Test {
            struct Pair { Fst: Int, Snd: Double }
            @EntryPoint()
            function Main() : (Int, Double) {
                let p = new Pair { Fst = 1, Snd = 2.0 };
                (p.Fst, p.Snd)
            }
        }
    "#;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::UdtErase);
    check(&store, pkg_id, InvariantLevel::PostUdtErase);
}

#[test]
fn invariant_post_all_passes_after_full_pipeline() {
    let source = r#"
        namespace Test {
            struct Pair { Fst: Int, Snd: Double }
            operation ApplyOp(op : Qubit => Unit, q : Qubit) : Unit { op(q); }
            @EntryPoint()
            operation Main() : Unit {
                let p = new Pair { Fst = 1, Snd = 2.0 };
                use q = Qubit();
                ApplyOp(H, q);
            }
        }
    "#;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
fn divergent_fail_body_of_non_unit_callable_passes_block_tail() {
    // A function whose entire body is `fail` declares a non-Unit return type
    // but its trailing expression is divergent (never yields a value). Typeck
    // leaves the `fail` tail with a defaulted Unit type, so block.ty (Int) and
    // the tail type (Unit) differ; the block-tail invariant must tolerate this.
    let source = r#"
        namespace Test {
            function NotImplemented() : Int { fail "todo" }
            @EntryPoint()
            operation Main() : Unit {
                let _ = NotImplemented();
            }
        }
    "#;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
fn divergent_nested_if_fail_tail_passes_block_tail() {
    // Both branches of the trailing `if` diverge, so the `if` (typed Unit)
    // never yields a value even though the enclosing block is Int. The
    // block-tail invariant must tolerate this nested divergence.
    let source = r#"
        namespace Test {
            operation Choose(cond : Bool) : Int {
                if cond { fail "a" } else { fail "b" }
            }
            @EntryPoint()
            operation Main() : Unit {
                let _ = Choose(true);
            }
        }
    "#;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
fn nonunit_if_with_one_value_branch_passes_block_tail() {
    // Regression guard against over-broadening: a trailing `if` with one
    // value-producing branch (Int) is non-divergent and correctly typed Int,
    // so it must continue to pass the block-tail invariant.
    let source = r#"
        namespace Test {
            operation Pick(cond : Bool) : Int {
                if cond { 5 } else { fail "b" }
            }
            @EntryPoint()
            operation Main() : Unit {
                let _ = Pick(true);
            }
        }
    "#;
    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Full);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
fn invariant_rejects_non_unit_assignment_expression() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMPLE_ASSIGNMENT, PipelineStage::TupleDecompose);
    inject_non_unit_assignment_expression_type(&mut store, pkg_id, "Main");
    assert_panics_with("Assignment type invariant violation", || {
        check(&store, pkg_id, InvariantLevel::PostTupleDecompose);
    });
}

#[test]
fn external_exec_graph_checker_rejects_empty_mutated_external_spec_graph() {
    let (mut store, _pkg_id, external_callable) =
        compile_external_copy_update_to_exec_graph_rebuild();
    clear_external_body_exec_graph(&mut store, external_callable);

    assert_panics_with(
        "Exec graph for external MakeUpdated/body (no_debug) is empty",
        || {
            check_external_spec_exec_graphs(
                &store,
                &[external_copy_update_spec_id(external_callable)],
            );
        },
    );
}

#[test]
fn external_exec_graph_checker_rejects_empty_mutated_external_expr_range() {
    let (mut store, _pkg_id, external_callable) =
        compile_external_copy_update_to_exec_graph_rebuild();
    clear_external_copy_update_field_range(&mut store, external_callable);

    assert_panics_with(
        "Exec graph range for external MakeUpdated/body Expr",
        || {
            check_external_spec_exec_graphs(
                &store,
                &[external_copy_update_spec_id(external_callable)],
            );
        },
    );
}

#[test]
fn invariant_catches_stale_local_var() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Mono);
    inject_stale_local_var(&mut store, pkg_id, LocalVarId::from(9999u32));
    assert_panics_with("LocalVarId consistency", || {
        check(&store, pkg_id, InvariantLevel::PostMono);
    });
}

#[test]
fn scoped_local_rejects_cross_spec_local_reference() {
    let source = r#"
        namespace Test {
            operation CrossSpec() : Unit is Adj {
                body (...) {
                    let bodyOnly = 1;
                    let _ = bodyOnly;
                }

                adjoint (...) {
                    let adjOnly = 2;
                    let _ = adjOnly;
                }
            }

            @EntryPoint()
            operation Main() : Unit {
                CrossSpec();
                Adjoint CrossSpec();
            }
        }
    "#;

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    inject_cross_spec_local_reference(&mut store, pkg_id, "CrossSpec");
    assert_panics_with("LocalVarId consistency", || {
        check(&store, pkg_id, InvariantLevel::PostMono);
    });
}

#[test]
fn scoped_local_rejects_initializer_self_reference() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            function Main() : Int {
                let value = 1;
                value
            }
        }
    "#;

    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Mono);
    inject_initializer_self_reference(&mut store, pkg_id, "Main");
    assert_panics_with("LocalVarId consistency", || {
        check(&store, pkg_id, InvariantLevel::PostMono);
    });
}

#[test]
fn post_udt_erase_catches_remaining_udt_type() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    inject_udt_expr_type(&mut store, pkg_id);
    assert_panics_with("Ty::Udt after UDT erasure", || {
        check(&store, pkg_id, InvariantLevel::PostUdtErase);
    });
}

#[test]
fn post_udt_erase_catches_non_copy_struct_expr() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    inject_non_copy_struct(&mut store, pkg_id);
    assert_panics_with("ExprKind::Struct after UDT erasure", || {
        check(&store, pkg_id, InvariantLevel::PostUdtErase);
    });
}

#[test]
fn post_udt_erase_catches_udt_in_callable_output() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    inject_udt_callable_output(&mut store, pkg_id);
    assert_panics_with("Ty::Udt after UDT erasure", || {
        check(&store, pkg_id, InvariantLevel::PostUdtErase);
    });
}

#[test]
fn invariant_catches_functor_set_param_post_mono() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Mono);
    inject_functor_param_arrow(&mut store, pkg_id);
    assert_panics_with("FunctorSet::Param after monomorphization", || {
        check(&store, pkg_id, InvariantLevel::PostMono);
    });
}

#[test]
fn invariant_post_defunc_catches_closure() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Defunc);
    inject_closure_expr(&mut store, pkg_id);
    assert_panics_with("is a Closure after defunctionalization", || {
        check(&store, pkg_id, InvariantLevel::PostDefunc);
    });
}

#[test]
fn invariant_post_defunc_catches_arrow_param() {
    // Need a callable with a named parameter (PatKind::Bind) so the
    // arrow-type injection is caught by check_pat_for_arrow.
    let source = r#"
        namespace Test {
            function Helper(x : Int) : Int { x }
            @EntryPoint()
            function Main() : Int { Helper(42) }
        }
    "#;
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Defunc);
    inject_arrow_param(&mut store, pkg_id);
    assert_panics_with("Arrow-typed parameter remains in callable input", || {
        check(&store, pkg_id, InvariantLevel::PostDefunc);
    });
}

#[test]
fn post_tuple_decompose_catches_nested_tuple_bound_arrow() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            function Main() : ((Int, Int), Int) {
                let value = ((1, 2), 3);
                value
            }
        }
    "#;
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::TupleDecompose);
    inject_nested_tuple_bound_arrow_local(&mut store, pkg_id);
    assert_panics_with("tuple-bound local retains an arrow-typed field", || {
        check(&store, pkg_id, InvariantLevel::PostTupleDecompose);
    });
}

#[test]
fn invariant_post_mono_catches_ty_param() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Mono);
    inject_ty_param(&mut store, pkg_id);
    assert_panics_with("Ty::Param", || {
        check(&store, pkg_id, InvariantLevel::PostMono);
    });
}

#[test]
fn post_all_field_path_on_tuple_passes() {
    let (store, pkg_id) = compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS, PipelineStage::Full);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
fn post_tuple_decompose_tuple_local_pattern_passes() {
    let (store, pkg_id) =
        compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS, PipelineStage::TupleDecompose);
    check(&store, pkg_id, InvariantLevel::PostTupleDecompose);
}

#[test]
fn post_tuple_decompose_catches_tuple_local_pattern_arity_mismatch() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS, PipelineStage::TupleDecompose);
    inject_local_tuple_pattern_arity_mismatch(&mut store, pkg_id);
    assert_panics_with("Tuple pattern/type invariant violation", || {
        check(&store, pkg_id, InvariantLevel::PostTupleDecompose);
    });
}

#[test]
fn post_arg_promote_tuple_input_pattern_passes() {
    let (store, pkg_id) =
        compile_and_run_pipeline_to(PROMOTED_CALLABLE_INPUT, PipelineStage::ArgPromote);
    check(&store, pkg_id, InvariantLevel::PostArgPromote);
}

#[test]
fn post_item_dce_cut_point_passes_invariant() {
    let source = r#"
        namespace Test {
            function Unused() : Int { 42 }

            @EntryPoint()
            function Main() : Int { 1 }
        }
    "#;

    let (store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::ItemDce);
    check(&store, pkg_id, InvariantLevel::PostItemDce);
}

#[test]
fn post_arg_promote_catches_callable_input_pattern_arity_mismatch() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(PROMOTED_CALLABLE_INPUT, PipelineStage::ArgPromote);
    inject_callable_input_tuple_pattern_arity_mismatch(&mut store, pkg_id, "Foo");
    assert_panics_with("Tuple pattern/type invariant violation", || {
        check(&store, pkg_id, InvariantLevel::PostArgPromote);
    });
}

#[test]
fn post_arg_promote_catches_functor_wrapper_stale_item_signature() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(
        FUNCTOR_PROMOTED_CALLABLE_VARIABLE_ARG,
        PipelineStage::ArgPromote,
    );
    inject_callable_output_type(&mut store, pkg_id, "Foo", Ty::Prim(Prim::Int));
    assert_panics_with("PostArgPromote/PostAll call invariant violation", || {
        check(&store, pkg_id, InvariantLevel::PostArgPromote);
    });
}

#[test]
fn post_mono_catches_stale_local_in_simulatable_intrinsic_body() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMULATABLE_INTRINSIC_BODY, PipelineStage::Mono);
    inject_stale_local_var_in_callable(
        &mut store,
        pkg_id,
        "MyMeasurement",
        LocalVarId::from(9999u32),
    );
    assert_panics_with("LocalVarId consistency", || {
        check(&store, pkg_id, InvariantLevel::PostMono);
    });
}

#[test]
fn post_all_catches_simulatable_intrinsic_body_type_violation() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMULATABLE_INTRINSIC_BODY, PipelineStage::Full);
    inject_udt_expr_type_in_callable(&mut store, pkg_id, "MyMeasurement");
    assert_panics_with("contains Ty::Udt after UDT erasure", || {
        check(&store, pkg_id, InvariantLevel::PostAll);
    });
}

#[test]
fn post_all_field_path_on_non_tuple_panics() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS, PipelineStage::Full);
    inject_non_tuple_field_path_target(&mut store, pkg_id);
    assert_panics_with("Field::Path on non-tuple", || {
        check(&store, pkg_id, InvariantLevel::PostAll);
    });
}

#[test]
fn post_all_catches_nested_field_path_on_non_tuple_inside_if_branch() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(STRUCT_FIELD_ACCESS_INSIDE_IF, PipelineStage::Full);
    inject_nested_non_tuple_field_path_target(&mut store, pkg_id);
    assert_panics_with("Field::Path on non-tuple", || {
        check(&store, pkg_id, InvariantLevel::PostAll);
    });
}

#[test]
fn post_all_binding_type_consistency_passes() {
    let (store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Full);
    check(&store, pkg_id, InvariantLevel::PostAll);
}

#[test]
fn post_all_binding_type_mismatch_panics() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Full);
    inject_binding_type_mismatch(&mut store, pkg_id);
    assert_panics_with("PostReturnUnify invariant violation: local binding", || {
        check(&store, pkg_id, InvariantLevel::PostAll);
    });
}

#[test]
fn post_all_catches_call_argument_shape_mismatch() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(PROMOTED_CALLABLE_VARIABLE_ARG, PipelineStage::Full);
    inject_call_argument_shape_mismatch(&mut store, pkg_id, "Main");
    assert_panics_with("PostArgPromote/PostAll call invariant violation", || {
        check(&store, pkg_id, InvariantLevel::PostAll);
    });
}

#[test]
fn post_defunc_catches_tuple_arity_mismatch() {
    let source = r#"
        namespace Test {
            @EntryPoint()
            function Main() : (Int, Int, Int) {
                (1, 2, 3)
            }
        }
    "#;
    let (mut store, pkg_id) = compile_and_run_pipeline_to(source, PipelineStage::Defunc);
    inject_tuple_arity_mismatch(&mut store, pkg_id);
    assert_panics_with("Tuple arity mismatch", || {
        check(&store, pkg_id, InvariantLevel::PostDefunc);
    });
}

#[test]
fn post_defunc_catches_non_unit_block_tail_violation() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Defunc);
    convert_last_body_expr_to_semi(&mut store, pkg_id);
    assert_panics_with("Non-Unit block-tail invariant violation", || {
        check(&store, pkg_id, InvariantLevel::PostDefunc);
    });
}

#[test]
fn post_tuple_comp_lower_catches_nested_tuple_eq_inside_if_branch() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(
        NESTED_TUPLE_LITERAL_INSIDE_IF,
        PipelineStage::TupleCompLower,
    );
    inject_nested_tuple_eq_in_if_branch(&mut store, pkg_id);
    assert_panics_with("PostTupleCompLower invariant violation", || {
        check(&store, pkg_id, InvariantLevel::PostTupleCompLower);
    });
}

#[test]
fn post_item_dce_catches_dangling_stmt_expr_reference() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::ItemDce);
    inject_dangling_stmt_expr_id(&mut store, pkg_id);
    assert_panics_with("references nonexistent Expr", || {
        check(&store, pkg_id, InvariantLevel::PostItemDce);
    });
}

#[test]
fn invariant_catches_dangling_stmt_id_in_block() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Full);
    inject_dangling_stmt_id(&mut store, pkg_id);
    assert_panics_with("references nonexistent Stmt", || {
        check(&store, pkg_id, InvariantLevel::PostAll);
    });
}

// `InvariantLevel::PostSignaturePreserving` membership is hand-placed in each
// `is_post_*_or_later` predicate (see `invariants.rs`) rather than derived from a
// single monotone threshold. That makes it vulnerable to silent weakening:
// dropping `PostSignaturePreserving` from a return/tuple predicate
// would stop rejecting residue the body-only sub-pipeline must remove, while adding
// it to the defunc/UDT predicates would wrongly reject the arrow/closure/UDT residue
// the sub-pipeline legitimately preserves. The tests below pin both directions of
// that membership matrix and confirm the main-pipeline levels are unchanged.
//
// These complement the behavioral checks in `signature_preserving_tests.rs`, which
// drive the real seed-rooted sub-pipeline end to end. Here we inject a single
// isolated residue into otherwise fully-processed FIR and check the levels
// directly, so a future per-predicate mis-edit is caught at its source.

/// A dynamic (measurement-dependent) early return inside the entry callable.
/// `monomorphize` runs before `return_unify`, so compiling only to
/// `PipelineStage::Mono` leaves the `ExprKind::Return` in place.
const DYNAMIC_EARLY_RETURN: &str = r#"
    namespace Test {
        import Std.Measurement.*;
        @EntryPoint()
        operation Main() : Int {
            use q = Qubit();
            if MResetZ(q) == One {
                return 1;
            }
            2
        }
    }
"#;

/// A callable with a named (`PatKind::Bind`) parameter so `inject_arrow_param`
/// produces an arrow-typed input pattern the defunc predicate inspects.
const NAMED_PARAM_CALLABLE: &str = r#"
    namespace Test {
        function Helper(x : Int) : Int { x }
        @EntryPoint()
        function Main() : Int { Helper(42) }
    }
"#;

// INCLUDE side: the sub-pipeline runs `return_unify`, so `PostSignaturePreserving`
// must still reject a residual dynamic `ExprKind::Return`. Removing
// `PostSignaturePreserving` from `is_post_return_unify_or_later` would silently
// weaken the sub-pipeline check and stop this test from panicking.
#[test]
fn post_signature_preserving_guard_rejects_residual_return() {
    let (store, pkg_id) = compile_and_run_pipeline_to(DYNAMIC_EARLY_RETURN, PipelineStage::Mono);
    assert_panics_with("ExprKind::Return found", || {
        check(&store, pkg_id, InvariantLevel::PostSignaturePreserving);
    });
}

// EXCLUDE side: the sub-pipeline preserves arrow-typed parameters, so
// `PostSignaturePreserving` must NOT fire on an injected arrow param. Adding
// `PostSignaturePreserving` to `is_post_defunc_or_later` would make this panic.
#[test]
fn post_signature_preserving_guard_allows_arrow_param() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(NAMED_PARAM_CALLABLE, PipelineStage::Defunc);
    inject_arrow_param(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostSignaturePreserving);
}

// Cross-level confirmation on the SAME fragment: `PostDefunc` still rejects the
// arrow residue that `PostSignaturePreserving` allows.
#[test]
fn post_signature_preserving_guard_arrow_param_still_rejected_post_defunc() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(NAMED_PARAM_CALLABLE, PipelineStage::Defunc);
    inject_arrow_param(&mut store, pkg_id);
    assert_panics_with("Arrow-typed parameter remains in callable input", || {
        check(&store, pkg_id, InvariantLevel::PostDefunc);
    });
}

// EXCLUDE side: the sub-pipeline preserves `ExprKind::Closure`.
#[test]
fn post_signature_preserving_guard_allows_closure() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Defunc);
    inject_closure_expr(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostSignaturePreserving);
}

// Cross-level confirmation on the SAME fragment: `PostDefunc` still rejects the
// closure residue that `PostSignaturePreserving` allows.
#[test]
fn post_signature_preserving_guard_closure_still_rejected_post_defunc() {
    let (mut store, pkg_id) = compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::Defunc);
    inject_closure_expr(&mut store, pkg_id);
    assert_panics_with("is a Closure after defunctionalization", || {
        check(&store, pkg_id, InvariantLevel::PostDefunc);
    });
}

// EXCLUDE side: the sub-pipeline preserves `Ty::Udt`.
#[test]
fn post_signature_preserving_guard_allows_udt_type() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    inject_udt_expr_type(&mut store, pkg_id);
    check(&store, pkg_id, InvariantLevel::PostSignaturePreserving);
}

// Cross-level confirmation on the SAME fragment: `PostUdtErase` still rejects the
// UDT residue that `PostSignaturePreserving` allows.
#[test]
fn post_signature_preserving_guard_udt_still_rejected_post_udt_erase() {
    let (mut store, pkg_id) =
        compile_and_run_pipeline_to(SIMPLE_LOCAL_VAR, PipelineStage::UdtErase);
    inject_udt_expr_type(&mut store, pkg_id);
    assert_panics_with("Ty::Udt after UDT erasure", || {
        check(&store, pkg_id, InvariantLevel::PostUdtErase);
    });
}
