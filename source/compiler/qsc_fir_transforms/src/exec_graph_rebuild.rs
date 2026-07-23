// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Exec graph rebuild pass — the final pass in the pipeline.
//!
//! Reconstructs exec graphs from scratch for every reachable callable across
//! the entry-reachable package closure plus the entry expression. After
//! earlier passes synthesize nodes with `EMPTY_EXEC_RANGE` sentinels (return
//! unify, defunctionalize, UDT erase, tuple-compare lower, tuple-decompose,
//! argument promote), the `SpecDecl` and `Package.entry_exec_graph` graphs are
//! stale; this pass walks the FIR and re-emits the same node sequences the
//! original lowerer would produce.
//!
//! # What to know before diving in
//!
//! - **Must run last.** It relies on earlier passes having removed the
//!   expression forms the exec-graph builder treats as eliminated.
//! - **Whole-closure rebuild.** `udt_erase` structurally mutates reachable
//!   foreign callables in place, so this pass rebuilds the exec graph of every
//!   reachable spec in every reachable package — keyed off each spec's owning
//!   `package_id` — rather than only the entry package plus a narrow set of
//!   externally mutated specs.
//! - **Borrow-splitting via deferred writes.** The rebuild cannot hold
//!   `&Package` (to read exprs) and `&mut Package` (to write graphs) at once,
//!   so ranges are accumulated in `RangeUpdates` during the read-only walk and
//!   written back by `apply_ranges` afterward.
//! - **Delegates to `ExecGraphBuilder`** from `qsc_lowerer` (paired no-debug /
//!   debug node vectors) so rebuilt graphs match the original lowering format.

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

use crate::CallableSpecKind;
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
/// Invoked once per specialization. Each call writes the ranges gathered for
/// that spec back to the package before the next specialization rebuilds.
fn apply_ranges(package: &mut Package, ranges: &RangeUpdates) {
    // Each id was gathered from this same package during the read-only pass, so
    // every `.expect` below is infallible (see `get_spec_decl_mut`).
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
    kind: CallableSpecKind,
}

/// All spec infos for one callable item, collected while holding `&Package`.
struct CallableSpecs {
    package_id: PackageId,
    item_id: LocalItemId,
    specs: Vec<SpecInfo>,
}

/// Rebuilds exec graphs for every reachable callable across the entry-reachable
/// package closure and the entry expression. When `pinned_items` is non-empty,
/// uses seed-based reachability to include pinned callables that are not
/// entry-reachable.
///
/// This must be called after all FIR transforms have completed. The function
/// is idempotent — calling it multiple times produces the same result.
///
/// # Panics
///
/// Panics if reachable bodies still contain FIR variants eliminated by earlier
/// transforms, such as `ExprKind::Struct`, `ExprKind::Closure`, or
/// `ExprKind::Hole`.
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

    let collected = collect_callable_specs(store, &reachable);
    rebuild_callable_exec_graphs(store, &collected);
    rebuild_entry_exec_graph(store, package_id);
}

/// Collects the block IDs for every spec in every reachable callable across the
/// entry-reachable package closure, grouped by the callable's owning package.
fn collect_callable_specs(
    store: &PackageStore,
    reachable: &rustc_hash::FxHashSet<StoreItemId>,
) -> Vec<CallableSpecs> {
    let mut collected: Vec<CallableSpecs> = Vec::new();
    for item_id in reachable {
        let package = store.get(item_id.package);
        let item = package.get_item(item_id.item);
        let decl = match &item.kind {
            ItemKind::Callable(decl) => decl.as_ref(),
            ItemKind::Ty(..) => continue,
        };
        let specs = collect_specs_from_impl(&decl.implementation);
        if !specs.is_empty() {
            collected.push(CallableSpecs {
                package_id: item_id.package,
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
                kind: CallableSpecKind::Body,
            });
            if let Some(adj) = &spec_impl.adj {
                specs.push(SpecInfo {
                    block: adj.block,
                    kind: CallableSpecKind::Adj,
                });
            }
            if let Some(ctl) = &spec_impl.ctl {
                specs.push(SpecInfo {
                    block: ctl.block,
                    kind: CallableSpecKind::Ctl,
                });
            }
            if let Some(ctl_adj) = &spec_impl.ctl_adj {
                specs.push(SpecInfo {
                    block: ctl_adj.block,
                    kind: CallableSpecKind::CtlAdj,
                });
            }
        }
        CallableImpl::SimulatableIntrinsic(spec) => {
            specs.push(SpecInfo {
                block: spec.block,
                kind: CallableSpecKind::SimulatableIntrinsic,
            });
        }
    }
    specs
}

