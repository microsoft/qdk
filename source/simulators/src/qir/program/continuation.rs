// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Whether a shot of a program may continue after a batch
//! ([`continues_after_a_batch`]): send a batch of quantum calls after the
//! outcomes of an earlier one. Answered once, when the program is built,
//! and read from the [`QirProgram`](super::QirProgram).
//!
//! Shots run in batches of quantum calls. Many programs never stop before
//! their end (a Base Profile program, say, whose measurements are only
//! read once its quantum calls are done), so each of their shots is one
//! batch, a fixed circuit; others (feed-forward, a loop until success)
//! need an outcome mid-shot and send another batch after it. The target
//! that runs the calls (a simulator, a device) differs in what it can do
//! with each: one that keeps no shot state between batches (a device with
//! no mid-circuit feedback, a simulator of whole circuits) can run a
//! program whose shots never continue, but not one whose shots do. So
//! whoever chooses the target must know, before any shot, which kind the
//! program is.
//!
//! The answer is conservative: it may say yes for a program no shot of
//! which continues, but never no for one a shot of which does, so a no may
//! be relied on.
//!
//! It applies the stop rule of a [`QirMachine`](super::super::QirMachine)
//! to every path of the bytecode, whatever the values. A context yields a
//! batch before the program's end only where it stops: before an
//! instruction that reads, records or writes a result a call of its batch
//! writes
//! (`READ_RESULT`, `READ_LOSS`, the record of a result, `WRITE_RESULT`),
//! or before a call that writes a result a call of its batch already
//! writes. It yields another batch only when a call runs after such a
//! stop: any quantum instruction, correlated noise included. So a read, a
//! record or a write of any result may stop; a call that writes a result
//! may stop when another call may write the same result (a result held in
//! a register may be any), or when it may run twice (in a loop, or in a
//! function: every return is taken to go on after every call, so a
//! function called once, after another function's call, counts as called
//! twice).

use super::super::machine::block_start;
use super::super::quantum_instruction_encoding::CallOperand;
use super::prepared_quantum_calls::{CallSource, PreparedQuantumCalls};
use crate::bytecode::AdaptiveProgram;
use crate::bytecode::encoding::{
    OP_BRANCH, OP_CALL, OP_CALL_RETURN, OP_JUMP, OP_READ_LOSS, OP_READ_RESULT, OP_RECORD_OUTPUT,
    OP_RET, OP_SWITCH, OP_WRITE_RESULT,
};

/// Whether a context of `bytecode` may yield a batch after a stop before the
/// program's end: a stop from which a call may run. It may answer yes
/// where no run does: on a path no value takes, at a read of a result no
/// earlier call writes, or for code after a function call, since every
/// return is taken to go on after every call (a function called once,
/// after another function's call, counts as called twice).
#[allow(clippy::cast_possible_truncation)]
pub(super) fn continues_after_a_batch(
    bytecode: &AdaptiveProgram<u64>,
    prepared_quantum_calls: &PreparedQuantumCalls,
) -> bool {
    let graph = Graph::new(bytecode);
    let ahead = graph.reaching(|pc| prepared_quantum_calls.source(pc).is_some());
    let repeated = graph.on_a_cycle();
    let writers = Writers::new(bytecode, prepared_quantum_calls);
    bytecode
        .instructions
        .iter()
        .enumerate()
        .any(|(pc, instruction)| match instruction.primary_opcode() {
            OP_READ_RESULT | OP_READ_LOSS | OP_WRITE_RESULT => ahead[pc],
            OP_RECORD_OUTPUT if instruction.aux1 == 0 => ahead[pc],
            _ => written(prepared_quantum_calls, pc)
                .is_some_and(|written| repeated[pc] || writers.share(written)),
        })
}

/// What a call that writes a result writes.
#[derive(Clone, Copy)]
enum Written {
    /// The result of this id.
    Known(u32),
    /// A result held in a register: any.
    Any,
}

