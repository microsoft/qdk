// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! FIR structural invariant checker.
//!
//! Verifies that the FIR is well-formed after each transformation pass.
//! Different invariant levels check progressively stronger properties as more
//! passes have been applied.
//!
//! [`InvariantLevel`] variants correspond to pipeline stages in order:
//!
//! | Variant | Checked after |
//! |---|---|
//! | `PostMono` | Monomorphization — no `Ty::Param` in reachable code. |
//! | `PostReturnUnify` | Return unification — no `ExprKind::Return`. |
//! | `PostDefunc` | Defunctionalization — no `Ty::Arrow` / closures. |
//! | `PostUdtErase` | UDT erasure — no `Ty::Udt` / struct exprs. |
//! | `PostTupleCompLower` | Tuple comparison lowering. |
//! | `PostSroa` | SROA — tuple decomposition patterns match types. |
//! | `PostArgPromote` | Argument promotion — input patterns match types. |
//! | `PostGc` | Unreachable GC — no orphaned arena node references. |
//! | `PostAll` | All passes — full structural + type checks. |
//!
#[cfg(test)]
mod tests;

use crate::fir_builder::functored_specs;
use qsc_fir::fir::{
    BinOp, BlockId, CallableDecl, CallableImpl, ExecGraphConfig, ExecGraphDebugNode, ExecGraphNode,
    ExprId, ExprKind, Field, ItemKind, LocalItemId, LocalVarId, Package, PackageId, PackageLookup,
    PackageStore, PatId, PatKind, Res, SpecDecl, StmtKind, StoreItemId,
};
use qsc_fir::ty::{FunctorSet, Ty};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::reachability::{collect_reachable_from_entry, collect_reachable_package_closure};

/// The level of invariant checking to perform, corresponding to which passes
/// have already been applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InvariantLevel {
    /// After monomorphization: no `Ty::Param` in reachable code.
    PostMono,
    /// After return unification: additionally no `ExprKind::Return` in reachable code.
    PostReturnUnify,
    /// After defunctionalization: additionally no `Ty::Arrow` params and no
    /// `ExprKind::Closure` in reachable code.
    PostDefunc,
    /// After UDT erasure: additionally no `Ty::Udt`, no
    /// `ExprKind::Struct`, and no `Field::Path` in `UpdateField`/`AssignField`.
    PostUdtErase,
    /// After tuple comparison lowering: additionally no `BinOp(Eq/Neq)` on
    /// tuple-typed operands.
    PostTupleCompLower,
    /// After SROA: additionally synthesized local tuple patterns must match
    /// the tuple types they decompose.
    PostSroa,
    /// After argument promotion: additionally synthesized callable input tuple
    /// patterns must match the callable input types they decompose.
    PostArgPromote,
    /// After unreachable GC: no orphaned arena node references survive in the
    /// live FIR tree. Inherits all [`PostArgPromote`](Self::PostArgPromote)
    /// checks.
    PostGc,
    /// After all passes: all structural checks plus per-pass type constraints.
    PostAll,
}

impl InvariantLevel {
    /// Returns `true` when this level is at or after monomorphization.
    fn is_post_mono_or_later(self) -> bool {
        matches!(
            self,
            Self::PostMono
                | Self::PostReturnUnify
                | Self::PostDefunc
                | Self::PostUdtErase
                | Self::PostTupleCompLower
                | Self::PostSroa
                | Self::PostArgPromote
                | Self::PostGc
                | Self::PostAll
        )
    }

    /// Returns `true` when this level is at or after return unification.
    fn is_post_return_unify_or_later(self) -> bool {
        matches!(
            self,
            Self::PostReturnUnify
                | Self::PostDefunc
                | Self::PostUdtErase
                | Self::PostTupleCompLower
                | Self::PostSroa
                | Self::PostArgPromote
                | Self::PostGc
                | Self::PostAll
        )
    }

    /// Returns `true` when this level is at or after defunctionalization.
    fn is_post_defunc_or_later(self) -> bool {
        matches!(
            self,
            Self::PostDefunc
                | Self::PostUdtErase
                | Self::PostTupleCompLower
                | Self::PostSroa
                | Self::PostArgPromote
                | Self::PostGc
                | Self::PostAll
        )
    }

    /// Returns `true` when this level is at or after UDT erasure.
    fn is_post_udt_erase_or_later(self) -> bool {
        matches!(
            self,
            Self::PostUdtErase
                | Self::PostTupleCompLower
                | Self::PostSroa
                | Self::PostArgPromote
                | Self::PostGc
                | Self::PostAll
        )
    }

    /// Returns `true` when this level is at or after tuple comparison lowering.
    fn is_post_tuple_comp_lower_or_later(self) -> bool {
        matches!(
            self,
            Self::PostTupleCompLower
                | Self::PostSroa
                | Self::PostArgPromote
                | Self::PostGc
                | Self::PostAll
        )
    }

    /// Returns `true` when this level is at or after SROA.
    fn is_post_sroa_or_later(self) -> bool {
        matches!(
            self,
            Self::PostSroa | Self::PostArgPromote | Self::PostGc | Self::PostAll
        )
    }

    /// Returns `true` when this level is at or after argument promotion.
    fn is_post_arg_promote_or_later(self) -> bool {
        matches!(self, Self::PostArgPromote | Self::PostGc | Self::PostAll)
    }
}

/// Checks FIR structural invariants on entry-reachable code.
///
/// The invariant walk is scoped to items reachable from the target package's
/// entry expression. Items pinned for backend codegen (e.g. for
/// `fir_to_qir_from_callable`) are excluded from this check — the production
/// pipeline intentionally limits invariant enforcement to the entry-rooted
/// reachability closure.
///
/// # Panics
///
/// Panics with a descriptive message if any invariant is violated.
pub fn check(store: &PackageStore, package_id: qsc_fir::fir::PackageId, level: InvariantLevel) {
    let package = store.get(package_id);
    check_id_references(package);

    let Some(entry_id) = package.entry else {
        return;
    };

    let reachable = collect_reachable_from_entry(store, package_id);
    if level.is_post_udt_erase_or_later() {
        let reachable_packages = collect_reachable_package_closure(package_id, &reachable);
        for reachable_package_id in reachable_packages {
            let reachable_package = store.get(reachable_package_id);
            if reachable_package_id != package_id {
                check_id_references(reachable_package);
            }
            check_package_udt_erase_invariants(reachable_package);
        }
    }

    check_reachable_invariants(store, package_id, &reachable, level);

    if level.is_post_defunc_or_later() {
        check_expr_id_ownership(store, package_id, &reachable, entry_id);
    }

    if level.is_post_return_unify_or_later() {
        check_non_unit_block_tails(store, package_id, &reachable);
    }

    // Check type invariants on the entry expression tree.
    check_expr_types(store, package, entry_id, level);

    // After all passes, validate the entry exec graph.
    if level == InvariantLevel::PostAll {
        for (config, label) in [
            (ExecGraphConfig::NoDebug, "no_debug"),
            (ExecGraphConfig::Debug, "debug"),
        ] {
            let nodes = package.entry_exec_graph.select_ref(config);
            check_configured_exec_graph(package, nodes, "entry_exec_graph", label);
        }
    }
}

