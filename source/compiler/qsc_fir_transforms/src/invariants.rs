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
//! | `PostTupleDecompose` | tuple-decompose — tuple decomposition patterns match types. |
//! | `PostArgPromote` | Argument promotion — input patterns match types. |
//! | `PostItemDce` | Item DCE — no orphaned live-tree references after item pruning. |
//! | `PostAll` | All passes — full structural + type checks. |
//!
//! # Two entry points
//!
//! - [`check`] runs the staged invariant set on the target package's
//!   entry-rooted reachability closure. At [`InvariantLevel::PostUdtErase`]
//!   and later it additionally walks the reachable-package closure to apply
//!   the package-wide UDT-erase invariants to every reachable external
//!   package.
//! - The reachable-spec exec-graph surface (structural well-formedness and
//!   non-empty ranges) is validated for every reachable spec in every
//!   reachable package at [`InvariantLevel::PostAll`], since
//!   `exec_graph_rebuild` now rebuilds the whole reachable closure.

#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_utils;

use crate::fir_builder::functored_specs;
use qsc_fir::fir::{
    BinOp, Block, BlockId, CallableDecl, CallableImpl, ExecGraphConfig, ExecGraphDebugNode,
    ExecGraphNode, Expr, ExprId, ExprKind, Field, Functor, ItemId, ItemKind, LocalItemId,
    LocalVarId, Package, PackageId, PackageLookup, PackageStore, Pat, PatId, PatKind, Res,
    SpecDecl, Stmt, StmtId, StmtKind, StoreItemId, StringComponent, UnOp,
};
use qsc_fir::ty::{FunctorSet, Prim, Ty};
use qsc_fir::visit::{self, Visitor};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::reachability::collect_reachable_with_seeds;
use crate::walk_utils::{CallableNode, for_each_node_from_expr_root, for_each_node_in_callable};

/// The level of invariant checking to perform, corresponding to which passes
/// have already been applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum InvariantLevel {
    /// After monomorphization: no `Ty::Param` in reachable code.
    PostMono,
    /// After return unification: additionally no `ExprKind::Return` in reachable code.
    PostReturnUnify,
    /// After the body-only signature-preserving sub-pipeline
    /// (`return_unify` → `tuple_compare_lower` → `tuple_decompose`, run on
    /// pinned `ReinvokeOriginal` target bodies).
    ///
    /// This level is **off the strict-linear pipeline axis**: it enforces only
    /// the checks those three passes establish — no `ExprKind::Return` and no
    /// tuple `BinOp(Eq/Neq)`, with matching tuple-decompose patterns. Because
    /// the pinned bodies are never monomorphized, defunctionalized, UDT-erased,
    /// or argument-promoted, it deliberately **allows** residual `Ty::Param` /
    /// `FunctorSet::Param`, `Ty::Arrow` parameters, `ExprKind::Closure`,
    /// `Ty::Udt`, `ExprKind::Struct`, and `Field::Path`. Membership is
    /// special-cased in `InvariantLevel::enforces`.
    PostSignaturePreserving,
    /// After defunctionalization: additionally no `Ty::Arrow` params and no
    /// `ExprKind::Closure` in reachable code.
    PostDefunc,
    /// After UDT erasure: additionally no `Ty::Udt`, no
    /// `ExprKind::Struct`, and no `Field::Path` in `UpdateField`/`AssignField`.
    PostUdtErase,
    /// After tuple comparison lowering: additionally no `BinOp(Eq/Neq)` on
    /// tuple-typed operands.
    PostTupleCompLower,
    /// After tuple-decompose: additionally synthesized local tuple patterns must match
    /// the tuple types they decompose.
    PostTupleDecompose,
    /// After argument promotion: additionally synthesized callable input tuple
    /// patterns must match the callable input types they decompose.
    PostArgPromote,
    /// After item DCE: live FIR tree references remain valid after item pruning.
    /// `StmtKind::Item` definitions may still point at removed items, because
    /// they are declarations rather than executable tree edges.
    PostItemDce,
    /// After all passes: every earlier-stage invariant plus the postconditions
    /// unique to this stage:
    ///
    /// - `Package.entry_exec_graph` is structurally well-formed in both
    ///   [`ExecGraphConfig::NoDebug`] and [`ExecGraphConfig::Debug`]
    ///   configurations, and every reachable callable specialization's
    ///   `exec_graph` is structurally well-formed in both configurations.
    /// - No `Ty::Infer` or `Ty::Err` survives in any checked type — their
    ///   presence at this stage indicates a pass bug.
    PostAll,
}

/// A pipeline-stage invariant check that an [`InvariantLevel`] may enforce.
///
/// Each variant names the pass whose postconditions the check verifies. The
/// strictly-linear `InvariantLevel` values enforce a check once the level is at
/// or after the pass's own stage, so the derived [`Ord`] decides membership.
/// [`InvariantLevel::PostSignaturePreserving`] is the sole off-axis level and
/// is special-cased in [`InvariantLevel::enforces`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum StageCheck {
    /// Monomorphization: no `Ty::Param` / `FunctorSet::Param`.
    Mono,
    /// Return unification: no `ExprKind::Return`.
    ReturnUnify,
    /// Defunctionalization: no `Ty::Arrow` params / `ExprKind::Closure`.
    Defunc,
    /// UDT erasure: no `Ty::Udt` / `ExprKind::Struct` / `Field::Path`.
    UdtErase,
    /// Tuple comparison lowering: no `BinOp(Eq/Neq)` on tuple operands.
    TupleCompLower,
    /// tuple-decompose: synthesized tuple patterns match their types.
    TupleDecompose,
    /// Argument promotion: call argument shapes match callee signatures.
    ArgPromote,
}