/// The calls that write results, counted by the result they write.
struct Writers {
    /// The calls writing each result named by a constant.
    known: Vec<u32>,
    /// The calls writing a result named by a constant, all results.
    all_known: u32,
    /// The calls writing a result held in a register.
    any: u32,
}

impl Writers {
    fn new(bytecode: &AdaptiveProgram<u64>, prepared_quantum_calls: &PreparedQuantumCalls) -> Self {
        let mut writers = Writers {
            known: vec![0; bytecode.num_results as usize],
            all_known: 0,
            any: 0,
        };
        for pc in 0..bytecode.instructions.len() {
            match written(prepared_quantum_calls, pc) {
                Some(Written::Known(result)) => {
                    writers.known[result as usize] += 1;
                    writers.all_known += 1;
                }
                Some(Written::Any) => writers.any += 1,
                None => {}
            }
        }
        writers
    }

    /// Whether another call may write what a call writes: a batch may
    /// then hold a call writing it when this one comes.
    fn share(&self, written: Written) -> bool {
        let others = match written {
            Written::Known(result) => self.known[result as usize] - 1 + self.any,
            Written::Any => self.all_known + self.any - 1,
        };
        others > 0
    }
}

/// What the call at `pc` writes, if it is a call that writes a result.
/// A run of prebuilt calls is counted once, at its last call, the one
/// that writes: each instruction of the run before it holds the run's
/// result too, and the graph goes through each in turn, so that the last one is
/// on every path the run is. A `WRITE_RESULT` writes a result but is no
/// call, so the analysis counts it with the reads, as a stop.
fn written(prepared_quantum_calls: &PreparedQuantumCalls, pc: usize) -> Option<Written> {
    match prepared_quantum_calls.source(pc)? {
        CallSource::Prebuilt {
            end,
            result: Some(result),
            ..
        } if *end as usize == pc + 1 => Some(Written::Known(*result)),
        CallSource::BuiltFromRegisters(call) => match call.operands.result? {
            CallOperand::Known(result) => Some(Written::Known(result)),
            CallOperand::InRegister(_) => Some(Written::Any),
        },
        CallSource::Prebuilt { .. } | CallSource::CorrelatedNoise { .. } => None,
    }
}

/// The ways a context may go from each instruction to the next, whatever
/// the values: a node per instruction, and one past them through which
/// every return goes to the instruction after every call of a function.
struct Graph {
    /// The successors of node `n`: `targets[starts[n]..starts[n + 1]]`.
    starts: Vec<usize>,
    targets: Vec<usize>,
    /// The predecessors of node `n`, likewise:
    /// `sources[source_starts[n]..source_starts[n + 1]]`.
    source_starts: Vec<usize>,
    sources: Vec<usize>,
}

impl Graph {
    #[allow(clippy::cast_possible_truncation)]
    fn new(bytecode: &AdaptiveProgram<u64>) -> Self {
        let returns = bytecode.instructions.len();
        let block = |block: u64| block_start(bytecode, block);
        let mut graph = Graph {
            starts: Vec::with_capacity(returns + 2),
            targets: Vec::new(),
            source_starts: Vec::new(),
            sources: Vec::new(),
        };
        for (pc, instruction) in bytecode.instructions.iter().enumerate() {
            graph.starts.push(graph.targets.len());
            match instruction.primary_opcode() {
                OP_RET => {}
                OP_CALL_RETURN => graph.targets.push(returns),
                OP_JUMP => graph.targets.push(block(instruction.dst)),
                OP_BRANCH => {
                    graph
                        .targets
                        .extend([block(instruction.aux0), block(instruction.aux1)]);
                }
                OP_SWITCH => {
                    graph.targets.push(block(instruction.aux0));
                    graph.targets.extend(
                        bytecode.switch_cases[instruction.aux1 as usize..]
                            [..instruction.aux2 as usize]
                            .iter()
                            .map(|case| block(case.target_block)),
                    );
                }
                OP_CALL => graph.targets.push(block(
                    bytecode.function_table[instruction.aux0 as usize].entry_block_id,
                )),
                _ => graph.targets.push(pc + 1),
            }
        }
        graph.starts.push(graph.targets.len());
        graph.targets.extend(
            bytecode
                .instructions
                .iter()
                .enumerate()
                .filter(|(_, instruction)| instruction.primary_opcode() == OP_CALL)
                .map(|(pc, _)| pc + 1),
        );
        graph.starts.push(graph.targets.len());
        graph.reverse();
        graph
    }

