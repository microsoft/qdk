// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Rebuilds exec graphs for all reachable callables and the entry expression.
//!
//! After earlier FIR transforms synthesize new expressions or statements with
//! empty ranges, the exec graphs on `SpecDecl` and
//! `Package.entry_exec_graph` are stale. In practice this includes return
//! unification, defunctionalization, UDT erasure, tuple-compare lowering,
//! SROA, and argument promotion. This pass reconstructs every graph from
//! scratch by walking the FIR and emitting the same node sequences that the
//! original lowerer would have produced.
//!
//! ## Transformation Shape
//!
//! **Before:** Callable specs and the entry expression carry stale
//! `exec_graph_range` values — often `EMPTY_EXEC_RANGE` sentinels inserted
//! by earlier passes. The exec graph vectors may reference deleted or
//! renumbered nodes.
//!
//! **After:** Every reachable callable spec and the entry expression has a
//! freshly built exec graph. Ranges on individual `Expr` and `Stmt` nodes
//! index into the rebuilt vectors.
//!
//! ## Borrow-Splitting Strategy
//!
//! The rebuild cannot hold both `&Package` (for reading expressions) and
//! `&mut Package` (for writing exec graphs) simultaneously. This is solved
//! by accumulating deferred writes in `RangeUpdates`: during the read-only
//! graph-building walk, expression and statement ranges are recorded as
//! `(ExprId, Range<ExecGraphIdx>)` and `(StmtId, Range<ExecGraphIdx>)`
//! pairs. After building completes and the immutable borrow ends,
//! `apply_ranges` writes each range back to the corresponding `Expr` or
//! `Stmt` under a mutable borrow.
//!
//! ## `ExecGraphBuilder` Delegation
//!
//! Graph nodes are emitted via `ExecGraphBuilder` from `qsc_lowerer`, which
//! maintains paired no-debug and debug node vectors. This ensures the rebuilt
//! graphs match the format produced by the original lowering pass.
//!
//! ## See Also
//!
//! - `qsc_lowerer::exec_graph` — The `ExecGraphBuilder` that emits graph
//!   nodes. The rebuild pass re-uses this builder to ensure graph format
//!   fidelity with the original lowering pass.

#[cfg(test)]
mod tests;

use std::ops::Range;

use qsc_fir::fir::{
    BinOp, BlockId, CallableImpl, ExecGraphDebugNode, ExecGraphIdx, ExecGraphNode, ExprId,
    ExprKind, ItemKind, LocalItemId, Package, PackageId, PackageLookup, PackageStore,
    SpecDecl as FirSpecDecl, StmtId, StmtKind, StoreItemId, StringComponent,
};
use qsc_fir::ty::Ty;
use qsc_lowerer::ExecGraphBuilder;

use crate::reachability::{collect_reachable_from_entry, collect_reachable_with_seeds};

/// Side-table collecting deferred `exec_graph_range` updates.
/// Populated during the read-only graph-building pass, then applied in a
/// separate write pass to avoid simultaneous mutable and immutable borrows.
#[derive(Default)]
struct RangeUpdates {
    exprs: Vec<(ExprId, Range<ExecGraphIdx>)>,
    stmts: Vec<(StmtId, Range<ExecGraphIdx>)>,
}

/// Applies collected range updates to package expressions and statements.
///
/// Invoked once per specialization (not once at the end of the pass). Each
/// call writes the ranges gathered for that spec back to the package
/// before the next specialization begins rebuilding.
fn apply_ranges(package: &mut Package, ranges: &RangeUpdates) {
    for (id, range) in &ranges.exprs {
        package
            .exprs
            .get_mut(*id)
            .expect("expr must exist")
            .exec_graph_range = range.clone();
    }
    for (id, range) in &ranges.stmts {
        package
            .stmts
            .get_mut(*id)
            .expect("stmt must exist")
            .exec_graph_range = range.clone();
    }
}

/// Collected spec info for a single callable — avoids holding a `&Package`
/// reference while mutating.
struct SpecInfo {
    block: BlockId,
    /// Which specialization on the containing callable should receive the
    /// rebuilt graph during write-back.
    kind: SpecKind,
}

/// Which specialization within a `CallableImpl`.
#[derive(Clone, Copy)]
enum SpecKind {
    /// The default callable body implementation.
    Body,
    /// The adjoint specialization.
    Adj,
    /// The controlled specialization.
    Ctl,
    /// The controlled-adjoint specialization.
    CtlAdj,
    /// A simulatable intrinsic with an explicit body block.
    SimulatableIntrinsic,
}

