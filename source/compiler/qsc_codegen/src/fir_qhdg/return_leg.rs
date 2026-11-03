// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Carrying a graph-side retirement back into the FIR it was lowered from.
//!
//! # Why nothing here touches an execution graph
//!
//! FIR's executed form is an [`qsc_fir::fir::ExecGraph`], a flat sequence
//! addressed by ordinal, where jumps carry absolute targets and every
//! expression and statement stores an `exec_graph_range` into it. Deleting a
//! node there would rebase every jump above the cut and staleness every stored
//! range, twice over, because the debug and no-debug graphs run in parallel
//! behind one index.
//!
//! So the edit is made in FIR and the graphs are regenerated. That is not this
//! module's work to arrange: the FIR stage runs inside
//! `qsc_fir_transforms::run_pipeline_with_diagnostics_and_optimize`, and item
//! dead-code elimination, node garbage collection, and
//! `qsc_fir_transforms::rebuild_exec_graphs` all run after that point, over the
//! whole reachable closure. The pipeline's own `PostItemDce` and `PostAll`
//! invariant walks then check the result. A retiring edit therefore owes the
//! statement removal and nothing else, which is exactly what the insertion
//! point was chosen for.
//!
//! # Subtraction, not reconstruction
//!
//! Each node remembers the statement it was lowered from. A statement whose
//! nodes are all gone was retired; every other statement is left exactly as it
//! was. That is what makes the identity case byte-identical by construction
//! rather than by careful reproduction, on the terms the post-partial-evaluation
//! return leg set.
//!
//! # A gate call is not always a removable statement
//!
//! A gate returns unit, so the favourable case is a semicolon statement in a
//! block and removal is a list rebuild. The unfavourable cases are real and are
//! refused by name rather than removed into something well-formed and wrong: a
//! call bound to a name, a call standing as a block's value, and a call the
//! statement merely contains. The same posture the producer takes toward an
//! expression form it did not model, applied in the other direction.

use std::collections::{BTreeMap, BTreeSet};

use qsc_fir::fir::{BlockId, ExprKind, Package, PackageLookup, PackageStore, StmtKind};
use qsc_qhdg::{Graph, StableNodeId};

use super::{Lowered, StatementSite, expr_kind_name};

/// Why a graph could not be carried back into the FIR it came from.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RetireError {
    /// The graph holds a node this lowering did not build.
    ///
    /// The return leg can only remove, so a node a pass introduced has no
    /// statement to become and would vanish silently.
    #[error("the graph holds a node this lowering did not produce")]
    Introduced,
    /// A retired node was never given a statement address.
    ///
    /// A qubit declaration for a parameter and every release are built outside
    /// any statement, so retiring one names no edit. Reporting that is the only
    /// alternative to performing the retirement's opposite in silence.
    #[error("a retired node names no statement, so its removal has no address in FIR")]
    Unaddressed,
    /// Some but not all of one statement's nodes were retired.
    ///
    /// A call into a bodied callable attributes every node of that body to the
    /// one call statement, so a partial retirement asks for a body to lose one
    /// gate and keep the rest. Removing the statement would drop the others and
    /// keeping it would drop the retirement, and neither is what was asked.
    #[error("the statement at block {block} index {index} produced nodes that are still live")]
    PartiallyRetired { block: BlockId, index: usize },
    /// A recorded site does not address a statement in the package.
    #[error("block {block} index {index} does not address a statement")]
    UnknownSite { block: BlockId, index: usize },
    /// The retired call is bound to a name.
    #[error("the call at block {block} index {index} is bound to a name")]
    BoundValue { block: BlockId, index: usize },
    /// The retired call stands as the block's value.
    #[error("the call at block {block} index {index} is the block's value")]
    BlockValue { block: BlockId, index: usize },
    /// The statement is something other than a bare call.
    #[error("the statement at block {block} index {index} is '{form}' rather than a bare call")]
    NotACall {
        block: BlockId,
        index: usize,
        form: &'static str,
    },
    /// The graph omitted evaluation that could fail within this call.
    #[error(
        "the call at block {block} index {index} contains evaluation whose totality is unproved"
    )]
    RetainedEvaluation { block: BlockId, index: usize },
}