/// Validates the package-wide surfaces that `udt_erase` mutates.
///
/// The pass rewrites expression types and kinds, pattern types, block types,
/// and callable output types across every package in the reachable package
/// closure. This checker mirrors that mutation boundary without applying the
/// stronger target-package-only assumptions from later passes.
fn check_package_udt_erase_invariants(package: &Package) {
    for (expr_id, _expr) in &package.exprs {
        check_expr_udt_erase_invariants(package, expr_id);
    }

    for (pat_id, pat) in &package.pats {
        check_type_udt_erase_invariants(&pat.ty, &format!("Pat {pat_id}"));
    }

    for (block_id, block) in &package.blocks {
        check_type_udt_erase_invariants(&block.ty, &format!("Block {block_id}"));
    }

    for (item_id, item) in &package.items {
        if let ItemKind::Callable(decl) = &item.kind {
            check_type_udt_erase_invariants(&decl.output, &format!("Callable {item_id} output"));
        }
    }
}

/// Validates that a single expression satisfies post-UDT-erasure invariants:
/// no `Ty::Udt` in its type, no `ExprKind::Struct`, no `Field::Path` in
/// `UpdateField`/`AssignField`, and `Field::Path` only on tuple-typed records.
///
/// # Panics
///
/// Panics with a descriptive message if any UDT-erasure invariant is violated.
fn check_expr_udt_erase_invariants(package: &Package, expr_id: ExprId) {
    let expr = package.get_expr(expr_id);
    check_type_udt_erase_invariants(&expr.ty, &format!("Expr {expr_id}"));

    if matches!(&expr.kind, ExprKind::Struct(_, _, _)) {
        panic!(
            "PostUdtErase invariant violation: Expr {expr_id} contains \
             ExprKind::Struct after UDT erasure"
        );
    }

    if let ExprKind::UpdateField(_, Field::Path(_), _)
    | ExprKind::AssignField(_, Field::Path(_), _) = &expr.kind
    {
        panic!(
            "PostUdtErase invariant violation: Expr {expr_id} contains \
             Field::Path in UpdateField/AssignField after UDT erasure"
        );
    }

    if let ExprKind::Field(record_id, Field::Path(_)) = &expr.kind {
        let record = package.get_expr(*record_id);
        assert!(
            matches!(&record.ty, Ty::Tuple(_)),
            "PostUdtErase invariant violation: Expr {expr_id} has Field::Path \
             on non-tuple record Expr {record_id} (type: {:?})",
            record.ty,
        );
    }
}

/// Recursively validates that a type contains no `Ty::Udt` variants.
///
/// # Panics
///
/// Panics if `Ty::Udt` is found anywhere within the type tree.
fn check_type_udt_erase_invariants(ty: &Ty, context: &str) {
    match ty {
        Ty::Array(inner) => check_type_udt_erase_invariants(inner, context),
        Ty::Tuple(items) => {
            for item in items {
                check_type_udt_erase_invariants(item, context);
            }
        }
        Ty::Arrow(arrow) => {
            check_type_udt_erase_invariants(&arrow.input, context);
            check_type_udt_erase_invariants(&arrow.output, context);
        }
        Ty::Udt(_) => {
            panic!("{context} contains Ty::Udt after UDT erasure");
        }
        Ty::Prim(_) | Ty::Param(_) | Ty::Infer(_) | Ty::Err => {}
    }
}

/// Verifies that every reachable non-Unit callable body block and nested block
/// expression ends in a trailing expression whose type matches the block type.
///
/// This dispatcher fans out to `check_callable_non_unit_block_tails` for every
/// reachable callable, then runs `check_nested_block_expr_tails` on the entry
/// expression so nested block expressions outside callable bodies are covered
/// too.
///
/// This invariant is only valid after return unification has collapsed terminal
/// wrappers and for all later pipeline checkpoints.
///
/// # Panics
///
/// Panics with a descriptive message if any non-Unit block lacks a matching
/// trailing `StmtKind::Expr`.
pub(crate) fn check_non_unit_block_tails(
    store: &PackageStore,
    package_id: qsc_fir::fir::PackageId,
    reachable: &FxHashSet<StoreItemId>,
) {
    let package = store.get(package_id);
    let Some(entry_id) = package.entry else {
        return;
    };

    for item_id in reachable {
        if item_id.package != package_id {
            continue;
        }

        let item_pkg = store.get(item_id.package);
        let item = item_pkg.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            check_callable_non_unit_block_tails(item_pkg, decl);
        }
    }

    check_nested_block_expr_tails(package, entry_id, "entry expression");
}

/// Checks the root blocks for a callable body and each explicit specialization,
/// then re-walks the callable implementation to validate every nested block
/// expression through `check_non_unit_block_tail`.
fn check_callable_non_unit_block_tails(package: &Package, decl: &CallableDecl) {
    let callable_name = decl.name.name.to_string();

    match &decl.implementation {
        CallableImpl::Spec(spec_impl) => {
            check_spec_block_tail(
                package,
                &spec_impl.body,
                &format!("callable '{callable_name}' body"),
            );

            for (label, spec) in [
                ("adj", &spec_impl.adj),
                ("ctl", &spec_impl.ctl),
                ("ctl_adj", &spec_impl.ctl_adj),
            ] {
                if let Some(spec) = spec {
                    check_spec_block_tail(
                        package,
                        spec,
                        &format!("callable '{callable_name}' {label}"),
                    );
                }
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            check_spec_block_tail(
                package,
                spec,
                &format!("callable '{callable_name}' simulatable intrinsic"),
            );
        }
        CallableImpl::Intrinsic => {}
    }

    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |expr_id, expr| {
            if let ExprKind::Block(block_id) = &expr.kind {
                check_non_unit_block_tail(
                    package,
                    *block_id,
                    &format!("callable '{callable_name}' Expr {expr_id}"),
                );
            }
        },
    );
}

/// Small adapter that routes a specialization root block into the general
/// non-Unit tail checker.
fn check_spec_block_tail(package: &Package, spec: &SpecDecl, context: &str) {
    check_non_unit_block_tail(package, spec.block, context);
}