    /// Fills the predecessors from the successors: counted per node, then
    /// placed.
    fn reverse(&mut self) {
        let nodes = self.nodes();
        let mut source_starts = vec![0; nodes + 1];
        for &target in &self.targets {
            source_starts[target + 1] += 1;
        }
        for node in 0..nodes {
            source_starts[node + 1] += source_starts[node];
        }
        let mut next = source_starts.clone();
        let mut sources = vec![0; self.targets.len()];
        for node in 0..nodes {
            for &target in self.successors(node) {
                sources[next[target]] = node;
                next[target] += 1;
            }
        }
        self.source_starts = source_starts;
        self.sources = sources;
    }

    fn nodes(&self) -> usize {
        self.starts.len() - 1
    }

    fn successors(&self, node: usize) -> &[usize] {
        &self.targets[self.starts[node]..self.starts[node + 1]]
    }

    fn predecessors(&self, node: usize) -> &[usize] {
        &self.sources[self.source_starts[node]..self.source_starts[node + 1]]
    }

    /// Whether each instruction may reach one that `target` holds for,
    /// itself included.
    fn reaching(&self, target: impl Fn(usize) -> bool) -> Vec<bool> {
        let mut reaches = vec![false; self.nodes()];
        let mut stack: Vec<usize> = (0..self.nodes() - 1).filter(|pc| target(*pc)).collect();
        for &node in &stack {
            reaches[node] = true;
        }
        while let Some(node) = stack.pop() {
            for &predecessor in self.predecessors(node) {
                if !reaches[predecessor] {
                    reaches[predecessor] = true;
                    stack.push(predecessor);
                }
            }
        }
        reaches
    }

    /// Whether each instruction lies on a cycle of more than one node, so
    /// that a context may run it twice: the strongly connected components
    /// of more than one node (Kosaraju's algorithm). A call that writes a
    /// result goes on to the next instruction, so it is never its own
    /// successor.
    fn on_a_cycle(&self) -> Vec<bool> {
        let nodes = self.nodes();
        // The nodes in the order a depth-first walk leaves them.
        let mut order = Vec::with_capacity(nodes);
        let mut seen = vec![false; nodes];
        let mut stack: Vec<(usize, usize)> = Vec::new();
        for root in 0..nodes {
            if seen[root] {
                continue;
            }
            seen[root] = true;
            stack.push((root, 0));
            while let Some(&(node, next)) = stack.last() {
                if let Some(&successor) = self.successors(node).get(next) {
                    if let Some(top) = stack.last_mut() {
                        top.1 += 1;
                    }
                    if !seen[successor] {
                        seen[successor] = true;
                        stack.push((successor, 0));
                    }
                } else {
                    order.push(node);
                    stack.pop();
                }
            }
        }
        // Each component, from the node left last, walked backwards.
        let mut placed = vec![false; nodes];
        let mut on_a_cycle = vec![false; nodes];
        let mut component = Vec::new();
        for &root in order.iter().rev() {
            if placed[root] {
                continue;
            }
            placed[root] = true;
            component.clear();
            component.push(root);
            let mut next = 0;
            while let Some(&node) = component.get(next) {
                next += 1;
                for &predecessor in self.predecessors(node) {
                    if !placed[predecessor] {
                        placed[predecessor] = true;
                        component.push(predecessor);
                    }
                }
            }
            if component.len() > 1 {
                for &node in &component {
                    on_a_cycle[node] = true;
                }
            }
        }
        on_a_cycle
    }
}