/// Carries a graph back into the FIR it was lowered from, and reports how many
/// statements that removed.
///
/// `lowered` supplies what the producer recorded; `graph` is that graph after
/// whatever ran over it. A statement every one of whose nodes is gone is
/// removed only when it carries no retained evaluation obligation; nothing
/// else is touched.
///
/// Every refusal is decided before the first write, so a store this declines is
/// a store it did not modify.
///
/// # Errors
///
/// Returns [`RetireError::Introduced`] when the graph gained a node, which is
/// outside what removal can express; [`RetireError::Unaddressed`] when a
/// retired node has no statement to remove; [`RetireError::PartiallyRetired`]
/// when one statement's nodes were retired in part; and
/// [`RetireError::UnknownSite`], [`RetireError::BoundValue`],
/// [`RetireError::BlockValue`], or [`RetireError::NotACall`] when the addressed
/// statement is not one a removal leaves well-formed. Returns
/// [`RetireError::RetainedEvaluation`] when removal could suppress a failure.
pub fn retire_into_fir(
    store: &mut PackageStore,
    graph: &Graph,
    lowered: &Lowered,
) -> Result<usize, RetireError> {
    let retired = plan(graph, lowered)?;
    let package = store.get_mut(lowered.package);
    check_removable(package, &retired)?;
    remove(package, &retired);
    Ok(retired.len())
}

/// Decides which statements the graph says are gone, refusing every difference
/// a statement removal cannot express.
fn plan(graph: &Graph, lowered: &Lowered) -> Result<BTreeSet<StatementSite>, RetireError> {
    let live: BTreeSet<StableNodeId> = graph
        .nodes()
        .filter_map(|node| graph.stable_id(node.id))
        .collect();
    if live.iter().any(|node| !lowered.built.contains(node)) {
        return Err(RetireError::Introduced);
    }
    if lowered
        .built
        .iter()
        .any(|node| !live.contains(node) && !lowered.sites.contains_key(node))
    {
        return Err(RetireError::Unaddressed);
    }

    let mut counts: BTreeMap<StatementSite, (usize, usize)> = BTreeMap::new();
    for (node, site) in &lowered.sites {
        let (alive, total) = counts.entry(*site).or_insert((0, 0));
        *total += 1;
        if live.contains(node) {
            *alive += 1;
        }
    }

    let mut retired = BTreeSet::new();
    for (site, (alive, total)) in counts {
        if alive == 0 {
            if lowered.retained_evaluation.contains(&site) {
                return Err(RetireError::RetainedEvaluation {
                    block: site.block,
                    index: site.index,
                });
            }
            retired.insert(site);
        } else if alive != total {
            return Err(RetireError::PartiallyRetired {
                block: site.block,
                index: site.index,
            });
        }
    }
    Ok(retired)
}

/// Checks every planned removal against the FIR before any of them is applied.
fn check_removable(
    package: &Package,
    retired: &BTreeSet<StatementSite>,
) -> Result<(), RetireError> {
    for site in retired {
        let block = site.block;
        let index = site.index;
        let Some(stmts) = package.blocks.get(block).map(|block| &block.stmts) else {
            return Err(RetireError::UnknownSite { block, index });
        };
        let Some(stmt) = stmts.get(index).copied() else {
            return Err(RetireError::UnknownSite { block, index });
        };
        let expr = match package.get_stmt(stmt).kind {
            StmtKind::Semi(expr) => expr,
            // A trailing expression is the block's value. A gate returns unit,
            // so the types would still agree, which is why this is refused by
            // position rather than left to typing to catch.
            StmtKind::Expr(expr) if index + 1 < stmts.len() => expr,
            StmtKind::Expr(_) => return Err(RetireError::BlockValue { block, index }),
            StmtKind::Local(..) => return Err(RetireError::BoundValue { block, index }),
            StmtKind::Item(_) => {
                return Err(RetireError::NotACall {
                    block,
                    index,
                    form: "an item declaration",
                });
            }
        };
        let kind = &package.get_expr(expr).kind;
        if !matches!(kind, ExprKind::Call(..)) {
            // The producer refuses to lower a call it found inside a larger
            // expression, so nothing it built reaches this. It guards the
            // boundary rather than a path, on the terms the producer states for
            // an expression form it did not model.
            return Err(RetireError::NotACall {
                block,
                index,
                form: expr_kind_name(kind),
            });
        }
    }
    Ok(())
}

/// Rebuilds each affected block's statement list without the retired entries.
fn remove(package: &mut Package, retired: &BTreeSet<StatementSite>) {
    let mut by_block: BTreeMap<BlockId, BTreeSet<usize>> = BTreeMap::new();
    for site in retired {
        by_block.entry(site.block).or_default().insert(site.index);
    }
    for (block, indices) in by_block {
        let kept = package
            .get_block(block)
            .stmts
            .iter()
            .enumerate()
            .filter(|(index, _)| !indices.contains(index))
            .map(|(_, stmt)| *stmt)
            .collect();
        package
            .blocks
            .get_mut(block)
            .expect("the block was resolved before any removal")
            .stmts = kept;
    }
}