/// All spec infos for one callable item, collected while holding `&Package`.
struct CallableSpecs {
    item_id: LocalItemId,
    specs: Vec<SpecInfo>,
}

/// Rebuilds exec graphs for every reachable callable and the entry expression
/// in the given package. When `pinned_items` is non-empty, uses seed-based
/// reachability to include pinned callables that are not entry-reachable.
///
/// This must be called after all FIR transforms have completed. The function
/// is idempotent — calling it multiple times produces the same result.
pub fn rebuild_exec_graphs(
    store: &mut PackageStore,
    package_id: PackageId,
    pinned_items: &[StoreItemId],
) {
    // Early return if there is no entry expression — nothing to rebuild.
    {
        let package = store.get(package_id);
        if package.entry.is_none() {
            return;
        }
    }

    let reachable = if pinned_items.is_empty() {
        collect_reachable_from_entry(store, package_id)
    } else {
        collect_reachable_with_seeds(store, package_id, pinned_items)
    };

    let collected = collect_callable_specs(store, package_id, &reachable);
    rebuild_callable_exec_graphs(store, package_id, &collected);
    rebuild_entry_exec_graph(store, package_id);
}

/// Collects the block IDs for every spec in every reachable callable that
/// lives in this package (cross-package items are not rebuilt).
fn collect_callable_specs(
    store: &PackageStore,
    package_id: PackageId,
    reachable: &rustc_hash::FxHashSet<StoreItemId>,
) -> Vec<CallableSpecs> {
    let mut collected: Vec<CallableSpecs> = Vec::new();
    let package = store.get(package_id);
    for item_id in reachable {
        if item_id.package != package_id {
            continue;
        }
        let item = package.get_item(item_id.item);
        let decl = match &item.kind {
            ItemKind::Callable(decl) => decl.as_ref(),
            _ => continue,
        };
        let specs = collect_specs_from_impl(&decl.implementation);
        if !specs.is_empty() {
            collected.push(CallableSpecs {
                item_id: item_id.item,
                specs,
            });
        }
    }
    collected
}