impl InvariantLevel {
    /// Returns `true` when this level enforces `check`.
    ///
    /// For the strictly-linear levels a check applies once the level reaches the
    /// check's own pipeline stage, so the answer comes straight from the derived
    /// [`Ord`]. [`PostSignaturePreserving`](Self::PostSignaturePreserving) is the
    /// one off-axis level: the body-only sub-pipeline runs only `return_unify`,
    /// `tuple_compare_lower`, and `tuple_decompose` on pinned `ReinvokeOriginal`
    /// bodies that were never monomorphized, defunctionalized, UDT-erased, or
    /// argument-promoted. It therefore enforces exactly those three checks and
    /// none of the others — in particular it must not enforce [`StageCheck::Mono`]
    /// (the un-monomorphized bodies legitimately retain `Ty::Param` /
    /// `FunctorSet::Param`).
    fn enforces(self, check: StageCheck) -> bool {
        if self == Self::PostSignaturePreserving {
            return matches!(
                check,
                StageCheck::ReturnUnify | StageCheck::TupleCompLower | StageCheck::TupleDecompose
            );
        }

        let stage = match check {
            StageCheck::Mono => Self::PostMono,
            StageCheck::ReturnUnify => Self::PostReturnUnify,
            StageCheck::Defunc => Self::PostDefunc,
            StageCheck::UdtErase => Self::PostUdtErase,
            StageCheck::TupleCompLower => Self::PostTupleCompLower,
            StageCheck::TupleDecompose => Self::PostTupleDecompose,
            StageCheck::ArgPromote => Self::PostArgPromote,
        };
        self >= stage
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
/// This entry point checks every reachable callable. The pipeline uses
/// `check_with_skip` to bypass the residual-`Return` checks on callables that
/// return unification deliberately left un-rewritten.
///
/// # Ordering
///
/// `check_id_references` must run on the target package *before*
/// `collect_reachable_with_seeds`. The reachability walker dereferences IDs
/// through [`qsc_fir::fir::PackageLookup`], which panics with a generic
/// `"Statement not found"` message on a malformed `block.stmts` list. Running
/// the ID-reference check first surfaces the targeted invariant diagnostic
/// (`Block {block_id} references nonexistent Stmt {stmt_id}`) instead of the
/// opaque lookup panic.
///
/// # Panics
///
/// Panics with a descriptive message if any invariant is violated.
pub fn check(store: &PackageStore, package_id: qsc_fir::fir::PackageId, level: InvariantLevel) {
    check_with_skip_and_seeds(store, package_id, level, &FxHashSet::default(), &[]);
}

/// Like [`check`], but bypasses exactly the post-return-unification checks a
/// residual `Return` can violate for the callables named in `skip`.
///
/// `skip` names callables that return unification deliberately left
/// un-rewritten (their bodies still contain a residual `Return`). Those
/// callables bypass only the absence-of-`Return` check, the single-exit
/// non-Unit block-tail check, and the operand-position flag-write check —
/// every other invariant still runs on them. The production pipeline passes
/// the set returned by return unification; all other callers use [`check`]
/// (an empty skip set), which checks every callable.
pub(crate) fn check_with_skip(
    store: &PackageStore,
    package_id: qsc_fir::fir::PackageId,
    level: InvariantLevel,
    skip: &FxHashSet<StoreItemId>,
) {
    check_with_skip_and_seeds(store, package_id, level, skip, &[]);
}

/// Seed-rooted variant of [`check`] for the body-only signature-preserving
/// sub-pipeline.
///
/// Reachability is rooted at the entry expression **and** the `seeds` roots
/// (pinned `ReinvokeOriginal` target bodies and their transitive callees), so
/// the [`InvariantLevel::PostSignaturePreserving`] checks cover the pinned
/// bodies even though they are not entry-reachable.
///
/// With empty `seeds` this is identical to [`check`].
#[cfg(test)]
pub(crate) fn check_with_seeds(
    store: &PackageStore,
    package_id: qsc_fir::fir::PackageId,
    level: InvariantLevel,
    seeds: &[StoreItemId],
) {
    check_with_skip_and_seeds(store, package_id, level, &FxHashSet::default(), seeds);
}

/// Shared implementation backing [`check`] and [`check_with_skip`], and the
/// seed-rooted body-only signature-preserving sub-pipeline.
///
/// `skip` bypasses the residual-`Return` checks for specific callables;
/// `seeds` extends reachability roots beyond the entry expression so non
/// entry-reachable pinned bodies (pinned `ReinvokeOriginal` target bodies and
/// their transitive callees) are still validated.
///
/// # Generic-target assumption
///
/// Pinned `ReinvokeOriginal` targets are concrete user callables, so
/// `PostSignaturePreserving` not enforcing [`StageCheck::Mono`] (no `Ty::Param`)
/// is safe. If a generic target ever reaches this check, the no-`Ty::Param`
/// invariant panics with a descriptive message, which serves as the assertion
/// that the assumption was violated.
pub(crate) fn check_with_skip_and_seeds(
    store: &PackageStore,
    package_id: qsc_fir::fir::PackageId,
    level: InvariantLevel,
    skip: &FxHashSet<StoreItemId>,
    seeds: &[StoreItemId],
) {
    let package = store.get(package_id);
    check_id_references(package);

    if package.entry.is_none() && seeds.is_empty() {
        return;
    }

    let reachable = collect_reachable_with_seeds(store, package_id, seeds);
    if level.enforces(StageCheck::UdtErase) {
        check_package_udt_erase_invariants_in_reachable_items(store, &reachable, package_id);
        check_id_references_in_reachable_items(store, &reachable, package_id);
    }

    check_reachable_invariants(store, &reachable, level, skip);

    // After all passes, `exec_graph_rebuild` rebuilds the exec graph of every
    // reachable spec in every reachable package. Validate that whole reachable
    // closure here for structural well-formedness and non-empty, in-bounds
    // ranges, regardless of which package owns each spec.
    if level == InvariantLevel::PostAll {
        check_reachable_spec_exec_graphs(store, &reachable);
    }

    if let Some(entry_id) = package.entry {
        if level.enforces(StageCheck::Defunc) {
            check_expr_id_ownership(store, package_id, &reachable, entry_id);
        }

        if level.enforces(StageCheck::ReturnUnify) {
            check_non_unit_block_tails(store, package_id, &reachable, skip);
            check_no_flag_writes_in_operand_position(store, &reachable, skip);
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
}

/// Validates the exec-graph surface of every reachable callable spec across the
/// entry-reachable package closure: structural well-formedness and non-empty,
/// in-bounds `exec_graph_range`s for each expression.
///
/// `exec_graph_rebuild` rebuilds the exec graph of every reachable spec in
/// every reachable package, so this validation spans all packages rather than
/// only the entry package. `ExprId`s are package-relative, so each spec is
/// validated against its own owning package.
fn check_reachable_spec_exec_graphs(store: &PackageStore, reachable: &FxHashSet<StoreItemId>) {
    for item_id in reachable {
        let package = store.get(item_id.package);
        let item = package.get_item(item_id.item);
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        let name = &decl.name.name;
        match &decl.implementation {
            CallableImpl::Spec(spec_impl) => {
                check_spec_exec_graph(package, &spec_impl.body, &format!("{name}/body"));
                check_spec_exec_graph_ranges(package, &spec_impl.body, &format!("{name}/body"));
                for (label, spec) in [
                    ("adj", &spec_impl.adj),
                    ("ctl", &spec_impl.ctl),
                    ("ctl_adj", &spec_impl.ctl_adj),
                ] {
                    if let Some(s) = spec {
                        check_spec_exec_graph(package, s, &format!("{name}/{label}"));
                        check_spec_exec_graph_ranges(package, s, &format!("{name}/{label}"));
                    }
                }
            }
            CallableImpl::SimulatableIntrinsic(spec) => {
                check_spec_exec_graph(package, spec, &format!("{name}/sim_intrinsic"));
                check_spec_exec_graph_ranges(package, spec, &format!("{name}/sim_intrinsic"));
            }
            CallableImpl::Intrinsic => {}
        }
    }
}

/// Reachable-item-scoped UDT-erase invariant checker.
///
/// UDT erasure rewrites every reachable callable across the package closure, so
/// every reachable callable is guaranteed erased; unreachable callables are
/// dead-code-eliminated and never emitted, so re-validating the full std/core
/// arenas only adds cost. This walks the reachable callables of every reachable
/// package, applying the same per-node UDT-erase assertions as the full-arena
/// scan over exactly the nodes that are actually emitted.
///
/// The target package's entry expression lives outside any callable body, so it
/// is walked as a separate root to preserve parity with the full-arena scan.
fn check_package_udt_erase_invariants_in_reachable_items(
    store: &PackageStore,
    reachable: &FxHashSet<StoreItemId>,
    target_package_id: qsc_fir::fir::PackageId,
) {
    let check_node = |pkg: &Package, node: CallableNode| match node {
        CallableNode::Expr(id) => check_expr_udt_erase_invariants(pkg, id),
        CallableNode::Pat(id) => {
            check_type_udt_erase_invariants(&pkg.get_pat(id).ty, &format!("Pat {id}"));
        }
        CallableNode::Block(id) => {
            check_type_udt_erase_invariants(&pkg.get_block(id).ty, &format!("Block {id}"));
        }
        CallableNode::Stmt(_) => {}
    };

    for item_id in reachable {
        let pkg = store.get(item_id.package);
        let item = pkg.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            check_type_udt_erase_invariants(&decl.output, &format!("Callable {item_id} output"));
            for_each_node_in_callable(pkg, decl, &mut |node| check_node(pkg, node));
        }
    }

    // The target package's entry expression lives outside any callable body, so
    // the reachable-callable walk above does not reach it. The funnel's
    // `check_expr_types(entry_id)` call already validates the entry expression
    // nodes under `StageCheck::UdtErase`; the residual full-arena gap is entry-nested
    // Block `.ty` and Local Pat `.ty`, which this entry-root walk covers
    // (re-covering entry exprs here is redundant but harmless).
    let target_pkg = store.get(target_package_id);
    if let Some(entry_id) = target_pkg.entry {
        for_each_node_from_expr_root(target_pkg, entry_id, &mut |node| {
            check_node(target_pkg, node);
        });
    }
}

/// Reachable-item-scoped variant of [`check_id_references`] for foreign
/// packages.
///
/// The target package keeps its full-arena [`check_id_references`] ordering
/// guard (run at the top of the funnel) so a malformed `block.stmts` list
/// surfaces the targeted diagnostic before reachability dereferences it.
/// Foreign packages never had that ordering guard (reachability already
/// dereferenced their IDs), so scoping their id-reference check to reachable
/// callables loses no diagnostic while skipping the dead std/core arenas. This
/// applies the same per-node id-existence and self-id-field assertions as
/// [`check_id_references`] over each reachable foreign callable's nodes.
fn check_id_references_in_reachable_items(
    store: &PackageStore,
    reachable: &FxHashSet<StoreItemId>,
    target_package_id: qsc_fir::fir::PackageId,
) {
    for item_id in reachable {
        if item_id.package == target_package_id {
            continue;
        }
        let pkg = store.get(item_id.package);
        let item = pkg.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            for_each_node_in_callable(pkg, decl, &mut |node| match node {
                CallableNode::Block(id) => {
                    let block = pkg.get_block(id);
                    assert_eq!(block.id, id, "Block {id} has mismatched id field");
                    for &stmt_id in &block.stmts {
                        assert!(
                            pkg.stmts.get(stmt_id).is_some(),
                            "Block {id} references nonexistent Stmt {stmt_id}"
                        );
                    }
                }
                CallableNode::Stmt(id) => {
                    let stmt = pkg.get_stmt(id);
                    assert_eq!(stmt.id, id, "Stmt {id} has mismatched id field");
                    match &stmt.kind {
                        StmtKind::Expr(e) | StmtKind::Semi(e) => {
                            assert!(
                                pkg.exprs.get(*e).is_some(),
                                "Stmt {id} references nonexistent Expr {e}"
                            );
                        }
                        StmtKind::Local(_, pat, expr) => {
                            assert!(
                                pkg.pats.get(*pat).is_some(),
                                "Stmt {id} references nonexistent Pat {pat}"
                            );
                            assert!(
                                pkg.exprs.get(*expr).is_some(),
                                "Stmt {id} references nonexistent Expr {expr}"
                            );
                        }
                        StmtKind::Item(_) => {}
                    }
                }
                CallableNode::Expr(id) => {
                    let expr = pkg.get_expr(id);
                    assert_eq!(expr.id, id, "Expr {id} has mismatched id field");
                    check_expr_sub_ids(pkg, id, &expr.kind);
                }
                CallableNode::Pat(_) => {}
            });
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
/// expression ends in a trailing expression whose type matches the block type,
/// unless that trailing expression diverges (`fail`/`return`), in which case the
/// type may legitimately differ.
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
/// trailing `StmtKind::Expr`. A trailing expression whose type differs from the
/// block type is tolerated only when that expression diverges (`fail`/`return`).
pub(crate) fn check_non_unit_block_tails(
    store: &PackageStore,
    package_id: qsc_fir::fir::PackageId,
    reachable: &FxHashSet<StoreItemId>,
    skip: &FxHashSet<StoreItemId>,
) {
    let package = store.get(package_id);
    let Some(entry_id) = package.entry else {
        return;
    };

    for item_id in reachable {
        // Return unification runs across the whole reachable closure, so this
        // block-tail check applies to every reachable package.
        if !structural_check_in_scope(StageCheck::ReturnUnify) {
            continue;
        }

        // A callable left un-rewritten by return unification keeps a residual
        // `Return`, so its non-Unit blocks may still end in a `Semi` wrapper
        // rather than the collapsed single-exit tail. Skip only this
        // per-callable fan-out for such callables; the entry-scoped nested
        // block-tail walk below still covers every other reachable block.
        if skip.contains(item_id) {
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
/// type and does not diverge (`fail`/`return`). A divergent trailing expression
/// is exempt because it never yields a value, so typeck may leave its type
/// different from the enclosing block.
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
    // A divergent trailing expression (`fail`/`return`, or an `if`/block that
    // always diverges) never yields a value, so typeck may leave it with a
    // type that differs from the enclosing non-Unit block. Tolerate that
    // mismatch; any non-divergent type mismatch is still a real violation.
    assert!(
        expr_ty == &block.ty || expr_diverges(package, expr_id),
        "Non-Unit block-tail invariant violation: {context} Block {block_id} has type {:?} but trailing Expr {expr_id} has type {expr_ty:?}",
        block.ty,
    );
}

/// Returns `true` if evaluating `expr_id` never yields a value because it always
/// diverges (via `fail` or `return`).
///
/// Typeck assigns a divergent expression a fresh divergent type that defaults to
/// `Unit` when left unconstrained, so a divergent trailing expression can
/// legitimately carry a type that differs from its enclosing non-Unit block. The
/// predicate stays conservative: any unrecognized shape is treated as
/// non-divergent so genuine value-type mismatches still surface.
fn expr_diverges(package: &Package, expr_id: ExprId) -> bool {
    match &package.get_expr(expr_id).kind {
        ExprKind::Fail(_) | ExprKind::Return(_) => true,
        ExprKind::Block(block_id) => block_diverges(package, *block_id),
        ExprKind::If(_, then, Some(els)) => {
            expr_diverges(package, *then) && expr_diverges(package, *els)
        }
        _ => false,
    }
}

/// Returns `true` if `block_id` ends in a divergent trailing expression.
fn block_diverges(package: &Package, block_id: BlockId) -> bool {
    let block = package.get_block(block_id);
    let Some(&stmt_id) = block.stmts.last() else {
        return false;
    };
    match &package.get_stmt(stmt_id).kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => expr_diverges(package, *expr_id),
        StmtKind::Local(..) | StmtKind::Item(_) => false,
    }
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
/// - `check_callable_input_pattern_shapes` once tuple-decompose and argument promotion may
///   have synthesized tuple-shaped inputs.
/// - `check_no_returns` once return unification should have removed
///   `ExprKind::Return`.
/// - `check_spec_decl_types` on the body and explicit specializations.
/// - `check_local_var_consistency` to ensure every local reference is still
///   backed by a binder.
/// - `check_spec_exec_graph` once exec graphs have been rebuilt at `PostAll`.
fn check_reachable_invariants(
    store: &PackageStore,
    reachable: &FxHashSet<StoreItemId>,
    level: InvariantLevel,
    skip: &FxHashSet<StoreItemId>,
) {
    for item_id in reachable {
        // Every structural pass runs across the whole reachable closure, so
        // `enforces_stage` admits every reachable callable once its level is
        // reached. Package-wide UDT-erase invariants and reachable-spec exec
        // graphs are validated separately across the closure.
        let item_pkg = store.get(item_id.package);
        let item = item_pkg.get_item(item_id.item);
        if let ItemKind::Callable(decl) = &item.kind {
            // All reachable callables have been through the full pipeline
            // via the entry expression and should pass all stage-specific
            // invariant checks.
            check_type_invariants(&decl.output, level, "callable output type");

            // Check the input parameter pattern types too: the
            // `check_spec_decl_types` statement walk only covers `let`
            // bindings, so a stage-eliminated form left in the input signature
            // would otherwise go unchecked.
            check_pat_types(item_pkg, decl.input, level);

            if enforces_stage(level, StageCheck::Defunc) {
                check_no_arrow_params(item_pkg, decl);
            }

            if enforces_stage(level, StageCheck::ArgPromote) {
                check_callable_input_pattern_shapes(item_pkg, decl);
            }

            if enforces_stage(level, StageCheck::ReturnUnify) && !skip.contains(item_id) {
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

            if enforces_stage(level, StageCheck::Mono) {
                check_local_var_consistency(item_pkg, decl);
            }
        }
    }
}

/// Per-stage forcing function for the structural invariant checks.
///
/// Every structural pass runs across the whole reachable package closure, so
/// each stage's invariant holds for every reachable package and this admits
/// every stage. The exhaustive match is kept so that adding a stage forces an
/// explicit decision about whether its invariant holds across the closure.
fn structural_check_in_scope(check: StageCheck) -> bool {
    match check {
        // Every structural pass rewrites the whole reachable callable closure,
        // so each stage's invariant holds for every reachable package.
        StageCheck::ReturnUnify
        | StageCheck::UdtErase
        | StageCheck::TupleCompLower
        | StageCheck::TupleDecompose
        | StageCheck::ArgPromote
        | StageCheck::Mono
        | StageCheck::Defunc => true,
    }
}

/// Returns `true` when `level` enforces `check` and `check`'s structural
/// invariant is established across the whole reachable package closure.
///
/// Composes [`InvariantLevel::enforces`] with the forcing function
/// [`structural_check_in_scope`]. Every structural pass runs across the
/// closure, so this currently reduces to `level.enforces(check)`; the
/// composition is kept so a future not-yet-established stage has a single place
/// to be gated.
fn enforces_stage(level: InvariantLevel, check: StageCheck) -> bool {
    level.enforces(check) && structural_check_in_scope(check)
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

/// Bind-pattern names of the locals return unification synthesizes to carry an
/// early return: the boolean "already returned" flag and the deferred return
/// value slot.
///
/// These labels are consulted *only* to recognize a lowered flag write for the
/// fail-fast structural assertion in
/// [`check_no_flag_writes_in_operand_position`]. They never drive transform or
/// cleanup branch logic, which identify the same locals by `LocalVarId`
/// identity. Keeping the assertion name-based is acceptable because it is a
/// debug-only well-formedness check, not a semantic rewrite.
///
/// Sourced from the centralized `return_unify::symbols` constants so this
/// lookup automatically tracks the synthesized `.`-bearing in-memory spelling
/// (`_.has_returned` / `_.ret_val`) — those constants are the single source of
/// truth for these names.
const RETURN_FLAG_LOCAL_LABELS: [&str; 2] = [
    crate::return_unify::symbols::HAS_RETURNED,
    crate::return_unify::symbols::RET_VAL,
];

/// Verifies that no synthesized return-flag write survives in an operand
/// (eagerly-evaluated, non-statement) position after return unification.
///
/// Return unification lowers each `return` into writes of a synthesized
/// boolean flag and value slot, then relies on the ANF operand-lift pre-pass to
/// guarantee every such write lands at a statement boundary — never buried
/// inside a `Block`/`If`/`While` (or any other operand slot) whose value feeds
/// an enclosing operator, call, binding, or projection. A flag write reached in
/// an operand position therefore signals an operand lift that failed to expose
/// its buried `Return`. This check fails fast on that regression, before
/// partial evaluation or QIR lowering would mis-sequence the write.
///
/// The traversal mirrors the operand classification the ANF measure uses (the
/// sticky `in_operand` flag, flipped on entry to each eagerly-evaluated slot),
/// so "operand position" means exactly what the lift pre-pass targets.
///
/// Callables in `skip` are bypassed: they retain a residual `Return` by design
/// and were never lowered into flag writes.
///
/// # Panics
///
/// Panics if a synthesized flag write is found in an operand position.
fn check_no_flag_writes_in_operand_position(
    store: &PackageStore,
    reachable: &FxHashSet<StoreItemId>,
    skip: &FxHashSet<StoreItemId>,
) {
    // Return unification runs across the whole reachable closure, so a foreign
    // callable may carry synthesized flag locals too. Collect them lazily per
    // owning package, since each package has an independent `LocalVarId` space.
    let mut flag_locals_by_pkg: FxHashMap<PackageId, FxHashSet<LocalVarId>> = FxHashMap::default();

    for item_id in reachable {
        if !structural_check_in_scope(StageCheck::ReturnUnify) || skip.contains(item_id) {
            continue;
        }
        let item_pkg = store.get(item_id.package);
        let flag_locals = flag_locals_by_pkg
            .entry(item_id.package)
            .or_insert_with(|| collect_return_flag_locals(item_pkg));
        if flag_locals.is_empty() {
            continue;
        }
        let item = item_pkg.get_item(item_id.item);
        let ItemKind::Callable(decl) = &item.kind else {
            continue;
        };
        let callable_name = &decl.name.name;
        for block_id in callable_root_blocks(decl) {
            scan_block_for_operand_flag_writes(
                item_pkg,
                block_id,
                false,
                flag_locals,
                callable_name,
            );
        }
    }
}

/// Collects the `LocalVarId`s of every binding whose name matches a synthesized
/// return-flag label (see [`RETURN_FLAG_LOCAL_LABELS`]).
fn collect_return_flag_locals(package: &Package) -> FxHashSet<LocalVarId> {
    package
        .pats
        .values()
        .filter_map(|pat| match &pat.kind {
            PatKind::Bind(ident) if RETURN_FLAG_LOCAL_LABELS.contains(&ident.name.as_ref()) => {
                Some(ident.id)
            }
            _ => None,
        })
        .collect()
}

/// Returns the root blocks of a callable's specializations (body plus any
/// functor specializations or simulatable-intrinsic body).
fn callable_root_blocks(decl: &CallableDecl) -> Vec<BlockId> {
    match &decl.implementation {
        CallableImpl::Intrinsic => Vec::new(),
        CallableImpl::SimulatableIntrinsic(spec) => vec![spec.block],
        CallableImpl::Spec(spec_impl) => {
            let mut blocks = vec![spec_impl.body.block];
            for spec in [&spec_impl.adj, &spec_impl.ctl, &spec_impl.ctl_adj]
                .into_iter()
                .flatten()
            {
                blocks.push(spec.block);
            }
            blocks
        }
    }
}

/// Walks a block's statements, classifying each statement's value position.
///
/// A block re-establishes statement boundaries for its own statements: a
/// statement whose value is discarded (`Semi`, or a non-trailing `Expr`)
/// executes in order at a clean boundary, so a flag write there is correctly
/// sequenced even when the block itself is consumed as an operand. Only two
/// positions inherit the enclosing operand mode: a `let`/`mutable` initializer
/// (its value is bound, not discarded) and the block's trailing `Expr` (its
/// value is the block's value, so it is an operand exactly when the block is).
fn scan_block_for_operand_flag_writes(
    package: &Package,
    block_id: BlockId,
    in_operand: bool,
    flag_locals: &FxHashSet<LocalVarId>,
    callable_name: &str,
) {
    let stmts = &package.get_block(block_id).stmts;
    let last_index = stmts.len().wrapping_sub(1);
    for (index, &stmt_id) in stmts.iter().enumerate() {
        match &package.get_stmt(stmt_id).kind {
            // A trailing `Expr` carries the block's value, so it is an operand
            // exactly when the block is reached in operand position. Any other
            // expression statement discards its value at the boundary.
            StmtKind::Expr(expr_id) => {
                let tail_operand = in_operand && index == last_index;
                scan_expr_for_operand_flag_writes(
                    package,
                    *expr_id,
                    tail_operand,
                    flag_locals,
                    callable_name,
                );
            }
            StmtKind::Semi(expr_id) => {
                scan_expr_for_operand_flag_writes(
                    package,
                    *expr_id,
                    false,
                    flag_locals,
                    callable_name,
                );
            }
            // A `let`/`mutable` initializer is an operand slot: its value is
            // consumed by the binding rather than discarded at the statement
            // boundary.
            StmtKind::Local(_, _, expr_id) => {
                scan_expr_for_operand_flag_writes(
                    package,
                    *expr_id,
                    true,
                    flag_locals,
                    callable_name,
                );
            }
            StmtKind::Item(_) => {}
        }
    }
}

/// Recursively classifies operand positions within an expression, mirroring the
/// ANF operand measure's sticky `in_operand` flag, and asserts that no flag
/// write occupies an operand position. Unlike the measure (which only counts),
/// this walk descends every child so it reaches every statement boundary.
#[allow(clippy::too_many_lines)]
fn scan_expr_for_operand_flag_writes(
    package: &Package,
    expr_id: ExprId,
    in_operand: bool,
    flag_locals: &FxHashSet<LocalVarId>,
    callable_name: &str,
) {
    let kind = &package.get_expr(expr_id).kind;

    if in_operand && let Some(root) = assign_place_root_local(package, kind) {
        assert!(
            !flag_locals.contains(&root),
            "PostReturnUnify invariant violation: synthesized return-flag write \
             survives in an operand position (Expr {expr_id}) in callable '{callable_name}'"
        );
    }

    match kind {
        ExprKind::Return(inner) => {
            scan_expr_for_operand_flag_writes(package, *inner, true, flag_locals, callable_name);
        }
        ExprKind::Block(block_id) => {
            scan_block_for_operand_flag_writes(
                package,
                *block_id,
                in_operand,
                flag_locals,
                callable_name,
            );
        }
        ExprKind::If(cond, then, otherwise) => {
            scan_expr_for_operand_flag_writes(package, *cond, true, flag_locals, callable_name);
            scan_expr_for_operand_flag_writes(
                package,
                *then,
                in_operand,
                flag_locals,
                callable_name,
            );
            if let Some(otherwise) = otherwise {
                scan_expr_for_operand_flag_writes(
                    package,
                    *otherwise,
                    in_operand,
                    flag_locals,
                    callable_name,
                );
            }
        }
        ExprKind::While(cond, body) => {
            // The condition's value is consumed by the loop test each
            // iteration; the body is evaluated for effect and its value
            // discarded (a `while` yields Unit).
            scan_expr_for_operand_flag_writes(package, *cond, true, flag_locals, callable_name);
            scan_block_for_operand_flag_writes(package, *body, false, flag_locals, callable_name);
        }
        // Short-circuit logical operators carry the enclosing mode; eager
        // operand slots nested deeper re-flip the mode to operand.
        ExprKind::BinOp(BinOp::AndL | BinOp::OrL, a, b) => {
            scan_expr_for_operand_flag_writes(package, *a, in_operand, flag_locals, callable_name);
            scan_expr_for_operand_flag_writes(package, *b, in_operand, flag_locals, callable_name);
        }
        // Single-operand eager compounds.
        ExprKind::Fail(e) | ExprKind::Field(e, _) | ExprKind::UnOp(_, e) => {
            scan_expr_for_operand_flag_writes(package, *e, true, flag_locals, callable_name);
        }
        // Two-operand eager compounds.
        ExprKind::ArrayRepeat(a, b)
        | ExprKind::Assign(a, b)
        | ExprKind::AssignOp(_, a, b)
        | ExprKind::BinOp(_, a, b)
        | ExprKind::Call(a, b)
        | ExprKind::Index(a, b)
        | ExprKind::AssignField(a, _, b)
        | ExprKind::UpdateField(a, _, b) => {
            scan_expr_for_operand_flag_writes(package, *a, true, flag_locals, callable_name);
            scan_expr_for_operand_flag_writes(package, *b, true, flag_locals, callable_name);
        }
        // Three-operand eager compounds.
        ExprKind::AssignIndex(a, b, c) | ExprKind::UpdateIndex(a, b, c) => {
            scan_expr_for_operand_flag_writes(package, *a, true, flag_locals, callable_name);
            scan_expr_for_operand_flag_writes(package, *b, true, flag_locals, callable_name);
            scan_expr_for_operand_flag_writes(package, *c, true, flag_locals, callable_name);
        }
        // N-ary eager compounds.
        ExprKind::Array(exprs) | ExprKind::ArrayLit(exprs) | ExprKind::Tuple(exprs) => {
            for &e in exprs {
                scan_expr_for_operand_flag_writes(package, e, true, flag_locals, callable_name);
            }
        }
        ExprKind::Range(start, step, end) => {
            for &e in [start, step, end].into_iter().flatten() {
                scan_expr_for_operand_flag_writes(package, e, true, flag_locals, callable_name);
            }
        }
        ExprKind::Struct(_, copy, fields) => {
            if let Some(copy) = copy {
                scan_expr_for_operand_flag_writes(package, *copy, true, flag_locals, callable_name);
            }
            for field in fields {
                scan_expr_for_operand_flag_writes(
                    package,
                    field.value,
                    true,
                    flag_locals,
                    callable_name,
                );
            }
        }
        ExprKind::String(components) => {
            for component in components {
                if let StringComponent::Expr(e) = component {
                    scan_expr_for_operand_flag_writes(
                        package,
                        *e,
                        true,
                        flag_locals,
                        callable_name,
                    );
                }
            }
        }
        ExprKind::Closure(_, _) | ExprKind::Hole | ExprKind::Lit(_) | ExprKind::Var(_, _) => {}
    }
}

/// Resolves the root local of an assignment's place expression, peeling
/// `Field`/`Index` projections, when `kind` is an assignment whose target roots
/// at a local variable. Returns `None` for non-assignment expressions or places
/// not rooted at a local.
fn assign_place_root_local(package: &Package, kind: &ExprKind) -> Option<LocalVarId> {
    let place = match kind {
        ExprKind::Assign(place, _)
        | ExprKind::AssignOp(_, place, _)
        | ExprKind::AssignField(place, _, _)
        | ExprKind::AssignIndex(place, _, _) => *place,
        _ => return None,
    };
    place_root_local(package, place)
}

/// Peels `Field`/`Index` projections off a place expression to find its root
/// local variable, if any.
fn place_root_local(package: &Package, expr_id: ExprId) -> Option<LocalVarId> {
    match &package.get_expr(expr_id).kind {
        ExprKind::Var(Res::Local(id), _) => Some(*id),
        ExprKind::Field(inner, _) | ExprKind::Index(inner, _) => place_root_local(package, *inner),
        _ => None,
    }
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
/// This check becomes relevant after tuple-decomposing stages such as tuple-decompose and
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
    // A specialization may carry its own input pattern (for example the
    // controlled specialization's added control register). Validate its types
    // against the stage invariants alongside the body statements.
    if let Some(input) = spec.input {
        check_pat_types(package, input, level);
    }
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
/// - `check_local_pat_for_nested_tuple_arrow` after tuple-decompose (arrow types may
///   appear inside tuples between UDT erasure and tuple-decompose).
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
            if enforces_stage(level, StageCheck::TupleDecompose) {
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
/// `check_package_udt_erase_invariants_in_reachable_items`: this walker fires on
/// every reachable expression in the target package, while the reachable-scoped
/// walker visits every reachable callable expression in every reachable
/// package. Both paths must agree so a regression caught in either scope
/// produces the same diagnostic.
fn check_expr_type(
    store: &PackageStore,
    package: &Package,
    expr_id: ExprId,
    level: InvariantLevel,
) {
    let expr = package.get_expr(expr_id);
    check_type_invariants(&expr.ty, level, &format!("Expr {expr_id}"));

    if let Some(kind_name) = assignment_kind_name(&expr.kind) {
        assert!(
            expr.ty == Ty::UNIT,
            "Assignment type invariant violation: Expr {expr_id} is {kind_name} but has type {:?}",
            expr.ty,
        );
    }

    // After defunctionalization, no closures should remain in reachable code.
    if enforces_stage(level, StageCheck::Defunc) {
        assert!(
            !matches!(&expr.kind, ExprKind::Closure(_, _)),
            "Expr {expr_id} is a Closure after defunctionalization"
        );
    }

    // PostMono: no remaining generic args on Var references.
    if enforces_stage(level, StageCheck::Mono)
        && let ExprKind::Var(_, args) = &expr.kind
    {
        assert!(
            args.is_empty(),
            "PostMono invariant violation: Expr {expr_id} still has non-empty generic args"
        );
    }

    // After UDT erasure, all Struct expressions must have been lowered.
    if enforces_stage(level, StageCheck::UdtErase) {
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
    if enforces_stage(level, StageCheck::TupleCompLower)
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
    if enforces_stage(level, StageCheck::Defunc)
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

    if enforces_stage(level, StageCheck::ArgPromote)
        && let ExprKind::Call(callee_id, arg_id) = &expr.kind
    {
        check_call_shape_matches_callee(store, package, expr_id, *callee_id, *arg_id);
    }
}

/// Names assignment expression variants whose result type must be `Unit`.
fn assignment_kind_name(kind: &ExprKind) -> Option<&'static str> {
    match kind {
        ExprKind::Assign(_, _) => Some("Assign"),
        ExprKind::AssignField(_, _, _) => Some("AssignField"),
        ExprKind::AssignIndex(_, _, _) => Some("AssignIndex"),
        ExprKind::AssignOp(_, _, _) => Some("AssignOp"),
        _ => None,
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

    let call = package.get_expr(call_expr_id);
    if arg.ty != expected_input {
        if let Some((arrow_input, arrow_output)) = resolve_arrow_expr_signature(package, callee_id)
            && arg.ty == arrow_input
            && call.ty == arrow_output
        {
            return;
        }

        panic!(
            "PostArgPromote/PostAll call invariant violation: Expr {call_expr_id} passes Expr \
             {arg_id} with type {:?} to callee Expr {callee_id} expecting input type \
             {expected_input:?}",
            arg.ty,
        );
    }

    assert!(
        call.ty == expected_output,
        "PostArgPromote/PostAll call invariant violation: Expr {call_expr_id} has type {:?} \
         but callee Expr {callee_id} returns {expected_output:?}",
        call.ty,
    );
}

/// Resolves a callee expression to its `(input_ty, output_ty)` signature.
///
/// Handles direct item callees, including `UnOp(Functor, Var(Item))` wrappers,
/// before falling back to a direct `Ty::Arrow`-typed expression such as a
/// captured callable value. Returns `None` when the callee is neither form;
/// callers treat `None` as an invariant violation.
fn resolve_call_signature(
    store: &PackageStore,
    package: &Package,
    callee_id: ExprId,
) -> Option<(Ty, Ty)> {
    if let Some((item_id, controlled_depth)) = resolve_direct_item_callee(package, callee_id)
        && let Some((_, callee_package)) = store
            .iter()
            .find(|(package_id, _)| *package_id == item_id.package)
        && let Some(item) = callee_package.items.get(item_id.item)
        && let ItemKind::Callable(decl) = &item.kind
    {
        let input_ty = callee_package.get_pat(decl.input).ty.clone();
        return Some((
            apply_controlled_input_layers(input_ty, controlled_depth),
            decl.output.clone(),
        ));
    }

    let callee = package.get_expr(callee_id);
    if let Ty::Arrow(arrow) = &callee.ty {
        return Some(((*arrow.input).clone(), (*arrow.output).clone()));
    }

    None
}

/// Resolves a callee expression from its stored arrow type metadata.
fn resolve_arrow_expr_signature(package: &Package, callee_id: ExprId) -> Option<(Ty, Ty)> {
    let callee = package.get_expr(callee_id);
    let Ty::Arrow(arrow) = &callee.ty else {
        return None;
    };

    Some(((*arrow.input).clone(), (*arrow.output).clone()))
}

/// Resolves a direct item callee through adjoint and controlled functor
/// wrappers, returning the item and controlled depth.
fn resolve_direct_item_callee(package: &Package, callee_id: ExprId) -> Option<(ItemId, usize)> {
    let mut current = callee_id;
    let mut controlled_depth = 0usize;

    loop {
        let expr = package.get_expr(current);
        match &expr.kind {
            ExprKind::Var(Res::Item(item_id), _) => return Some((*item_id, controlled_depth)),
            ExprKind::UnOp(UnOp::Functor(Functor::Adj), inner_id) => {
                current = *inner_id;
            }
            ExprKind::UnOp(UnOp::Functor(Functor::Ctl), inner_id) => {
                controlled_depth += 1;
                current = *inner_id;
            }
            _ => return None,
        }
    }
}

/// Applies one controlled-call input tuple layer for each controlled wrapper.
fn apply_controlled_input_layers(mut input_ty: Ty, controlled_depth: usize) -> Ty {
    for _ in 0..controlled_depth {
        input_ty = Ty::Tuple(vec![Ty::Array(Box::new(Ty::Prim(Prim::Qubit))), input_ty]);
    }
    input_ty
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
/// pipeline stage while walking into nested array, tuple, and arrow types. Each
/// stage's assertion is gated by `enforces_stage`, so a type is only checked
/// against an invariant once the establishing pass's pipeline level is reached.
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
                !enforces_stage(level, StageCheck::Mono),
                "{context} contains Ty::Param after monomorphization"
            );
        }
        Ty::Arrow(arrow) => {
            if enforces_stage(level, StageCheck::Mono) {
                assert!(
                    !matches!(arrow.functors, FunctorSet::Param(_)),
                    "{context} contains FunctorSet::Param after monomorphization"
                );
            }
            if enforces_stage(level, StageCheck::Defunc) {
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
                !enforces_stage(level, StageCheck::UdtErase),
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

/// Verifies that every `Res::Local(id)` in a callable implementation refers to
/// a `LocalVarId` that is visible in the current lexical scope:
/// - the callable's input pattern,
/// - the current specialization input pattern, or
/// - an earlier `PatKind::Bind` in the current block scope.
///
/// # Panics
///
/// Panics if a local reference is found that is not in the bound set.
fn check_local_var_consistency(package: &Package, decl: &CallableDecl) {
    let mut callable_scope: FxHashSet<LocalVarId> = FxHashSet::default();
    collect_pat_bindings(package, decl.input, &mut callable_scope);

    match &decl.implementation {
        CallableImpl::Spec(spec_impl) => {
            check_spec_local_var_consistency(
                package,
                decl,
                "body",
                &spec_impl.body,
                &callable_scope,
            );
            for (label, spec) in [
                ("adj", spec_impl.adj.as_ref()),
                ("ctl", spec_impl.ctl.as_ref()),
                ("ctl_adj", spec_impl.ctl_adj.as_ref()),
            ] {
                if let Some(spec) = spec {
                    check_spec_local_var_consistency(package, decl, label, spec, &callable_scope);
                }
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            check_spec_local_var_consistency(
                package,
                decl,
                "simulatable intrinsic",
                spec,
                &callable_scope,
            );
        }
        CallableImpl::Intrinsic => {}
    }
}

/// Checks one callable specialization with callable-level and spec-level input
/// bindings already in scope.
fn check_spec_local_var_consistency(
    package: &Package,
    decl: &CallableDecl,
    label: &str,
    spec: &SpecDecl,
    callable_scope: &FxHashSet<LocalVarId>,
) {
    let mut spec_scope = callable_scope.clone();
    if let Some(input_pat) = spec.input {
        collect_pat_bindings(package, input_pat, &mut spec_scope);
    }

    let context = format!("callable \"{}\" {label}", decl.name.name);
    let mut checker = LocalScopeChecker {
        package,
        bound: spec_scope,
        context,
    };
    checker.visit_block(spec.block);
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

/// Validates `Res::Local` references against the set of locals currently in
/// lexical scope, driven by the FIR [`Visitor`].
///
/// A local binding extends the current block scope only *after* its
/// initializer expression has been checked (`visit_stmt`); a nested block
/// inherits the outer scope but does not leak its own bindings back out
/// (`visit_block`). Delegating expression descent to the visitor's default
/// `walk_expr` keeps this checker exhaustive automatically as `ExprKind`
/// grows.
struct LocalScopeChecker<'a> {
    package: &'a Package,
    bound: FxHashSet<LocalVarId>,
    context: String,
}

impl<'a> Visitor<'a> for LocalScopeChecker<'a> {
    fn get_block(&self, id: BlockId) -> &'a Block {
        self.package.get_block(id)
    }

    fn get_expr(&self, id: ExprId) -> &'a Expr {
        self.package.get_expr(id)
    }

    fn get_pat(&self, id: PatId) -> &'a Pat {
        self.package.get_pat(id)
    }

    fn get_stmt(&self, id: StmtId) -> &'a Stmt {
        self.package.get_stmt(id)
    }

    fn visit_block(&mut self, id: BlockId) {
        // A nested block inherits the outer scope but must not leak its own
        // local bindings back out to the enclosing block.
        let saved = self.bound.clone();
        visit::walk_block(self, id);
        self.bound = saved;
    }

    fn visit_stmt(&mut self, id: StmtId) {
        if let StmtKind::Local(_, pat, expr) = &self.package.get_stmt(id).kind {
            // Check the initializer before the binding is in scope, then extend
            // the current block scope so later statements can reference it.
            let (pat, expr) = (*pat, *expr);
            self.visit_expr(expr);
            collect_pat_bindings(self.package, pat, &mut self.bound);
        } else {
            visit::walk_stmt(self, id);
        }
    }

    fn visit_expr(&mut self, id: ExprId) {
        match &self.package.get_expr(id).kind {
            ExprKind::Var(Res::Local(var_id), _) => {
                check_local_reference(id, *var_id, &self.bound, &self.context);
            }
            ExprKind::Closure(ids, _) => {
                for &var_id in ids {
                    check_local_reference(id, var_id, &self.bound, &self.context);
                }
            }
            _ => {}
        }
        visit::walk_expr(self, id);
    }
}

/// Asserts that a local reference is bound in the current lexical context.
fn check_local_reference(
    expr_id: ExprId,
    var_id: LocalVarId,
    bound: &FxHashSet<LocalVarId>,
    context: &str,
) {
    assert!(
        bound.contains(&var_id),
        "LocalVarId consistency: Expr {expr_id} references {var_id}, \
         which is not bound in {context}",
    );
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

/// Validates that every expression in a spec has a non-empty exec graph range
/// within both configured graph views.
fn check_spec_exec_graph_ranges(package: &Package, spec: &SpecDecl, context: &str) {
    let no_debug_len = spec.exec_graph.select_ref(ExecGraphConfig::NoDebug).len();
    let debug_len = spec.exec_graph.select_ref(ExecGraphConfig::Debug).len();

    crate::walk_utils::for_each_expr_in_block(package, spec.block, &mut |expr_id, expr| {
        let range = &expr.exec_graph_range;
        assert!(
            range.start != range.end,
            "Exec graph range for {context} Expr {expr_id} is empty"
        );
        assert!(
            range.start.no_debug_idx <= range.end.no_debug_idx
                && range.end.no_debug_idx <= no_debug_len,
            "Exec graph range for {context} Expr {expr_id} no_debug indices {range:?} exceed graph length {no_debug_len}"
        );
        assert!(
            range.start.debug_idx <= range.end.debug_idx && range.end.debug_idx <= debug_len,
            "Exec graph range for {context} Expr {expr_id} debug indices {range:?} exceed graph length {debug_len}"
        );
    });
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
    // `ExprId`s are package-relative, so ownership is tracked with a separate
    // `seen` map per package. The same `ExprId` value may legitimately appear
    // in two different packages without being a genuine collision.
    let mut seen_by_package: FxHashMap<PackageId, FxHashMap<ExprId, (LocalItemId, &'static str)>> =
        FxHashMap::default();

    for item_id in reachable {
        let package = store.get(item_id.package);
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

        let seen = seen_by_package.entry(item_id.package).or_default();
        for (spec, label) in specs {
            let mut expr_ids = FxHashSet::default();
            collect_expr_ids_in_block(package, spec.block, &mut expr_ids);
            for eid in &expr_ids {
                if let Some((prev_item, prev_label)) = seen.get(eid) {
                    panic!(
                        "PostDefunc ExprId uniqueness violation: {eid} appears in \
                         both {prev_item}/{prev_label} and {}/{label} in package {}",
                        item_id.item, item_id.package,
                    );
                }
                seen.insert(*eid, (item_id.item, label));
            }
        }
    }

    // Check entry expression ExprIds are disjoint from spec body ExprIds in
    // the target package (the entry expression lives in the target package,
    // so only that package's `seen` map is relevant).
    let package = store.get(package_id);
    let mut entry_expr_ids = FxHashSet::default();
    collect_expr_ids_in_expr(package, entry_id, &mut entry_expr_ids);
    if let Some(seen) = seen_by_package.get(&package_id) {
        for eid in &entry_expr_ids {
            if let Some((owner_item, owner_label)) = seen.get(eid) {
                panic!(
                    "PostDefunc entry/spec disjointness violation: {eid} appears in \
                     both the entry expression and {owner_item}/{owner_label}",
                );
            }
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
