// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Carrying a graph back into the program it came from.
//!
//! This is not the shape of the circuit raise, and the difference is not an
//! accident. That direction rebuilds a circuit out of what the graph says,
//! because a circuit is entirely expressible as a sequence of operations over
//! numbered wires. RIR is not. A program also carries a callable table, a block
//! graph, variable numbering, and program-level metadata no instruction
//! references, and reconstruction would have to invent block and variable
//! numbering, which is the one thing a byte-identical round trip forbids.
//!
//! So the leg takes three things from three places. Order comes from the graph,
//! which is what lets a rewrite reorder. Block membership comes from the site
//! table, so a rewrite cannot move an instruction between blocks. The
//! instructions themselves, and everything the program carries that no
//! instruction references, are copied from the original. That is what makes the
//! identity case byte-identical by construction rather than by careful
//! reproduction, and it is why doubles survive exactly: a literal is never
//! re-derived, so the epsilon comparison in RIR's own `PartialEq` never gets a
//! chance to matter.
//!
//! Emission fails differently from the subtraction this replaced. Subtraction
//! could not invent an instruction and could not lose one no rewrite touched;
//! emission can do both, silently, on any program the corpus does not hold.
//! [`emitted_exactly_once`] is the guard that does not depend on which programs
//! someone thought to write down.
//!
//! Retirement needs no wire re-threading on this side. RIR addresses a qubit by
//! index rather than by value, so deleting an instruction leaves every later
//! reference to that qubit still correct. The re-threading the graph rewrite
//! did is exactly what the graph needed and the program does not.

use std::collections::{BTreeMap, BTreeSet};

use qsc_qhdg::{Graph, StableNodeId};
use qsc_rir::rir::{Block, BlockId, CallableId, Instruction, Program};

use super::{InstructionSite, Lowered, block_order};

/// Why a graph could not be carried back into its program.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RetireError {
    /// The graph holds a node this lowering did not build.
    ///
    /// Emission places a node by the instruction its site names, so a node a
    /// pass introduced has no instruction to become and would vanish silently.
    #[error("the graph holds a node this lowering did not produce")]
    Introduced,
    /// A retired instruction defines a value some other instruction may read.
    #[error("the instruction retired at block {block} index {index} defines a value")]
    DefinesValue { block: u32, index: usize },
    /// The graph does not linearize, so there is no order to emit in.
    #[error("the graph has no program order: {reason}")]
    Unordered { reason: String },
    /// A represented instruction was emitted more or fewer times than once.
    ///
    /// Emission can lose an instruction and can repeat one, and both are silent
    /// on any program the corpus does not hold. Subtraction could do neither,
    /// which is why this check arrives with emission.
    #[error("block {block} index {index} was emitted {count} times rather than once")]
    NotEmittedOnce {
        block: u32,
        index: usize,
        count: usize,
    },
}

/// Carries a graph back into the program it was lowered from.
///
/// `lowered` supplies what the producer recorded; `graph` is that graph after
/// whatever ran over it. Output order is the graph's, which is what lets a
/// rewrite reorder; block membership and the instructions themselves are still
/// the original program's, which is what keeps an untouched program identical.
///
/// # Errors
///
/// Returns [`RetireError::Introduced`] when the graph gained a node,
/// [`RetireError::DefinesValue`] when a retired instruction defines a value
/// another instruction could read, [`RetireError::Unordered`] when the graph
/// does not linearize, and [`RetireError::NotEmittedOnce`] when emission does
/// not account for a represented instruction exactly once.
pub fn graph_to_program(
    original: &Program,
    graph: &Graph,
    lowered: &Lowered,
) -> Result<Program, RetireError> {
    let order = block_order(graph, &lowered.sites, BTreeSet::pop_first).map_err(|error| {
        RetireError::Unordered {
            reason: error.to_string(),
        }
    })?;
    emit(original, graph, lowered, &order)
}