/// Extracts `SpecInfo` entries from a callable implementation.
fn collect_specs_from_impl(implementation: &CallableImpl) -> Vec<SpecInfo> {
    let mut specs = Vec::new();
    match implementation {
        CallableImpl::Intrinsic => {}
        CallableImpl::Spec(spec_impl) => {
            specs.push(SpecInfo {
                block: spec_impl.body.block,
                kind: SpecKind::Body,
            });
            if let Some(adj) = &spec_impl.adj {
                specs.push(SpecInfo {
                    block: adj.block,
                    kind: SpecKind::Adj,
                });
            }
            if let Some(ctl) = &spec_impl.ctl {
                specs.push(SpecInfo {
                    block: ctl.block,
                    kind: SpecKind::Ctl,
                });
            }
            if let Some(ctl_adj) = &spec_impl.ctl_adj {
                specs.push(SpecInfo {
                    block: ctl_adj.block,
                    kind: SpecKind::CtlAdj,
                });
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            specs.push(SpecInfo {
                block: spec.block,
                kind: SpecKind::SimulatableIntrinsic,
            });
        }
    }
    specs
}

/// Rebuilds and writes back the exec graph for each collected callable spec.
fn rebuild_callable_exec_graphs(
    store: &mut PackageStore,
    package_id: PackageId,
    collected: &[CallableSpecs],
) {
    for callable in collected {
        for spec_info in &callable.specs {
            // Build graph — immutable borrow.
            let (graph, ranges) = {
                let package = store.get(package_id);
                let mut builder = ExecGraphBuilder::default();
                let mut ranges = RangeUpdates::default();
                rebuild_block(package, &mut builder, spec_info.block, &mut ranges);
                (builder.take(), ranges)
            };

            // Write back — mutable borrow.
            let package = store.get_mut(package_id);
            apply_ranges(package, &ranges);

            let target_spec = get_spec_decl_mut(package, callable.item_id, spec_info.kind);
            target_spec.exec_graph = graph;
        }
    }
}

/// Returns a mutable reference to the spec decl identified by `kind` on the
/// callable at `item_id`.
fn get_spec_decl_mut(
    package: &mut Package,
    item_id: LocalItemId,
    kind: SpecKind,
) -> &mut FirSpecDecl {
    let item = package.items.get_mut(item_id).expect("item must exist");
    let decl = match &mut item.kind {
        ItemKind::Callable(decl) => decl.as_mut(),
        _ => unreachable!("already verified callable"),
    };
    match kind {
        SpecKind::Body => match &mut decl.implementation {
            CallableImpl::Spec(si) => &mut si.body,
            _ => unreachable!("already verified Spec"),
        },
        SpecKind::Adj => match &mut decl.implementation {
            CallableImpl::Spec(si) => si.adj.as_mut().expect("adj must exist"),
            _ => unreachable!("already verified Spec"),
        },
        SpecKind::Ctl => match &mut decl.implementation {
            CallableImpl::Spec(si) => si.ctl.as_mut().expect("ctl must exist"),
            _ => unreachable!("already verified Spec"),
        },
        SpecKind::CtlAdj => match &mut decl.implementation {
            CallableImpl::Spec(si) => si.ctl_adj.as_mut().expect("ctl_adj must exist"),
            _ => unreachable!("already verified Spec"),
        },
        SpecKind::SimulatableIntrinsic => match &mut decl.implementation {
            CallableImpl::SimulatableIntrinsic(spec) => spec,
            _ => unreachable!("already verified SimulatableIntrinsic"),
        },
    }
}

/// Rebuilds the entry exec graph from the package's entry expression.
fn rebuild_entry_exec_graph(store: &mut PackageStore, package_id: PackageId) {
    let entry_id = store
        .get(package_id)
        .entry
        .expect("entry must exist; caller guards against missing entry");
    let (graph, ranges) = {
        let package = store.get(package_id);
        let mut builder = ExecGraphBuilder::default();
        let mut ranges = RangeUpdates::default();
        rebuild_expr(package, &mut builder, entry_id, &mut ranges);
        (builder.take(), ranges)
    };
    let package = store.get_mut(package_id);
    package.entry_exec_graph = graph;
    apply_ranges(package, &ranges);
}

/// Rebuilds the execution graph for a block by visiting each statement and
/// appending a `Unit` node when the block is empty or does not end with
/// an expression statement.
fn rebuild_block(
    package: &Package,
    builder: &mut ExecGraphBuilder,
    block_id: BlockId,
    ranges: &mut RangeUpdates,
) {
    builder.debug_push(ExecGraphDebugNode::PushScope);

    let block = package.get_block(block_id);
    let stmts = block.stmts.clone();

    let set_unit = stmts.is_empty()
        || !matches!(
            package.get_stmt(*stmts.last().expect("non-empty")).kind,
            StmtKind::Expr(..)
        );

    for &stmt_id in &stmts {
        rebuild_stmt(package, builder, stmt_id, ranges);
    }

    if set_unit {
        builder.push(ExecGraphNode::Unit);
    }

    builder.debug_push(ExecGraphDebugNode::BlockEnd(block_id));
    builder.debug_push(ExecGraphDebugNode::PopScope);
}

/// Rebuilds the execution graph for a single statement. `Local` bindings
/// emit a `Bind` node after the initializer expression; `Item` statements
/// are no-ops.
fn rebuild_stmt(
    package: &Package,
    builder: &mut ExecGraphBuilder,
    stmt_id: StmtId,
    ranges: &mut RangeUpdates,
) {
    let graph_start = builder.len();
    builder.debug_push(ExecGraphDebugNode::Stmt(stmt_id));

    let kind = package.get_stmt(stmt_id).kind.clone();
    match kind {
        StmtKind::Expr(expr_id) | StmtKind::Semi(expr_id) => {
            rebuild_expr(package, builder, expr_id, ranges);
        }
        StmtKind::Local(_, pat_id, expr_id) => {
            rebuild_expr(package, builder, expr_id, ranges);
            builder.push(ExecGraphNode::Bind(pat_id));
        }
        StmtKind::Item(_) => {}
    }

    ranges.stmts.push((stmt_id, graph_start..builder.len()));
}

/// Rebuilds the execution graph for an expression, recursively visiting
/// sub-expressions. Control-flow expressions (`If`, `While`, short-circuit
/// operators) produce jump nodes; assignments use `truncate` to discard
/// the LHS target nodes; multi-operand expressions interleave `Store`
/// nodes to preserve intermediate values on the evaluation stack.
#[allow(clippy::too_many_lines)]
fn rebuild_expr(
    package: &Package,
    builder: &mut ExecGraphBuilder,
    expr_id: ExprId,
    ranges: &mut RangeUpdates,
) {
    let graph_start = builder.len();
    let expr = package.get_expr(expr_id);
    let kind = expr.kind.clone();

    match kind {
        //  Control flow (no trailing Expr(id))
        ExprKind::BinOp(BinOp::AndL, lhs, rhs) => {
            rebuild_expr(package, builder, lhs, ranges);
            let idx = builder.len();
            builder.push(ExecGraphNode::Jump(0));
            rebuild_expr(package, builder, rhs, ranges);
            builder.set_with_arg(ExecGraphNode::JumpIfNot, idx, builder.len());
        }

        ExprKind::BinOp(BinOp::OrL, lhs, rhs) => {
            rebuild_expr(package, builder, lhs, ranges);
            let idx = builder.len();
            builder.push(ExecGraphNode::Jump(0));
            rebuild_expr(package, builder, rhs, ranges);
            builder.set_with_arg(ExecGraphNode::JumpIf, idx, builder.len());
        }

        ExprKind::Block(block_id) => {
            rebuild_block(package, builder, block_id, ranges);
        }

        ExprKind::If(cond, if_true, if_false) => {
            rebuild_expr(package, builder, cond, ranges);
            let branch_idx = builder.len();
            builder.push(ExecGraphNode::Jump(0));
            rebuild_expr(package, builder, if_true, ranges);

            if let Some(else_id) = if_false {
                // With else branch.
                let idx = builder.len();
                builder.push(ExecGraphNode::Jump(0));
                rebuild_expr(package, builder, else_id, ranges);
                builder.set_with_arg(ExecGraphNode::Jump, idx, builder.len());
                let else_idx = idx + 1;
                builder.set_with_arg(ExecGraphNode::JumpIfNot, branch_idx, else_idx);
            } else {
                // Without else — produces Unit.
                let idx = builder.len();
                builder.push(ExecGraphNode::Unit);
                builder.set_with_arg(ExecGraphNode::JumpIfNot, branch_idx, idx);
            }
        }

        ExprKind::While(cond, body_block) => {
            builder.debug_push(ExecGraphDebugNode::PushLoopScope(expr_id));
            let cond_idx = builder.len();
            rebuild_expr(package, builder, cond, ranges);
            let idx = builder.len();
            builder.push(ExecGraphNode::Jump(0));
            builder.debug_push(ExecGraphDebugNode::LoopIteration);
            rebuild_block(package, builder, body_block, ranges);
            builder.push_with_arg(ExecGraphNode::Jump, cond_idx);
            builder.set_with_arg(ExecGraphNode::JumpIfNot, idx, builder.len());
            builder.debug_push(ExecGraphDebugNode::PopScope);
            builder.push(ExecGraphNode::Unit);
        }

        ExprKind::Return(inner) => {
            rebuild_expr(package, builder, inner, ranges);
            builder.push_ret();
        }

        // Assignments (trailing Expr(id) + Unit)
        ExprKind::Assign(lhs, rhs) => {
            // Visit the LHS to record its range, then truncate the emitted
            // nodes — the LHS is an assignment target, not a value to evaluate.
            let idx = builder.len();
            rebuild_expr(package, builder, lhs, ranges);
            builder.truncate(idx);
            rebuild_expr(package, builder, rhs, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
            builder.push(ExecGraphNode::Unit);
        }

        ExprKind::AssignOp(op, lhs, rhs) => {
            let idx = builder.len();
            let is_array = matches!(package.get_expr(lhs).ty, Ty::Array(..));
            rebuild_expr(package, builder, lhs, ranges);

            if is_array {
                // Array assignment targets are not evaluated — truncate the
                // LHS nodes so only the RHS value remains on the stack.
                builder.truncate(idx);
            }

            let idx = builder.len();
            if matches!(op, BinOp::AndL | BinOp::OrL) {
                builder.push(ExecGraphNode::Jump(0));
            } else if !is_array {
                builder.push(ExecGraphNode::Store);
            }

            rebuild_expr(package, builder, rhs, ranges);

            match op {
                BinOp::AndL => {
                    builder.set_with_arg(ExecGraphNode::JumpIfNot, idx, builder.len());
                }
                BinOp::OrL => {
                    builder.set_with_arg(ExecGraphNode::JumpIf, idx, builder.len());
                }
                _ => {}
            }

            builder.push(ExecGraphNode::Expr(expr_id));
            builder.push(ExecGraphNode::Unit);
        }

        ExprKind::AssignField(container, _field, replace) => {
            rebuild_expr(package, builder, replace, ranges);
            builder.push(ExecGraphNode::Store);
            rebuild_expr(package, builder, container, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
            builder.push(ExecGraphNode::Unit);
        }

        ExprKind::AssignIndex(container, index, replace) => {
            rebuild_expr(package, builder, index, ranges);
            builder.push(ExecGraphNode::Store);
            rebuild_expr(package, builder, replace, ranges);
            // Truncate: container is the assignment target, not a value.
            let idx = builder.len();
            rebuild_expr(package, builder, container, ranges);
            builder.truncate(idx);
            builder.push(ExecGraphNode::Expr(expr_id));
            builder.push(ExecGraphNode::Unit);
        }

        // Multi-operand with Store (trailing Expr(id))
        // Each sub-expression is followed by a Store node that pushes its
        // value onto the evaluation stack, keeping all operands available
        // when the final Expr node evaluates the compound expression.
        //
        // Note: `ExprKind::Array` emits a `Store` after each item (items
        // are kept on the value stack for the final `Expr` node), while
        // `ExprKind::ArrayLit` pops after each item. This asymmetry
        // matches the evaluator's expected stack shape for the two
        // array-construction variants.
        ExprKind::Array(items) | ExprKind::Tuple(items) => {
            for item_id in &items {
                rebuild_expr(package, builder, *item_id, ranges);
                builder.push(ExecGraphNode::Store);
            }
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::ArrayLit(items) => {
            for item_id in &items {
                rebuild_expr(package, builder, *item_id, ranges);
                builder.pop();
            }
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::ArrayRepeat(val, size) => {
            rebuild_expr(package, builder, val, ranges);
            builder.push(ExecGraphNode::Store);
            rebuild_expr(package, builder, size, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::BinOp(_op, lhs, rhs) => {
            // Non-short-circuit binary op (AndL/OrL handled above).
            // Store saves the LHS value so both operands are available
            // when the Expr node evaluates the operation.
            rebuild_expr(package, builder, lhs, ranges);
            builder.push(ExecGraphNode::Store);
            rebuild_expr(package, builder, rhs, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::Call(callee, arg) => {
            // Evaluate and store the callee, then evaluate the argument.
            // The Expr node performs the actual call dispatch at runtime.
            rebuild_expr(package, builder, callee, ranges);
            builder.push(ExecGraphNode::Store);
            rebuild_expr(package, builder, arg, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::Index(container, index) => {
            rebuild_expr(package, builder, container, ranges);
            builder.push(ExecGraphNode::Store);
            rebuild_expr(package, builder, index, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::UpdateField(record, _field, replace) => {
            rebuild_expr(package, builder, replace, ranges);
            builder.push(ExecGraphNode::Store);
            rebuild_expr(package, builder, record, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::UpdateIndex(lhs, mid, rhs) => {
            rebuild_expr(package, builder, mid, ranges);
            builder.push(ExecGraphNode::Store);
            rebuild_expr(package, builder, rhs, ranges);
            builder.push(ExecGraphNode::Store);
            rebuild_expr(package, builder, lhs, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::Range(start, step, end) => {
            if let Some(s) = start {
                rebuild_expr(package, builder, s, ranges);
                builder.push(ExecGraphNode::Store);
            }
            if let Some(st) = step {
                rebuild_expr(package, builder, st, ranges);
                builder.push(ExecGraphNode::Store);
            }
            if let Some(e) = end {
                rebuild_expr(package, builder, e, ranges);
            }
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::String(components) => {
            for component in &components {
                if let StringComponent::Expr(comp_expr_id) = component {
                    rebuild_expr(package, builder, *comp_expr_id, ranges);
                    builder.push(ExecGraphNode::Store);
                }
            }
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        // Simple variants (just Expr(id))
        ExprKind::Lit(..) | ExprKind::Var(..) => {
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::Fail(msg) => {
            rebuild_expr(package, builder, msg, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::Field(container, _) => {
            rebuild_expr(package, builder, container, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        ExprKind::UnOp(_, operand) => {
            rebuild_expr(package, builder, operand, ranges);
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        // Eliminated variant
        //
        // `ExprKind::Struct` must be unreachable here: the UDT erasure pass
        // establishes [`crate::invariants::InvariantLevel::PostUdtErase`],
        // which guarantees that no `ExprKind::Struct` survives into
        // exec-graph rebuild.
        ExprKind::Struct(..) => {
            panic!("Struct expressions should have been eliminated by udt_erase");
        }

        // Eliminated variant
        //
        // Closures and holes are forbidden by the `PostDefunc` invariant,
        // so they are unreachable at this pipeline stage.
        ExprKind::Closure(..) | ExprKind::Hole => {
            panic!("Closure and hole expressions should have been eliminated by post_defunc");
        }
    }

    ranges.exprs.push((expr_id, graph_start..builder.len()));
}