/// Walks an expression tree and applies `check_non_unit_block_tail` to every
/// nested `ExprKind::Block` it finds.
fn check_nested_block_expr_tails(package: &Package, expr_id: ExprId, context: &str) {
    crate::walk_utils::for_each_expr(package, expr_id, &mut |nested_expr_id, expr| {
        if let ExprKind::Block(block_id) = &expr.kind {
            check_non_unit_block_tail(
                package,
                *block_id,
                &format!("{context} Expr {nested_expr_id}"),
            );
        }
    });
}

/// Validates the trailing statement shape for a single non-Unit block.
///
/// This is the leaf helper used by the higher-level non-Unit block-tail
/// walkers once they have identified a specific block that should already be
/// in single-exit form.
///
/// # Panics
///
/// Panics if the block has a non-Unit type but is empty, ends in a non-Expr
/// statement, or ends in an expression whose type does not match the block
/// type.
fn check_non_unit_block_tail(package: &Package, block_id: BlockId, context: &str) {
    let block = package.get_block(block_id);
    if block.ty == Ty::UNIT {
        return;
    }

    let Some(&stmt_id) = block.stmts.last() else {
        panic!(
            "Non-Unit block-tail invariant violation: {context} Block {block_id} has type {:?} but has no trailing statement",
            block.ty,
        );
    };

    let stmt = package.get_stmt(stmt_id);
    let expr_id = match &stmt.kind {
        StmtKind::Expr(expr_id) => *expr_id,
        StmtKind::Semi(expr_id) => {
            panic!(
                "Non-Unit block-tail invariant violation: {context} Block {block_id} has type {:?} but ends with Semi Expr {expr_id}",
                block.ty,
            );
        }
        StmtKind::Local(..) => {
            panic!(
                "Non-Unit block-tail invariant violation: {context} Block {block_id} has type {:?} but ends with a Local statement",
                block.ty,
            );
        }
        StmtKind::Item(_) => {
            panic!(
                "Non-Unit block-tail invariant violation: {context} Block {block_id} has type {:?} but ends with an Item statement",
                block.ty,
            );
        }
    };

    let expr_ty = &package.get_expr(expr_id).ty;
    assert!(
        expr_ty == &block.ty,
        "Non-Unit block-tail invariant violation: {context} Block {block_id} has type {:?} but trailing Expr {expr_id} has type {expr_ty:?}",
        block.ty,
    );
}

/// Verifies that all IDs referenced inside blocks, stmts, exprs, and pats
/// actually exist in their respective `IndexMap`s.
fn check_id_references(package: &Package) {
    for (block_id, block) in &package.blocks {
        assert_eq!(
            block.id, block_id,
            "Block {block_id} has mismatched id field"
        );
        for &stmt_id in &block.stmts {
            assert!(
                package.stmts.get(stmt_id).is_some(),
                "Block {block_id} references nonexistent Stmt {stmt_id}"
            );
        }
    }

    for (stmt_id, stmt) in &package.stmts {
        assert_eq!(stmt.id, stmt_id, "Stmt {stmt_id} has mismatched id field");
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                assert!(
                    package.exprs.get(*e).is_some(),
                    "Stmt {stmt_id} references nonexistent Expr {e}"
                );
            }
            StmtKind::Local(_, pat, expr) => {
                assert!(
                    package.pats.get(*pat).is_some(),
                    "Stmt {stmt_id} references nonexistent Pat {pat}"
                );
                assert!(
                    package.exprs.get(*expr).is_some(),
                    "Stmt {stmt_id} references nonexistent Expr {expr}"
                );
            }
            StmtKind::Item(_) => {
                // After item DCE, `StmtKind::Item` stmts may reference
                // items that were removed. This is benign: the exec graph
                // never executes through item-definition stmts.
            }
        }
    }

    for (expr_id, expr) in &package.exprs {
        assert_eq!(expr.id, expr_id, "Expr {expr_id} has mismatched id field");
        check_expr_sub_ids(package, expr_id, &expr.kind);
    }
}