/// Rebuilds and writes back the exec graph for each collected callable spec.
fn rebuild_callable_exec_graphs(store: &mut PackageStore, collected: &[CallableSpecs]) {
    for callable in collected {
        for spec_info in &callable.specs {
            // Build graph — immutable borrow.
            let (graph, ranges) = {
                let package = store.get(callable.package_id);
                let mut builder = ExecGraphBuilder::default();
                let mut ranges = RangeUpdates::default();
                rebuild_block(package, &mut builder, spec_info.block, &mut ranges);
                (builder.take(), ranges)
            };

            // Write back — mutable borrow.
            let package = store.get_mut(callable.package_id);
            apply_ranges(package, &ranges);

            let target_spec = get_spec_decl_mut(package, callable.item_id, spec_info.kind);
            target_spec.exec_graph = graph;
        }
    }
}

/// Returns a mutable reference to the spec decl identified by `kind` on the
/// callable at `item_id`.
///
/// Every `.expect`/`unreachable!` below is infallible by construction: this
/// pass runs after all transforms on well-formed reachable FIR, and `kind` was
/// collected from the same store this writes back, so the item, its `Spec`
/// implementation, and the requested specialization all exist.
fn get_spec_decl_mut(
    package: &mut Package,
    item_id: LocalItemId,
    kind: CallableSpecKind,
) -> &mut FirSpecDecl {
    let item = package.items.get_mut(item_id).expect("item must exist");
    let decl = match &mut item.kind {
        ItemKind::Callable(decl) => decl.as_mut(),
        ItemKind::Ty(..) => unreachable!("already verified callable"),
    };
    match kind {
        CallableSpecKind::Body => match &mut decl.implementation {
            CallableImpl::Spec(si) => &mut si.body,
            _ => unreachable!("already verified Spec"),
        },
        CallableSpecKind::Adj => match &mut decl.implementation {
            CallableImpl::Spec(si) => si.adj.as_mut().expect("adj must exist"),
            _ => unreachable!("already verified Spec"),
        },
        CallableSpecKind::Ctl => match &mut decl.implementation {
            CallableImpl::Spec(si) => si.ctl.as_mut().expect("ctl must exist"),
            _ => unreachable!("already verified Spec"),
        },
        CallableSpecKind::CtlAdj => match &mut decl.implementation {
            CallableImpl::Spec(si) => si.ctl_adj.as_mut().expect("ctl_adj must exist"),
            _ => unreachable!("already verified Spec"),
        },
        CallableSpecKind::SimulatableIntrinsic => match &mut decl.implementation {
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

        // Residual closures are leaves because captures are local IDs, matching
        // the lowerer. Later stages resolve or reject them.
        #[allow(clippy::match_same_arms)]
        ExprKind::Closure(..) => {
            builder.push(ExecGraphNode::Expr(expr_id));
        }

        // Eliminated variant
        //
        // A hole is not valid post-defunctionalization residue, so it is
        // unreachable at this pipeline stage.
        ExprKind::Hole => {
            panic!("Hole expressions should have been eliminated by post_defunc");
        }
    }

    ranges.exprs.push((expr_id, graph_start..builder.len()));
}