/// Emits a program in the order it is handed.
///
/// Separate from [`graph_to_program`] so a test can hand it an order the graph
/// did not produce. An emission leg that ignored its order would return the
/// same program either way, which is exactly what subtraction did and what this
/// replaces.
pub(crate) fn emit(
    original: &Program,
    graph: &Graph,
    lowered: &Lowered,
    order: &BTreeMap<BlockId, Vec<StableNodeId>>,
) -> Result<Program, RetireError> {
    let live: BTreeSet<StableNodeId> = graph
        .nodes()
        .filter_map(|node| graph.stable_id(node.id))
        .collect();
    if live.iter().any(|node| !lowered.built.contains(node)) {
        return Err(RetireError::Introduced);
    }

    // An instruction survives while any one of its nodes does. A measurement
    // becomes a node plus one store per result, and keeping it whenever part of
    // it remains is the direction that cannot lose a program.
    let mut alive: BTreeMap<BlockId, BTreeMap<usize, bool>> = BTreeMap::new();
    for (node, site) in &lowered.sites {
        let entry = alive
            .entry(site.block)
            .or_default()
            .entry(site.index)
            .or_insert(false);
        *entry |= live.contains(node);
    }

    let mut program = original.clone();
    let mut emitted: Vec<InstructionSite> = Vec::new();
    let mut retired: BTreeSet<InstructionSite> = BTreeSet::new();
    for (block, indices) in &alive {
        let body = &original.get_block(*block).0;
        for (index, alive) in indices {
            if *alive {
                continue;
            }
            retired.insert(InstructionSite {
                block: *block,
                index: *index,
            });
            // A gate defines nothing, so this never fires for the retirements
            // this stage admits. It fires if the set ever widens, which is the
            // point: a dropped definition leaves its readers dangling.
            if body.get(*index).is_none_or(defines_value) {
                return Err(RetireError::DefinesValue {
                    block: block.0,
                    index: *index,
                });
            }
        }

        let mut kept = Vec::new();
        let mut placed = BTreeSet::new();
        for node in order.get(block).into_iter().flatten() {
            let Some(site) = lowered.sites.get(node) else {
                continue;
            };
            // An instruction with several nodes lands where its first one does.
            if !live.contains(node) || !placed.insert(site.index) {
                continue;
            }
            let Some(instruction) = body.get(site.index) else {
                continue;
            };
            kept.push(instruction.clone());
            emitted.push(*site);
        }
        // The graph holds no terminator node. `lower_chain` splits one off
        // before lowering and a block cannot lose the instruction that says
        // where control goes next, so it is carried by position.
        if let Some(terminator) = body.last() {
            kept.push(terminator.clone());
        }
        program.blocks.insert(*block, Block(kept));
    }

    emitted_exactly_once(&lowered.represented, &retired, &emitted)?;
    prune_orphaned_callables(&mut program);
    Ok(program)
}

/// Whether emission accounted for every represented instruction exactly once.
///
/// Stated over caller-visible instructions rather than over nodes. A bodied
/// call maps many nodes to one site, so a node-counting form of this would
/// report duplication on every program holding a bodied callable.
pub(crate) fn emitted_exactly_once(
    represented: &BTreeSet<InstructionSite>,
    retired: &BTreeSet<InstructionSite>,
    emitted: &[InstructionSite],
) -> Result<(), RetireError> {
    let mut counts: BTreeMap<InstructionSite, usize> = BTreeMap::new();
    for site in emitted {
        *counts.entry(*site).or_default() += 1;
    }
    let short = represented
        .iter()
        .filter(|site| !retired.contains(site))
        .map(|site| (*site, counts.get(site).copied().unwrap_or(0)));
    let repeated = counts.iter().map(|(site, count)| (*site, *count));
    for (site, count) in short.chain(repeated) {
        if count != 1 {
            return Err(RetireError::NotEmittedOnce {
                block: site.block.0,
                index: site.index,
                count,
            });
        }
    }
    Ok(())
}

/// Drops the callables and blocks the retirement left unreachable.
///
/// RIR requires every declared callable to be reachable from the entry, and
/// `check_unreachable_code` asserts it. Removing the last call to a gate leaves
/// its declaration behind, so retiring an instruction and keeping the callable
/// table intact are not both possible.
///
/// Reachability is walked rather than approximated by collecting every call
/// anywhere, because the producer now walks bodied callables: a call sitting in
/// the body of a callable that itself became unreachable would otherwise keep
/// its callee alive.
fn prune_orphaned_callables(program: &mut Program) {
    let mut callables: BTreeSet<CallableId> = BTreeSet::from([program.entry]);
    let mut blocks: BTreeSet<BlockId> = BTreeSet::new();
    let mut pending: Vec<BlockId> = program
        .get_callable(program.entry)
        .body
        .into_iter()
        .collect();
    while let Some(block) = pending.pop() {
        if !blocks.insert(block) {
            continue;
        }
        for instruction in &program.get_block(block).0 {
            match instruction {
                Instruction::Call(callable, ..) => {
                    if callables.insert(*callable)
                        && let Some(body) = program.get_callable(*callable).body
                    {
                        pending.push(body);
                    }
                }
                Instruction::Jump(next) => pending.push(*next),
                Instruction::Branch(_, then_block, else_block, _) => {
                    pending.push(*then_block);
                    pending.push(*else_block);
                }
                _ => {}
            }
        }
    }
    program
        .callables
        .retain(|callable, _| callables.contains(&callable));
    program.blocks.retain(|block, _| blocks.contains(&block));
}

/// Whether an instruction binds a variable another instruction could read.
fn defines_value(instruction: &Instruction) -> bool {
    match instruction {
        Instruction::Call(_, _, output, _) => output.is_some(),
        Instruction::Jump(..) | Instruction::Branch(..) | Instruction::Return(..) => false,
        _ => true,
    }
}