/// Checks that every child ID referenced by an expression kind exists in the
/// corresponding package map.
///
/// `check_id_references` delegates expression-specific validation here after it
/// has confirmed the top-level expression record itself is present.
///
/// # Panics
///
/// Panics if any sub-expression or block ID referenced by `kind` is missing.
fn check_expr_sub_ids(package: &Package, parent_expr: ExprId, kind: &ExprKind) {
    let assert_expr = |e: ExprId| {
        assert!(
            package.exprs.get(e).is_some(),
            "Expr {parent_expr} references nonexistent sub-Expr {e}"
        );
    };
    let assert_block = |b: BlockId| {
        assert!(
            package.blocks.get(b).is_some(),
            "Expr {parent_expr} references nonexistent Block {b}"
        );
    };

    match kind {
        ExprKind::Array(es) | ExprKind::ArrayLit(es) | ExprKind::Tuple(es) => {
            for &e in es {
                assert_expr(e);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            assert_expr(*a);
            assert_expr(*b);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            assert_expr(*a);
            assert_expr(*b);
            assert_expr(*c);
        }
        ExprKind::Block(block_id) => assert_block(*block_id),
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            assert_expr(*e);
        }
        ExprKind::If(cond, body, otherwise) => {
            assert_expr(*cond);
            assert_expr(*body);
            if let Some(e) = otherwise {
                assert_expr(*e);
            }
        }
        ExprKind::Range(s, st, e) => {
            if let Some(x) = s {
                assert_expr(*x);
            }
            if let Some(x) = st {
                assert_expr(*x);
            }
            if let Some(x) = e {
                assert_expr(*x);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                assert_expr(*c);
            }
            for fa in fields {
                assert_expr(fa.value);
            }
        }
        ExprKind::String(components) => {
            for c in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = c {
                    assert_expr(*e);
                }
            }
        }
        ExprKind::While(cond, block) => {
            assert_expr(*cond);
            assert_block(*block);
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Applies stage-gated callable checks to each reachable callable in the
/// target package.
///
/// Depending on `level`, this dispatcher invokes:
/// - `check_type_invariants` on callable output types.
/// - `check_no_arrow_params` once defunctionalization should have removed
///   callable-valued parameters. Pinned items are excluded from this check
///   because they are specialization targets that intentionally retain
///   arrow-typed parameters for callable-args codegen.
/// - `check_callable_input_pattern_shapes` once SROA and argument promotion may
///   have synthesized tuple-shaped inputs.
/// - `check_no_returns` once return unification should have removed
///   `ExprKind::Return`.
/// - `check_spec_decl_types` on the body and explicit specializations.
/// - `check_local_var_consistency` to ensure every local reference is still
///   backed by a binder.
/// - `check_spec_exec_graph` once exec graphs have been rebuilt at `PostAll`.
fn check_reachable_invariants(
    store: &PackageStore,
    target_package_id: qsc_fir::fir::PackageId,
    reachable: &FxHashSet<StoreItemId>,
    level: InvariantLevel,
) {
    for item_id in reachable {
        // Only check invariants on items in the target package. Cross-package
        // items (e.g. stdlib) are not transformed by the surrounding stages
        // and may still contain Ty::Param, Arrow types, or closures. Their
        // package-wide UDT-erasure invariants are checked separately.
        if item_id.package != target_package_id {
            continue;
        }
        let item_pkg = store.get(item_id.package);
        let item = item_pkg.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            // All reachable callables have been through the full pipeline
            // via the entry expression and should pass all stage-specific
            // invariant checks.
            check_type_invariants(&decl.output, level, "callable output type");

            if level.is_post_defunc_or_later() {
                check_no_arrow_params(item_pkg, decl);
            }

            if level.is_post_arg_promote_or_later() {
                check_callable_input_pattern_shapes(item_pkg, decl);
            }

            if level.is_post_return_unify_or_later() {
                check_no_returns(item_pkg, decl);
            }

            match &decl.implementation {
                CallableImpl::Spec(spec_impl) => {
                    check_spec_decl_types(store, item_pkg, &spec_impl.body, level);
                    for spec in functored_specs(spec_impl) {
                        check_spec_decl_types(store, item_pkg, spec, level);
                    }
                }
                CallableImpl::SimulatableIntrinsic(spec) => {
                    check_spec_decl_types(store, item_pkg, spec, level);
                }
                CallableImpl::Intrinsic => {}
            }

            if level.is_post_mono_or_later() {
                check_local_var_consistency(item_pkg, decl);
            }

            // After all passes, validate exec graph structural integrity.
            if level == InvariantLevel::PostAll {
                let name = &decl.name.name;
                match &decl.implementation {
                    CallableImpl::Spec(spec_impl) => {
                        check_spec_exec_graph(item_pkg, &spec_impl.body, &format!("{name}/body"));
                        for (label, spec) in [
                            ("adj", &spec_impl.adj),
                            ("ctl", &spec_impl.ctl),
                            ("ctl_adj", &spec_impl.ctl_adj),
                        ] {
                            if let Some(s) = spec {
                                check_spec_exec_graph(item_pkg, s, &format!("{name}/{label}"));
                            }
                        }
                    }
                    CallableImpl::SimulatableIntrinsic(spec) => {
                        check_spec_exec_graph(item_pkg, spec, &format!("{name}/sim_intrinsic"));
                    }
                    CallableImpl::Intrinsic => {}
                }
            }
        }
    }
}

/// Validates that callable input patterns no longer expose arrow-typed leaves.
///
/// The actual recursion lives in `check_pat_for_arrow` so tuple-shaped inputs
/// are checked all the way down to their leaves.
fn check_no_arrow_params(package: &Package, callable: &qsc_fir::fir::CallableDecl) {
    check_pat_for_arrow(package, callable.input);
}

/// Verifies that no `ExprKind::Return` nodes remain in a callable's body.
///
/// # Panics
///
/// Panics if any return expression is found.
fn check_no_returns(package: &Package, decl: &CallableDecl) {
    crate::walk_utils::for_each_expr_in_callable_impl(
        package,
        &decl.implementation,
        &mut |_expr_id, expr| {
            assert!(
                !matches!(expr.kind, ExprKind::Return(_)),
                "PostReturnUnify invariant violation: ExprKind::Return found after return unification pass in callable '{}'",
                decl.name.name
            );
        },
    );
}

/// Recursively validates that a pattern tree contains no arrow-typed leaves.
///
/// This helper is used by `check_no_arrow_params` so tuple-shaped callable
/// inputs are checked all the way down to their bound and discard leaves.
///
/// # Panics
///
/// Panics if any bound or discarded leaf still carries `Ty::Arrow`.
fn check_pat_for_arrow(package: &Package, pat_id: PatId) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Tuple(pats) => {
            for &sub_pat_id in pats {
                check_pat_for_arrow(package, sub_pat_id);
            }
        }
        PatKind::Bind(_) => {
            assert!(
                !matches!(pat.ty, Ty::Arrow(_)),
                "PostDefunc invariant violation: Arrow-typed parameter remains in callable input (Pat {pat_id})"
            );
        }
        PatKind::Discard => {
            assert!(
                !matches!(pat.ty, Ty::Arrow(_)),
                "PostDefunc invariant violation: Arrow-typed discard parameter in callable input (Pat {pat_id})"
            );
        }
    }
}

/// Validates the tuple-pattern shape of a callable's primary input pattern and
/// any specialization-specific input patterns.
///
/// This check becomes relevant after tuple-decomposing stages such as SROA and
/// argument promotion, which may synthesize tuple-shaped inputs that must still
/// mirror the callable input types exactly.
///
/// # Panics
///
/// Panics if any callable or specialization input pattern has tuple structure
/// that does not match its declared type.
fn check_callable_input_pattern_shapes(package: &Package, decl: &CallableDecl) {
    let callable_name = decl.name.name.to_string();
    check_tuple_pat_shape_matches_type(
        package,
        decl.input,
        &format!("callable '{callable_name}' input"),
    );

    match &decl.implementation {
        CallableImpl::Spec(spec_impl) => {
            for (label, input_pat) in [
                ("body", spec_impl.body.input),
                ("adj", spec_impl.adj.as_ref().and_then(|spec| spec.input)),
                ("ctl", spec_impl.ctl.as_ref().and_then(|spec| spec.input)),
                (
                    "ctl_adj",
                    spec_impl.ctl_adj.as_ref().and_then(|spec| spec.input),
                ),
            ] {
                if let Some(pat_id) = input_pat {
                    check_tuple_pat_shape_matches_type(
                        package,
                        pat_id,
                        &format!("callable '{callable_name}' {label} input"),
                    );
                }
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            if let Some(pat_id) = spec.input {
                check_tuple_pat_shape_matches_type(
                    package,
                    pat_id,
                    &format!("callable '{callable_name}' simulatable intrinsic input"),
                );
            }
        }
        CallableImpl::Intrinsic => {}
    }
}

/// Validates the tuple-pattern shape of `pat_id` against its declared type.
///
/// Recurses into `PatKind::Tuple` and requires the pattern arity to match the
/// `Ty::Tuple` element count exactly; each sub-pattern's type must equal the
/// corresponding tuple element type. `PatKind::Bind` and `PatKind::Discard`
/// are accepted unconditionally. `context` appears in panic messages to
/// disambiguate the calling site.
fn check_tuple_pat_shape_matches_type(package: &Package, pat_id: PatId, context: &str) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Tuple(pats) => {
            let Ty::Tuple(elem_tys) = &pat.ty else {
                panic!(
                    "Tuple pattern/type invariant violation: {context} Pat {pat_id} is tuple-shaped but has non-tuple type {:?}",
                    pat.ty,
                );
            };

            assert!(
                pats.len() == elem_tys.len(),
                "Tuple pattern/type invariant violation: {context} Pat {pat_id} has {} tuple elements but type has {} elements",
                pats.len(),
                elem_tys.len(),
            );

            for (index, (&sub_pat_id, elem_ty)) in pats.iter().zip(elem_tys.iter()).enumerate() {
                let sub_pat_ty = &package.get_pat(sub_pat_id).ty;
                assert!(
                    sub_pat_ty == elem_ty,
                    "Tuple pattern/type invariant violation: {context} Pat {pat_id} element {index} Pat {sub_pat_id} has type {sub_pat_ty:?} but tuple type expects {elem_ty:?}",
                );
                check_tuple_pat_shape_matches_type(package, sub_pat_id, context);
            }
        }
        PatKind::Bind(_) | PatKind::Discard => {}
    }
}

/// Asserts that no tuple-bound local leaf retains an arrow-typed field.
///
/// Recurses into `PatKind::Tuple` to reach every `Bind`/`Discard` leaf, then
/// delegates to `tuple_type_contains_arrow` on the leaf's declared type.
fn check_local_pat_for_nested_tuple_arrow(package: &Package, pat_id: PatId) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Tuple(pats) => {
            for &sub_pat_id in pats {
                check_local_pat_for_nested_tuple_arrow(package, sub_pat_id);
            }
        }
        PatKind::Bind(_) | PatKind::Discard => {
            assert!(
                !tuple_type_contains_arrow(&pat.ty),
                "PostDefunc invariant violation: tuple-bound local retains an arrow-typed field (Pat {pat_id})"
            );
        }
    }
}

/// Returns `true` when a `Ty::Tuple` contains any arrow-typed field,
/// transitively through nested tuples. Non-tuple types yield `false`.
fn tuple_type_contains_arrow(ty: &Ty) -> bool {
    match ty {
        Ty::Tuple(items) => items.iter().any(tuple_field_type_contains_arrow),
        _ => false,
    }
}

/// Returns `true` when a tuple field type is itself an arrow or a tuple that
/// transitively contains one. Used by `tuple_type_contains_arrow` to walk
/// into nested tuple fields.
fn tuple_field_type_contains_arrow(ty: &Ty) -> bool {
    match ty {
        Ty::Arrow(_) => true,
        Ty::Tuple(items) => items.iter().any(tuple_field_type_contains_arrow),
        _ => false,
    }
}

/// Drives the statement walk for a single specialization body by forwarding
/// each statement to `check_stmt_types`.
fn check_spec_decl_types(
    store: &PackageStore,
    package: &Package,
    spec: &qsc_fir::fir::SpecDecl,
    level: InvariantLevel,
) {
    let block = package.get_block(spec.block);
    for &stmt_id in &block.stmts {
        check_stmt_types(store, package, stmt_id, level);
    }
}

/// Applies the statement-local checks for a specialization block.
///
/// For each local binding, this layers:
/// - `check_pat_types` on the bound pattern type.
/// - `check_tuple_pat_shape_matches_type` after tuple-decomposing stages.
/// - `check_local_pat_for_nested_tuple_arrow` after SROA (arrow types may
///   appear inside tuples between UDT erasure and SROA).
/// - `check_expr_types` on the initializer expression.
/// - a final initializer-type equality assertion at `PostAll`.
///
/// Standalone expression statements are delegated directly to
/// `check_expr_types`.
fn check_stmt_types(
    store: &PackageStore,
    package: &Package,
    stmt_id: qsc_fir::fir::StmtId,
    level: InvariantLevel,
) {
    let stmt = package.get_stmt(stmt_id);
    match &stmt.kind {
        StmtKind::Expr(e) | StmtKind::Semi(e) => check_expr_types(store, package, *e, level),
        StmtKind::Local(_, pat, expr) => {
            check_pat_types(package, *pat, level);
            if level.is_post_sroa_or_later() {
                check_tuple_pat_shape_matches_type(package, *pat, "local binding");
                check_local_pat_for_nested_tuple_arrow(package, *pat);
            }
            check_expr_types(store, package, *expr, level);

            if level == InvariantLevel::PostReturnUnify || level == InvariantLevel::PostAll {
                let pat_ty = &package.get_pat(*pat).ty;
                let init_ty = &package.get_expr(*expr).ty;
                // Ty::Infer and Ty::Err should never appear at PostAll — all
                // passes must have resolved these types by then. At
                // PostReturnUnify, later passes may still need to resolve
                // them, so skip the type-equality check for those types.
                let has_unresolved = matches!(pat_ty, Ty::Err | Ty::Infer(_))
                    || matches!(init_ty, Ty::Err | Ty::Infer(_));
                if !has_unresolved || level == InvariantLevel::PostAll {
                    assert!(
                        pat_ty == init_ty,
                        "PostReturnUnify invariant violation: local binding Pat {pat} has type \
                         {pat_ty:?} but initializer Expr {expr} has type {init_ty:?}",
                    );
                }
            }
        }
        StmtKind::Item(_) => {}
    }
}

/// Walks the full subtree rooted at `expr_id` and forwards every visited node
/// to `check_expr_type`.
fn check_expr_types(
    store: &PackageStore,
    package: &Package,
    expr_id: ExprId,
    level: InvariantLevel,
) {
    crate::walk_utils::for_each_expr(package, expr_id, &mut |expr_id, _expr| {
        check_expr_type(store, package, expr_id, level);
    });
}

/// Applies node-local expression invariants.
///
/// This always starts with `check_type_invariants` on the expression's own
/// type and then layers stage-specific structural checks on the expression
/// kind itself.
///
/// The `PostUdtErase`-era expression-kind assertions here (for
/// [`ExprKind::Struct`], [`Field::Path`] in `UpdateField`/`AssignField`, and
/// [`Field::Path`] on non-tuple records) intentionally overlap with
/// `check_package_udt_erase_invariants`: this walker fires on every
/// reachable expression in the target package, while the package-wide walker
/// visits every expression in every reachable package. Both paths must agree
/// so a regression caught in either scope produces the same diagnostic.
fn check_expr_type(
    store: &PackageStore,
    package: &Package,
    expr_id: ExprId,
    level: InvariantLevel,
) {
    let expr = package.get_expr(expr_id);
    check_type_invariants(&expr.ty, level, &format!("Expr {expr_id}"));

    // After defunctionalization, no closures should remain in reachable code.
    if level.is_post_defunc_or_later() {
        assert!(
            !matches!(&expr.kind, ExprKind::Closure(_, _)),
            "Expr {expr_id} is a Closure after defunctionalization"
        );
    }

    // PostMono: no remaining generic args on Var references.
    if level.is_post_mono_or_later()
        && let ExprKind::Var(_, args) = &expr.kind
    {
        assert!(
            args.is_empty(),
            "PostMono invariant violation: Expr {expr_id} still has non-empty generic args"
        );
    }

    // After UDT erasure, all Struct expressions must have been lowered.
    if level.is_post_udt_erase_or_later() {
        if matches!(&expr.kind, ExprKind::Struct(_, _, _)) {
            panic!(
                "PostUdtErase invariant violation: Expr {expr_id} contains \
                 ExprKind::Struct after UDT erasure"
            );
        }

        // Field::Path references UDT field paths that must be lowered by udt_erase.
        if let ExprKind::UpdateField(_, Field::Path(_), _)
        | ExprKind::AssignField(_, Field::Path(_), _) = &expr.kind
        {
            panic!(
                "PostUdtErase invariant violation: Expr {expr_id} contains \
                 Field::Path in UpdateField/AssignField after UDT erasure"
            );
        }

        // After UDT erasure, every Field::Path target must be a Tuple.
        if let ExprKind::Field(record_id, Field::Path(_)) = &expr.kind {
            let record = package.get_expr(*record_id);
            assert!(
                matches!(&record.ty, Ty::Tuple(_)),
                "PostUdtErase invariant violation: Expr {expr_id} has Field::Path \
                 on non-tuple record Expr {record_id} (type: {:?})",
                record.ty,
            );
        }
    }

    // After tuple comparison lowering, no BinOp(Eq/Neq) on non-empty tuple operands.
    if level.is_post_tuple_comp_lower_or_later()
        && let ExprKind::BinOp(BinOp::Eq | BinOp::Neq, lhs_id, _) = &expr.kind
    {
        let lhs_ty = &package.get_expr(*lhs_id).ty;
        if let Ty::Tuple(elems) = lhs_ty {
            assert!(
                elems.is_empty(),
                "PostTupleCompLower invariant violation: Expr {expr_id} has \
                     BinOp(Eq/Neq) on tuple-typed operands"
            );
        }
    }

    // After defunctionalization, tuple expressions must have types with matching arity.
    if level.is_post_defunc_or_later()
        && let ExprKind::Tuple(es) = &expr.kind
        && let Ty::Tuple(tys) = &expr.ty
    {
        assert!(
            es.len() == tys.len(),
            "Tuple arity mismatch: Expr {expr_id} has {} elements but type has {} elements",
            es.len(),
            tys.len()
        );
    }

    if level.is_post_arg_promote_or_later()
        && let ExprKind::Call(callee_id, arg_id) = &expr.kind
    {
        check_call_shape_matches_callee(store, package, expr_id, *callee_id, *arg_id);
    }
}

/// Verifies that a `ExprKind::Call` expression's argument type matches the
/// callee's declared input type and that the call's result type matches the
/// callee's declared output type.
///
/// This is the post-`arg_promote` check that catches signature drift
/// introduced by tuple-decomposing stages.
fn check_call_shape_matches_callee(
    store: &PackageStore,
    package: &Package,
    call_expr_id: ExprId,
    callee_id: ExprId,
    arg_id: ExprId,
) {
    let arg = package.get_expr(arg_id);

    let Some((expected_input, expected_output)) = resolve_call_signature(store, package, callee_id)
    else {
        let callee = package.get_expr(callee_id);
        panic!(
            "PostArgPromote/PostAll call invariant violation: Expr {call_expr_id} calls Expr \
             {callee_id} whose signature cannot be resolved from callee type {:?}",
            callee.ty,
        );
    };

    assert!(
        arg.ty == expected_input,
        "PostArgPromote/PostAll call invariant violation: Expr {call_expr_id} passes Expr \
         {arg_id} with type {:?} to callee Expr {callee_id} expecting input type \
         {expected_input:?}",
        arg.ty,
    );

    let call = package.get_expr(call_expr_id);
    assert!(
        call.ty == expected_output,
        "PostArgPromote/PostAll call invariant violation: Expr {call_expr_id} has type {:?} \
         but callee Expr {callee_id} returns {expected_output:?}",
        call.ty,
    );
}

/// Resolves a callee expression to its `(input_ty, output_ty)` signature.
///
/// Handles the two callee forms that can appear after the pipeline runs: a
/// direct `Ty::Arrow`-typed expression (e.g., a captured callable value), and
/// an `ExprKind::Var(Res::Item, _)` pointing at a `Callable` item in any
/// package. Returns `None` when the callee is neither form; callers treat
/// `None` as an invariant violation.
fn resolve_call_signature(
    store: &PackageStore,
    package: &Package,
    callee_id: ExprId,
) -> Option<(Ty, Ty)> {
    let callee = package.get_expr(callee_id);
    if let Ty::Arrow(arrow) = &callee.ty {
        return Some(((*arrow.input).clone(), (*arrow.output).clone()));
    }

    if let ExprKind::Var(Res::Item(item_id), _) = &callee.kind {
        let callee_package = store.get(item_id.package);
        let item = callee_package.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            let input_ty = callee_package.get_pat(decl.input).ty.clone();
            return Some((input_ty, decl.output.clone()));
        }
    }

    None
}

/// Validates a pattern's declared type by delegating to
/// `check_type_invariants`.
fn check_pat_types(package: &Package, pat_id: PatId, level: InvariantLevel) {
    let pat = package.get_pat(pat_id);
    check_type_invariants(&pat.ty, level, &format!("Pat {pat_id}"));
}

/// Recursively validates the stage-sensitive invariants for a type.
///
/// This is the common type checker used by callable signatures, patterns, and
/// expressions. It enforces the type-form restrictions guaranteed by each
/// pipeline stage while walking into nested array, tuple, and arrow types.
///
/// # Panics
///
/// Panics when a type still contains a form that should have been eliminated by
/// the current invariant level, such as `Ty::Param`, `FunctorSet::Param`, or
/// `Ty::Udt`.
fn check_type_invariants(ty: &Ty, level: InvariantLevel, context: &str) {
    match ty {
        Ty::Param(_) => {
            assert!(
                !level.is_post_mono_or_later(),
                "{context} contains Ty::Param after monomorphization"
            );
        }
        Ty::Arrow(arrow) => {
            if level.is_post_mono_or_later() {
                assert!(
                    !matches!(arrow.functors, FunctorSet::Param(_)),
                    "{context} contains FunctorSet::Param after monomorphization"
                );
            }
            if level.is_post_defunc_or_later() {
                // `Ty::Arrow` leaves are allowed on callable outputs and
                // cross-package items; the `PostDefunc` invariant targets
                // arrow-typed callable *parameters*, enforced by
                // `check_no_arrow_params`.
            }
            check_type_invariants(&arrow.input, level, context);
            check_type_invariants(&arrow.output, level, context);
        }
        Ty::Array(inner) => check_type_invariants(inner, level, context),
        Ty::Tuple(items) => {
            for item in items {
                check_type_invariants(item, level, context);
            }
        }
        Ty::Udt(_) => {
            assert!(
                !level.is_post_udt_erase_or_later(),
                "{context} contains Ty::Udt after UDT erasure"
            );
        }
        Ty::Infer(_) | Ty::Err => {
            assert!(
                level != InvariantLevel::PostAll,
                "{context} contains unexpected Ty::Infer/Ty::Err — indicates a pass bug"
            );
        }
        Ty::Prim(_) => {}
    }
}

/// Verifies that every `Res::Local(id)` in a callable body refers to a
/// `LocalVarId` that is bound by:
/// - the callable's input pattern,
/// - a specialization input pattern, or
/// - a `PatKind::Bind` in a body-internal `StmtKind::Local`.
///
/// # Panics
///
/// Panics if a local reference is found that is not in the bound set.
fn check_local_var_consistency(package: &Package, decl: &CallableDecl) {
    let mut bound: FxHashSet<LocalVarId> = FxHashSet::default();
    let mut refs: Vec<(ExprId, LocalVarId)> = Vec::new();

    // Collect bindings from the callable's input pattern.
    collect_pat_bindings(package, decl.input, &mut bound);

    match &decl.implementation {
        CallableImpl::Spec(spec_impl) => {
            for spec in std::iter::once(&spec_impl.body)
                .chain(spec_impl.adj.iter())
                .chain(spec_impl.ctl.iter())
                .chain(spec_impl.ctl_adj.iter())
            {
                if let Some(input_pat) = spec.input {
                    collect_pat_bindings(package, input_pat, &mut bound);
                }
                walk_block_for_locals(package, spec.block, &mut bound, &mut refs);
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            if let Some(input_pat) = spec.input {
                collect_pat_bindings(package, input_pat, &mut bound);
            }
            walk_block_for_locals(package, spec.block, &mut bound, &mut refs);
        }
        CallableImpl::Intrinsic => {}
    }

    // Assert every referenced local is bound.
    for (expr_id, var_id) in &refs {
        assert!(
            bound.contains(var_id),
            "LocalVarId consistency: Expr {expr_id} references {var_id}, \
             which is not bound in callable \"{}\"",
            decl.name.name,
        );
    }
}

/// Recursively collects all `LocalVarId`s from `PatKind::Bind` nodes.
fn collect_pat_bindings(package: &Package, pat_id: PatId, bound: &mut FxHashSet<LocalVarId>) {
    let pat = package.get_pat(pat_id);
    match &pat.kind {
        PatKind::Bind(ident) => {
            bound.insert(ident.id);
        }
        PatKind::Discard => {}
        PatKind::Tuple(pats) => {
            for &sub in pats {
                collect_pat_bindings(package, sub, bound);
            }
        }
    }
}

/// Walks a block, collecting both bindings and local references.
fn walk_block_for_locals(
    package: &Package,
    block_id: BlockId,
    bound: &mut FxHashSet<LocalVarId>,
    refs: &mut Vec<(ExprId, LocalVarId)>,
) {
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) => {
                walk_expr_for_locals(package, *e, bound, refs);
            }
            StmtKind::Local(_, pat, expr) => {
                collect_pat_bindings(package, *pat, bound);
                walk_expr_for_locals(package, *expr, bound, refs);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Walks an expression tree, recording `Res::Local` references and recursing
/// into sub-expressions and nested blocks.
fn walk_expr_for_locals(
    package: &Package,
    expr_id: ExprId,
    bound: &mut FxHashSet<LocalVarId>,
    refs: &mut Vec<(ExprId, LocalVarId)>,
) {
    let expr = package.get_expr(expr_id);

    // Record local references.
    match &expr.kind {
        ExprKind::Var(Res::Local(id), _) => refs.push((expr_id, *id)),
        ExprKind::Closure(ids, _) => {
            for id in ids {
                refs.push((expr_id, *id));
            }
        }
        _ => {}
    }

    // Recurse into sub-expressions and sub-blocks.
    match &expr.kind {
        ExprKind::Array(es) | ExprKind::ArrayLit(es) | ExprKind::Tuple(es) => {
            for &e in es {
                walk_expr_for_locals(package, e, bound, refs);
            }
        }
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            walk_expr_for_locals(package, *a, bound, refs);
            walk_expr_for_locals(package, *b, bound, refs);
        }
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            walk_expr_for_locals(package, *a, bound, refs);
            walk_expr_for_locals(package, *b, bound, refs);
            walk_expr_for_locals(package, *c, bound, refs);
        }
        ExprKind::Block(block_id) => walk_block_for_locals(package, *block_id, bound, refs),
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::Return(e) | ExprKind::UnOp(_, e) => {
            walk_expr_for_locals(package, *e, bound, refs);
        }
        ExprKind::If(cond, body, otherwise) => {
            walk_expr_for_locals(package, *cond, bound, refs);
            walk_expr_for_locals(package, *body, bound, refs);
            if let Some(e) = otherwise {
                walk_expr_for_locals(package, *e, bound, refs);
            }
        }
        ExprKind::Range(s, st, e) => {
            if let Some(x) = s {
                walk_expr_for_locals(package, *x, bound, refs);
            }
            if let Some(x) = st {
                walk_expr_for_locals(package, *x, bound, refs);
            }
            if let Some(x) = e {
                walk_expr_for_locals(package, *x, bound, refs);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(c) = copy {
                walk_expr_for_locals(package, *c, bound, refs);
            }
            for fa in fields {
                walk_expr_for_locals(package, fa.value, bound, refs);
            }
        }
        ExprKind::String(components) => {
            for c in components {
                if let qsc_fir::fir::StringComponent::Expr(e) = c {
                    walk_expr_for_locals(package, *e, bound, refs);
                }
            }
        }
        ExprKind::While(cond, block) => {
            walk_expr_for_locals(package, *cond, bound, refs);
            walk_block_for_locals(package, *block, bound, refs);
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Validates structural integrity of a single configured exec graph.
///
/// # Panics
///
/// Panics with a descriptive message if any invariant is violated.
fn check_configured_exec_graph(
    package: &Package,
    nodes: &[ExecGraphNode],
    context: &str,
    config_label: &str,
) {
    let len = nodes.len();
    assert!(
        len > 0,
        "Exec graph for {context} ({config_label}) is empty"
    );

    // Invariant E: graph terminates correctly.
    match config_label {
        "no_debug" => assert!(
            matches!(nodes[len - 1], ExecGraphNode::Ret),
            "Exec graph for {context} ({config_label}) does not end with Ret, found {:?}",
            nodes[len - 1],
        ),
        "debug" => assert!(
            matches!(
                nodes[len - 1],
                ExecGraphNode::Debug(ExecGraphDebugNode::RetFrame)
            ),
            "Exec graph for {context} ({config_label}) does not end with RetFrame, found {:?}",
            nodes[len - 1],
        ),
        _ => {}
    }

    for (i, node) in nodes.iter().enumerate() {
        match node {
            // Invariant A: jump targets within bounds.
            ExecGraphNode::Jump(idx)
            | ExecGraphNode::JumpIf(idx)
            | ExecGraphNode::JumpIfNot(idx) => {
                assert!(
                    (*idx as usize) < len,
                    "Exec graph for {context} ({config_label}): node {i} has jump target {idx} >= len {len}"
                );
            }
            // Invariant B: Expr references valid ExprId.
            ExecGraphNode::Expr(expr_id) => {
                assert!(
                    package.exprs.get(*expr_id).is_some(),
                    "Exec graph for {context} ({config_label}): node {i} references nonexistent Expr {expr_id}"
                );
            }
            // Invariant C: Bind references valid PatId.
            ExecGraphNode::Bind(pat_id) => {
                assert!(
                    package.pats.get(*pat_id).is_some(),
                    "Exec graph for {context} ({config_label}): node {i} references nonexistent Pat {pat_id}"
                );
            }
            // Invariant D: debug node ID references are valid.
            ExecGraphNode::Debug(debug_node) => match debug_node {
                ExecGraphDebugNode::Stmt(stmt_id) => {
                    assert!(
                        package.stmts.get(*stmt_id).is_some(),
                        "Exec graph for {context} ({config_label}): node {i} references nonexistent Stmt {stmt_id}"
                    );
                }
                ExecGraphDebugNode::PushLoopScope(expr_id) => {
                    assert!(
                        package.exprs.get(*expr_id).is_some(),
                        "Exec graph for {context} ({config_label}): node {i} PushLoopScope references nonexistent Expr {expr_id}"
                    );
                }
                ExecGraphDebugNode::BlockEnd(block_id) => {
                    assert!(
                        package.blocks.get(*block_id).is_some(),
                        "Exec graph for {context} ({config_label}): node {i} BlockEnd references nonexistent Block {block_id}"
                    );
                }
                ExecGraphDebugNode::PushScope
                | ExecGraphDebugNode::PopScope
                | ExecGraphDebugNode::RetFrame
                | ExecGraphDebugNode::LoopIteration => {}
            },
            ExecGraphNode::Store | ExecGraphNode::Unit | ExecGraphNode::Ret => {}
        }
    }
}

/// Validates both configurations of a spec's exec graph.
///
/// This fans out to `check_configured_exec_graph` for the compact and debug
/// views so both serialized forms are kept structurally consistent.
fn check_spec_exec_graph(package: &Package, spec: &SpecDecl, context: &str) {
    for (config, label) in [
        (ExecGraphConfig::NoDebug, "no_debug"),
        (ExecGraphConfig::Debug, "debug"),
    ] {
        let nodes = spec.exec_graph.select_ref(config);
        check_configured_exec_graph(package, nodes, context, label);
    }
}

/// Verifies two ownership properties of `ExprId`s after defunctionalization:
///
/// 1. **Per-spec uniqueness**: No `ExprId` appears in more than one
///    specialization body across all reachable callables.
/// 2. **Entry-vs-spec disjointness**: `ExprId`s reachable from the entry
///    expression are disjoint from those inside any specialization body.
///
/// These properties ensure that RCA can assign per-arity `ComputeKind`
/// entries without collision. Defunctionalization's closure cleanup pass
/// is the primary mechanism that establishes property (2) for producer
/// function bodies that originally contained closure nodes.
///
/// # Panics
///
/// Panics with a descriptive message if any `ExprId` is shared.
fn check_expr_id_ownership(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &FxHashSet<StoreItemId>,
    entry_id: ExprId,
) {
    let package = store.get(package_id);

    // Map each ExprId to the (item, spec_label) that owns it.
    let mut seen: FxHashMap<ExprId, (LocalItemId, &'static str)> = FxHashMap::default();

    for item_id in reachable {
        if item_id.package != package_id {
            continue;
        }
        let item = package.get_item(item_id.item);
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };

        let specs: Vec<(&SpecDecl, &'static str)> = match &decl.implementation {
            CallableImpl::Spec(spec_impl) => {
                let mut v = vec![(&spec_impl.body, "body")];
                if let Some(adj) = &spec_impl.adj {
                    v.push((adj, "adj"));
                }
                if let Some(ctl) = &spec_impl.ctl {
                    v.push((ctl, "ctl"));
                }
                if let Some(cta) = &spec_impl.ctl_adj {
                    v.push((cta, "ctl_adj"));
                }
                v
            }
            CallableImpl::SimulatableIntrinsic(spec) => {
                vec![(spec, "sim")]
            }
            CallableImpl::Intrinsic => continue,
        };

        for (spec, label) in specs {
            let mut expr_ids = FxHashSet::default();
            collect_expr_ids_in_block(package, spec.block, &mut expr_ids);
            for eid in &expr_ids {
                if let Some((prev_item, prev_label)) = seen.get(eid) {
                    panic!(
                        "PostDefunc ExprId uniqueness violation: {eid} appears in \
                         both {prev_item}/{prev_label} and {}/{label}",
                        item_id.item,
                    );
                }
                seen.insert(*eid, (item_id.item, label));
            }
        }
    }

    // Check entry expression ExprIds are disjoint from spec body ExprIds.
    let mut entry_expr_ids = FxHashSet::default();
    collect_expr_ids_in_expr(package, entry_id, &mut entry_expr_ids);
    for eid in &entry_expr_ids {
        if let Some((owner_item, owner_label)) = seen.get(eid) {
            panic!(
                "PostDefunc entry/spec disjointness violation: {eid} appears in \
                 both the entry expression and {owner_item}/{owner_label}",
            );
        }
    }
}

/// Recursively collects all `ExprId`s reachable from a block.
fn collect_expr_ids_in_block(package: &Package, block_id: BlockId, ids: &mut FxHashSet<ExprId>) {
    let block = package.get_block(block_id);
    for &stmt_id in &block.stmts {
        let stmt = package.get_stmt(stmt_id);
        match &stmt.kind {
            StmtKind::Expr(e) | StmtKind::Semi(e) | StmtKind::Local(_, _, e) => {
                collect_expr_ids_in_expr(package, *e, ids);
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Recursively collects all `ExprId`s reachable from an expression.
fn collect_expr_ids_in_expr(package: &Package, expr_id: ExprId, ids: &mut FxHashSet<ExprId>) {
    ids.insert(expr_id);
    crate::walk_utils::for_each_expr(package, expr_id, &mut |child_id, _| {
        ids.insert(child_id);
    });
}
