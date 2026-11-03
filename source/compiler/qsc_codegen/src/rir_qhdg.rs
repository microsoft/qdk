// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering post-partial-evaluation RIR into the QHDG graph.
//!
//! This producer shares the circuit and FIR producers' graph vocabulary.
//! Differences in source representation do not introduce new gate names.
//!
//! RIR addresses a qubit by literal index below the Adaptive profile and reuses
//! an index freely, so the register-to-linear bridge is the same one the
//! circuit producer builds: each instruction consumes the capability standing
//! for each qubit it touches and produces a fresh one, and an index becomes a
//! chain rather than a name repeated.
//!
//! What differs is control flow. A circuit carries a structured conditional; a
//! block graph does not, so this module reconstructs one: a result-conditional
//! branch becomes a gamma, and a branch one of whose arms comes back to it
//! becomes a theta.
//!
//! A back edge is what makes the block graph stop being a DAG, and the join
//! search rests on its being one. So the back edge is removed from the search
//! rather than tolerated by it: while a loop is being lowered its header is not
//! a successor of anything, which leaves the blocks inside one iteration a DAG
//! again and keeps the join exact rather than a guess.
//!
//! A call into a callable that has a body is walked at the call site rather
//! than once. At the Adaptive profile that is not an optimization, it is the
//! only way in: every quantum operation arrives wrapped in an IR function, so
//! a producer that refuses a bodied callable refuses the whole profile. Walking
//! at the call site is also what keeps a qubit's identity, because the callee
//! names a parameter and only the call site says which wire that parameter
//! holds.
//!
//! The stage this consumes is fixed: the program as it stands after
//! `check_and_transform`'s common prefix and before its profile branch. Control
//! flow is simplified, types are checked, and block IDs are stable, while the
//! program form is still uniform across profiles.

#[cfg(test)]
mod tests;

mod return_leg;

pub use return_leg::{RetireError, graph_to_program};

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use qsc_qhdg::vocabulary::{
    ALLOCATE, BARRIER, CARRY, CLBIT, GAMMA, OPAQUE, RELAY, RELEASE, RESET, STORE, THETA,
};
use qsc_qhdg::{
    Direction, EdgeKind, EffectDomain, EffectEvent, EffectMode, EffectResource, ExitKind, Graph,
    GraphBuilder, InterfaceBinding, InterfaceRole, NodeId, NodeOrigin, NodePayload, NodeValue,
    Operand as Wire, OperandWires, PortId, PortKind, RegionId, RegionKind, ResourceId,
    StableNodeId, Stage, Type, Usage, verify,
};
use qsc_rir::rir::{
    BlockId, CallableId, CallableType, Instruction, Literal, Operand, Prim, Program, Ty, Variable,
    VariableId,
};

use crate::graph_order::{OrderError, region_orders_with};

/// Why a program could not be lowered into a graph.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("callable '{name}' has no graph form")]
    UnsupportedCallable { name: String },
    #[error("the graph could not be built: {reason}")]
    Build { reason: String },
    #[error("block {block} does not end with a terminator")]
    MissingTerminator { block: u32 },
    #[error("an operation names qubit {qubit}, which the program does not declare")]
    UnknownQubit { qubit: u32 },
    #[error("a qubit operand did not resolve to a static index")]
    DynamicQubit,
    #[error("callable '{name}' is reached from inside its own body")]
    RecursiveCallable { name: String },
    #[error("an operation names the same qubit more than once")]
    RepeatedQubit,
    #[error("a branch reads a value no measurement in scope produced")]
    UnknownCondition,
    #[error("the program's entry point has no body")]
    NoEntryBody,
    #[error("a branch arm does not rejoin, or a back edge reaches a block no branch heads")]
    Unstructured,
    #[error("a loop is outside the shape this stage repeats: {reason}")]
    UnsupportedLoop { reason: String },
    #[error("'{kind}' cannot reach this stage, and ordering one would be a guess")]
    UnreachableKind { kind: &'static str },
    #[error("a value of type {ty} has no port type, so it cannot be carried")]
    UncarriableType { ty: String },
    #[error("variable {variable} is written inside a branch arm, and nothing merges the arms")]
    BranchWrite { variable: u32 },
    #[error(
        "block {block} index {index} gained no node, so the graph is not the whole program: {instruction}"
    )]
    Unrepresented {
        block: u32,
        index: usize,
        instruction: String,
    },
    #[error("the graph has no program order: {reason}")]
    Unordered { reason: String },
    #[error("the lowered graph failed its own verification: {0}")]
    Verification(String),
}

impl From<OrderError> for Error {
    fn from(error: OrderError) -> Self {
        Error::Unordered {
            reason: error.to_string(),
        }
    }
}

/// Where an instruction sits in the program a node was lowered from.
///
/// A block and an index rather than the node's `NodeOrigin`, which is a flat
/// counter over lowered calls and so cannot name a position in a block.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct InstructionSite {
    pub block: BlockId,
    pub index: usize,
}

/// A lowered program and the instruction each of its nodes came from.
///
/// Keyed by durable identity, so the table still answers after a rewrite: a
/// stable identity is never reissued, and a retired node's absence from the
/// graph is exactly the fact the return leg reads.
pub struct Lowered {
    pub graph: Graph,
    pub sites: BTreeMap<StableNodeId, InstructionSite>,
    /// Every instruction the graph is supposed to hold a node for.
    ///
    /// The represented set is every non-terminator instruction in a
    /// caller-visible block. A terminator is exempt because `lower_chain`
    /// splits it off and a block cannot lose one; an instruction inside a
    /// callee block is exempt because the bodied call that reached it carries
    /// it, which is why nodes built there take the caller's site.
    pub represented: BTreeSet<InstructionSite>,
    /// Every node this lowering produced, so the return leg can tell a node it
    /// removed from one something else added.
    pub built: BTreeSet<StableNodeId>,
    pub census: IdentityCensus,
}

/// How the qubit operands in one program resolved through the shared wire
/// abstraction.
///
/// Reported whether or not lowering succeeded, so a decline can be attributed
/// to identity rather than inferred later from an absence of benefit. A
/// producer that is correct and useless looks exactly like a producer that is
/// correct and useful until someone counts.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct IdentityCensus {
    /// Operands resolved to a durable identity, which is what lowers.
    pub resource: usize,
    /// Operands the producer could name but not resolve.
    pub binding: usize,
    /// Operands the producer could put in neither form.
    pub opaque: usize,
}

/// Lowers a program into a graph, refusing to return one that does not verify.
///
/// # Errors
///
/// Returns an error when the program holds a construct this stage does not
/// accept, when the graph cannot be assembled, or when the assembled graph
/// fails the verification the graph crate applies to its own programs.
pub fn program_to_graph(program: &Program) -> Result<Graph, Error> {
    program_to_graph_with_sites(program).map(|lowered| lowered.graph)
}

/// Lowers a program and reports which instruction each node came from.
///
/// # Errors
///
/// Returns the same errors as [`program_to_graph`].
pub fn program_to_graph_with_sites(program: &Program) -> Result<Lowered, Error> {
    lower_with_census(program).0
}

/// Lowers a program and reports how its qubit operands resolved either way.
///
/// The census is returned alongside a refusal as well as alongside a graph,
/// because the interesting number is how often identity declined, and a
/// program that refused is exactly where that number would otherwise be lost.
pub fn lower_with_census(program: &Program) -> (Result<Lowered, Error>, IdentityCensus) {
    let (built, census) = Lowering::new(program).run();
    let lowered = built.and_then(|(graph, sites, represented)| {
        let diagnostics = verify(&graph);
        if !diagnostics.is_empty() {
            let reported = diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code.as_str(), diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(Error::Verification(reported));
        }
        let sites: BTreeMap<StableNodeId, InstructionSite> = sites
            .into_iter()
            .filter_map(|(node, site)| Some((graph.stable_id(node)?, site)))
            .collect();
        if let Some(site) = unrepresented(&represented, sites.values().copied()) {
            return Err(Error::Unrepresented {
                block: site.block.0,
                index: site.index,
                instruction: program.get_block(site.block).0[site.index]
                    .to_string()
                    .trim()
                    .to_string(),
            });
        }
        let built = graph
            .nodes()
            .filter_map(|node| graph.stable_id(node.id))
            .collect();
        Ok(Lowered {
            graph,
            sites,
            represented,
            built,
            census,
        })
    });
    (lowered, census)
}

/// The first instruction the graph was supposed to represent and does not.
///
/// The guard on the failure mode emission has and subtraction did not.
/// Subtraction could not drop an instruction no rewrite touched; emission can,
/// silently, on any program the corpus does not hold. This holds on every
/// program instead of on the ones someone thought to write down.
fn unrepresented(
    represented: &BTreeSet<InstructionSite>,
    sites: impl IntoIterator<Item = InstructionSite>,
) -> Option<InstructionSite> {
    let covered: BTreeSet<InstructionSite> = sites.into_iter().collect();
    represented.difference(&covered).next().copied()
}

/// A graph, the instruction position each of its nodes was built at, and the
/// instructions the graph is supposed to represent.
type Built = (
    Graph,
    BTreeMap<NodeId, InstructionSite>,
    BTreeSet<InstructionSite>,
);

/// The order the graph says a lowered program runs its nodes in, per block.
///
/// The order is the graph's rather than the original program's, which is what
/// a rewrite that reorders needs and what subtracting against the original
/// instruction list cannot give. Block membership is still the original
/// program's answer, read out of [`Lowered::sites`]: this makes order
/// graph-derived, not block assignment.
///
/// Nodes with no site are dropped here. A capability declaration and a region
/// owner are graph bookkeeping rather than instructions, and the caller asks
/// this question about instructions.
///
/// # Errors
///
/// Returns [`Error::Unordered`] when the graph does not linearize.
pub fn program_order(lowered: &Lowered) -> Result<BTreeMap<BlockId, Vec<StableNodeId>>, Error> {
    block_order(&lowered.graph, &lowered.sites, BTreeSet::pop_first)
}

/// The same question asked of a graph and a site table directly.
///
/// The return leg holds a graph a rewrite may have changed and a site table
/// from before it, so it cannot go through [`Lowered`]. The tie-break is a
/// parameter for the same reason it is one in the ordering module: a test
/// perturbs it to show that emission follows the order rather than the input.
pub(crate) fn block_order(
    graph: &Graph,
    sites: &BTreeMap<StableNodeId, InstructionSite>,
    next_ready: fn(&mut BTreeSet<StableNodeId>) -> Option<StableNodeId>,
) -> Result<BTreeMap<BlockId, Vec<StableNodeId>>, Error> {
    let orders = region_orders_with(graph, next_ready)?;
    let mut blocks: BTreeMap<BlockId, Vec<StableNodeId>> = BTreeMap::new();
    for id in compose(graph, &orders) {
        if let Some(site) = sites.get(&id) {
            blocks.entry(site.block).or_default().push(id);
        }
    }
    Ok(blocks)
}

/// One order for the whole program out of one order per region.
///
/// A region's contents follow the node that owns it, because that is where they
/// run: a gamma arm runs at the branch, not before it and not after everything
/// else. Regions are not blocks, so this says nothing about which block a node
/// lands in.
fn compose(graph: &Graph, orders: &BTreeMap<RegionId, Vec<StableNodeId>>) -> Vec<StableNodeId> {
    let mut bodies: BTreeMap<StableNodeId, Vec<RegionId>> = BTreeMap::new();
    let mut module = None;
    for region in graph.regions() {
        match region.owner.and_then(|owner| graph.stable_id(owner)) {
            Some(owner) => bodies.entry(owner).or_default().push(region.id),
            None if region.kind == RegionKind::ModuleOmega => module = Some(region.id),
            None => {}
        }
    }

    let mut composed = Vec::new();
    let mut walked = BTreeSet::new();
    if let Some(module) = module {
        splice(module, orders, &bodies, &mut walked, &mut composed);
    }
    composed
}

fn splice(
    region: RegionId,
    orders: &BTreeMap<RegionId, Vec<StableNodeId>>,
    bodies: &BTreeMap<StableNodeId, Vec<RegionId>>,
    walked: &mut BTreeSet<RegionId>,
    composed: &mut Vec<StableNodeId>,
) {
    // A region reached twice would be a cycle in the owner relation, which the
    // walk answers by stopping rather than by repeating its contents.
    if !walked.insert(region) {
        return;
    }
    for id in orders.get(&region).into_iter().flatten() {
        composed.push(*id);
        for inner in bodies.get(id).into_iter().flatten() {
            splice(*inner, orders, bodies, walked, composed);
        }
    }
}

struct Lowering<'a> {
    program: &'a Program,
    builder: GraphBuilder,
    region: RegionId,
    /// The capability standing for each qubit as the walk proceeds.
    live: Vec<PortId>,
    /// The port each result index was last written from.
    result_source: BTreeMap<u32, PortId>,
    /// Qubits whose live capability is still outside the region being built.
    crossings: BTreeSet<u32>,
    /// What a crossing declares in the region currently being built.
    ///
    /// An arm takes its wire from the enclosing region once; a loop body takes
    /// it from there on the first iteration and from its own back edge on
    /// every later one, which is a different claim and a different role.
    crossing_role: InterfaceRole,
    /// The port a loop body first took each carried qubit at, so the back edge
    /// knows where to return it.
    carried_entry: BTreeMap<u32, PortId>,
    /// The headers of the loops currently being lowered.
    ///
    /// A block graph containing a back edge is not a DAG, and every join this
    /// stage computes assumes one. Holding the headers here is what removes the
    /// back edge from the search instead of weakening the search.
    loop_headers: BTreeSet<BlockId>,
    /// How many nodes this lowering has built, so a block can be asked whether
    /// it contributed any.
    ///
    /// Maintained with `carriers_built` at the one site that builds a node, so
    /// the two cannot drift apart.
    nodes_built: usize,
    /// How many of those were carriers, which a loop header may hold.
    carriers_built: usize,
    /// Blocks already lowered, so a back edge is refused rather than followed.
    ///
    /// Scoped to one body walk. A body reached from two call sites is walked
    /// twice, which is not a cycle; a call reaching itself is, and the call
    /// stack is what catches that.
    visited: BTreeSet<BlockId>,
    /// The wire each callee parameter stands for, for the body being walked.
    frame: BTreeMap<VariableId, Wire>,
    /// Callables currently being walked, innermost last.
    call_stack: Vec<CallableId>,
    /// Variables the program assigns exactly once.
    singly_assigned: BTreeSet<VariableId>,
    /// The port each variable was last read or written at, and the region that
    /// port sits in.
    ///
    /// One chain per variable covering every access in source order. Value
    /// dataflow alone orders none of two writes, a read before a later write,
    /// or a loop-carried update, because this stage runs before the SSA
    /// transform and a variable here is not one value.
    access_of: BTreeMap<VariableId, (PortId, RegionId)>,
    /// The port a loop body first took each carried variable at, so the back
    /// edge knows where to return it.
    carried_values: BTreeMap<VariableId, PortId>,
    /// How many branch arms enclose the walk, so a write inside one is refused.
    arm_depth: usize,
    census: IdentityCensus,
    /// A running position, so every node carries a distinct origin.
    position: u32,
    /// The instruction currently being lowered, and the nodes it produced.
    site: Option<InstructionSite>,
    sites: BTreeMap<NodeId, InstructionSite>,
    /// The caller-visible instructions the graph owes a node.
    represented: BTreeSet<InstructionSite>,
}

impl<'a> Lowering<'a> {
    fn new(program: &'a Program) -> Self {
        let mut builder = GraphBuilder::new();
        let region = builder.add_region(None, RegionKind::ModuleOmega);
        Self {
            program,
            builder,
            region,
            live: Vec::new(),
            result_source: BTreeMap::new(),
            crossings: BTreeSet::new(),
            crossing_role: InterfaceRole::Argument,
            carried_entry: BTreeMap::new(),
            loop_headers: BTreeSet::new(),
            nodes_built: 0,
            carriers_built: 0,
            visited: BTreeSet::new(),
            frame: BTreeMap::new(),
            call_stack: Vec::new(),
            singly_assigned: singly_assigned(program),
            access_of: BTreeMap::new(),
            carried_values: BTreeMap::new(),
            arm_depth: 0,
            census: IdentityCensus::default(),
            position: 0,
            site: None,
            sites: BTreeMap::new(),
            represented: BTreeSet::new(),
        }
    }

    fn run(mut self) -> (Result<Built, Error>, IdentityCensus) {
        let outcome = self.lower_program();
        let census = self.census;
        match outcome {
            Ok(()) => (
                Ok((self.builder.finish(), self.sites, self.represented)),
                census,
            ),
            Err(error) => (Err(error), census),
        }
    }

    fn lower_program(&mut self) -> Result<(), Error> {
        self.declare_widths()?;
        let entry = self
            .program
            .get_callable(self.program.entry)
            .body
            .ok_or(Error::NoEntryBody)?;
        self.lower_chain(entry, None)?;
        self.release_qubits()
    }

    /// Records the program's widths, which no instruction carries.
    fn declare_widths(&mut self) -> Result<(), Error> {
        for qubit in 0..self.program.num_qubits {
            let node = self.node(
                ALLOCATE,
                vec![qubit_effect(EffectMode::Allocate, false, qubit)],
                NodeOrigin::Qubit(qubit),
            )?;
            let port = self.port(node, Direction::Output, PortKind::Capability, Type::Qubit)?;
            self.live.push(port);
        }
        for result in 0..self.program.num_results {
            let node = self.node(CLBIT, Vec::new(), NodeOrigin::Clbit(result))?;
            self.port(node, Direction::Output, PortKind::Value, Type::Result)?;
        }
        Ok(())
    }

    /// Consumes every capability still live, so none dangles.
    fn release_qubits(&mut self) -> Result<(), Error> {
        for index in 0..self.live.len() {
            let qubit = u32::try_from(index).expect("a program's qubit count is a u32");
            let node = self.node(
                RELEASE,
                vec![qubit_effect(EffectMode::Release, false, qubit)],
                NodeOrigin::Qubit(qubit),
            )?;
            let input = self.port(node, Direction::Input, PortKind::Capability, Type::Qubit)?;
            self.connect(self.live[index], input, EdgeKind::AffineCapability)?;
        }
        Ok(())
    }

    /// Walks blocks from `start` until reaching `stop` or a return.
    fn lower_chain(&mut self, start: BlockId, stop: Option<BlockId>) -> Result<(), Error> {
        let mut current = start;
        loop {
            if Some(current) == stop {
                return Ok(());
            }
            if !self.visited.insert(current) {
                return Err(Error::Unstructured);
            }
            let body = self.program.get_block(current).0.clone();
            let (terminator, instructions) = body
                .split_last()
                .ok_or(Error::MissingTerminator { block: current.0 })?;
            let built_before = self.nodes_built - self.carriers_built;
            for (index, instruction) in instructions.iter().enumerate() {
                // A node built inside a callee body belongs to the call that
                // reached it, not to the body instruction, which two call
                // sites would otherwise share one site for.
                let outer = self.site;
                let site = InstructionSite {
                    block: current,
                    index,
                };
                if outer.is_none() {
                    self.represented.insert(site);
                }
                self.site = outer.or(Some(site));
                let lowered = self.lower_instruction(instruction);
                self.site = outer;
                lowered?;
            }
            match terminator {
                Instruction::Jump(next) => current = *next,
                Instruction::Return(_) => return Ok(()),
                Instruction::Branch(condition, then_block, else_block, _) => {
                    if let Some((body, exit)) =
                        self.loop_shape(current, *then_block, *else_block)?
                    {
                        // The condition runs once more than the body does, so a
                        // node built here would sit either inside a region that
                        // repeats it too few times or outside one that repeats
                        // it not at all. Neither is the program.
                        //
                        // A carrier is exempt, and the exemption has a
                        // precondition. Nothing interprets a carrier and
                        // emission places it by the block its site names, so
                        // its region makes no claim about when it runs. The day
                        // anything reads a carrier or counts iterations off the
                        // region tree, the header needs a region that repeats
                        // with it and this exemption has to go.
                        if self.nodes_built - self.carriers_built != built_before {
                            return Err(Error::UnsupportedLoop {
                                reason: "its condition holds an operation this stage would have to place"
                                    .to_string(),
                            });
                        }
                        current = self.lower_loop(current, body, exit)?;
                    } else {
                        let predicate = self.predicate(instructions, condition.variable_id)?;
                        match self.lower_conditional(predicate, *then_block, *else_block)? {
                            Some(join) => current = join,
                            None => return Ok(()),
                        }
                    }
                }
                _ => {
                    return Err(Error::MissingTerminator { block: current.0 });
                }
            }
        }
    }

    /// The port a branch reads its predicate from.
    ///
    /// Resolved by walking back through the block that ends in the branch,
    /// rather than by carrying a variable-to-port map across regions. A result
    /// read is the only source this stage accepts, and the comparison against
    /// one that partial evaluation emits around it carries nothing the result
    /// does not already carry, so this reads through it.
    fn predicate(
        &self,
        instructions: &[Instruction],
        mut wanted: VariableId,
    ) -> Result<PortId, Error> {
        for instruction in instructions.iter().rev() {
            match instruction {
                Instruction::Call(callable, args, Some(out), _)
                    if out.variable_id == wanted
                        && self.program.get_callable(*callable).call_type
                            == CallableType::Readout =>
                {
                    let Some(Operand::Literal(Literal::Result(result))) = args.first() else {
                        return Err(Error::UnknownCondition);
                    };
                    return self
                        .result_source
                        .get(result)
                        .copied()
                        .ok_or(Error::UnknownCondition);
                }
                Instruction::Store(Operand::Variable(source), target)
                    if target.variable_id == wanted =>
                {
                    wanted = source.variable_id;
                }
                Instruction::Icmp(_, Operand::Variable(source), _, target)
                | Instruction::Icmp(_, _, Operand::Variable(source), target)
                    if target.variable_id == wanted =>
                {
                    wanted = source.variable_id;
                }
                _ => {}
            }
        }
        Err(Error::UnknownCondition)
    }

    fn lower_instruction(&mut self, instruction: &Instruction) -> Result<(), Error> {
        let Instruction::Call(callable, args, output, _) = instruction else {
            let position = self.next_position();
            return self.lower_carrier(instruction, position);
        };
        let id = *callable;
        let callable = self.program.get_callable(id);
        if callable.body.is_some() {
            return self.lower_call_into_body(id, args, output.as_ref());
        }
        let position = self.next_position();
        match callable.call_type {
            // Recording a value and reading a result carry no quantum effect
            // and thread no capability, so they are carried rather than
            // interpreted. A readout also defines the variable a branch reads,
            // which the chain has to order like any other write.
            CallableType::OutputRecording | CallableType::Readout => {
                self.lower_carrier(instruction, position)
            }
            CallableType::Measurement => {
                // Dispatching on the call type alone would lower `mresetz` as
                // a plain measurement and drop its reset. The name decides,
                // and a measurement the vocabulary has no term for is refused.
                let operation =
                    measurement_name(&callable.name).ok_or_else(|| Error::UnsupportedCallable {
                        name: callable.name.clone(),
                    })?;
                self.lower_measurement(operation, args, position)
            }
            // A reset destroys information, so it is observable and the
            // retention rule must not treat it as removable.
            CallableType::Reset => self.lower_gate(RESET, args, position, true),
            CallableType::Regular => match operation_name(&callable.name) {
                Some(operation) => self.lower_gate(operation, args, position, false),
                // A runtime call such as `__quantum__rt__initialize` names no
                // qubit, so it threads no capability and is carried. One that
                // did name a qubit would be a gate this stage cannot see, and
                // reordering around it would be unsound, so it still refuses.
                None if self.qubits_of(args)?.is_empty() => {
                    self.lower_carrier(instruction, position)
                }
                None => Err(Error::UnsupportedCallable {
                    name: callable.name.clone(),
                }),
            },
            CallableType::NoiseIntrinsic => Err(Error::UnsupportedCallable {
                name: callable.name.clone(),
            }),
        }
    }

    /// Represents an instruction the graph carries without interpreting.
    ///
    /// The node says an instruction is here and where it sits in the order, and
    /// nothing more. What the instruction means stays in the program it was
    /// lowered from, and the return leg copies it back from there.
    fn lower_carrier(&mut self, instruction: &Instruction, position: u32) -> Result<(), Error> {
        // Created only by passes that run after this stage's hook, so a program
        // holding one arrived by a route this stage has not been reasoned
        // about. Refusing costs a match arm; ordering one would be a guess.
        let unreachable = match instruction {
            Instruction::Alloca(..) => Some("Alloca"),
            Instruction::Load(..) => Some("Load"),
            Instruction::Phi(..) => Some("Phi"),
            _ => None,
        };
        if let Some(kind) = unreachable {
            return Err(Error::UnreachableKind { kind });
        }

        let (read, written) = accesses(instruction);
        if self.arm_depth > 0
            && let Some(target) = written
        {
            // Both arms would write it and nothing joins them, because `Phi` is
            // refused and SSA has not run, so there is no merge to read after.
            return Err(Error::BranchWrite {
                variable: target.variable_id.0,
            });
        }

        let node = self.node(OPAQUE, Vec::new(), NodeOrigin::Instruction(position))?;
        let mut threaded = BTreeSet::new();
        for variable in read.into_iter().chain(written) {
            if threaded.insert(variable.variable_id) {
                self.chain(node, variable)?;
            }
        }
        Ok(())
    }

    /// Threads one variable through a carrier, ordering this access after the
    /// one before it.
    ///
    /// An access with nothing before it starts the chain rather than refusing:
    /// a program may branch on a variable no instruction in it defines.
    ///
    /// A qubit-typed variable is left out. Its ordering is the capability chain
    /// already threading that qubit, and a value port carrying a qubit would be
    /// linear, so a second read of one would trip the fan-out rule.
    fn chain(&mut self, node: NodeId, variable: Variable) -> Result<(), Error> {
        if is_qubit(variable.ty) {
            return Ok(());
        }
        let ty = port_type(variable.ty).ok_or_else(|| Error::UncarriableType {
            ty: variable.ty.to_string(),
        })?;
        if let Some((source, region)) = self.access_of.get(&variable.variable_id).copied() {
            let input = self.port(node, Direction::Input, PortKind::Value, ty.clone())?;
            if region == self.region {
                self.connect(source, input, EdgeKind::CopyableValue)?;
            } else {
                self.bind(self.region, source, input, self.crossing_role)?;
                if self.crossing_role == InterfaceRole::Carried {
                    // The one edge this port admits is the back edge that
                    // brings the value back. The binding says where it came
                    // from before the first iteration.
                    self.carried_values
                        .entry(variable.variable_id)
                        .or_insert(input);
                } else {
                    self.connect(source, input, EdgeKind::CopyableValue)?;
                }
            }
        }
        let output = self.port(node, Direction::Output, PortKind::Value, ty)?;
        self.access_of
            .insert(variable.variable_id, (output, self.region));
        Ok(())
    }

    /// Lowers one operation that threads capabilities and produces no result.
    fn lower_gate(
        &mut self,
        operation: &str,
        args: &[Operand],
        position: u32,
        observable: bool,
    ) -> Result<(), Error> {
        let qubits = self.qubits_of(args)?;
        let effects = if observable {
            qubits
                .iter()
                .map(|qubit| qubit_effect(EffectMode::Irreversible, true, *qubit))
                .collect()
        } else {
            vec![effect(EffectMode::Write, false)]
        };
        let node = self.node(operation, effects, NodeOrigin::Instruction(position))?;
        self.thread(node, &qubits)?;
        if observable {
            self.builder.demand(node);
        }
        Ok(())
    }

    /// Lowers a measurement, which threads capabilities and writes a result.
    ///
    /// `mresetz` arrives here too. It is one operation that carries both
    /// effects, so it is one node carrying both rather than two nodes: the
    /// measurement effect is observable because it produces a classical result,
    /// and the reset effect is irreversible without being observable. Splitting
    /// it would also break the one-instruction-to-one-node correspondence the
    /// identity round trip rests on.
    fn lower_measurement(
        &mut self,
        operation: &str,
        args: &[Operand],
        position: u32,
    ) -> Result<(), Error> {
        let qubits = self.qubits_of(args)?;
        let mut effects: Vec<EffectEvent> = qubits
            .iter()
            .map(|qubit| qubit_effect(EffectMode::Irreversible, true, *qubit))
            .collect();
        if operation == MEASURE_RESET {
            effects.extend(
                qubits
                    .iter()
                    .map(|qubit| qubit_effect(EffectMode::Irreversible, false, *qubit)),
            );
        }
        let node = self.node(operation, effects, NodeOrigin::Instruction(position))?;
        self.thread(node, &qubits)?;

        for result in results_of(args) {
            let output = self.port(node, Direction::Output, PortKind::Value, Type::Result)?;
            self.result_source.insert(result, output);
            let sink = self.valued_node(
                format!("{STORE}{result}"),
                NodeOrigin::Instruction(position),
                NodeValue::ClassicalBit(result),
            )?;
            let input = self.port(sink, Direction::Input, PortKind::Value, Type::Result)?;
            self.connect(output, input, EdgeKind::CopyableValue)?;
        }

        // Nothing consumes a measurement, and the capability chain already
        // orders it, so demanding it is what keeps it.
        self.builder.demand(node);
        Ok(())
    }

    /// Consumes and replaces the capability for each qubit an operation names.
    fn thread(&mut self, node: NodeId, qubits: &[u32]) -> Result<(), Error> {
        for qubit in qubits {
            let input = self.port(node, Direction::Input, PortKind::Capability, Type::Qubit)?;
            self.take(*qubit, input)?;
            let output = self.port(node, Direction::Output, PortKind::Capability, Type::Qubit)?;
            self.live[*qubit as usize] = output;
        }
        Ok(())
    }

    /// Walks a call into the callee's body, with the callee's parameters bound
    /// to the wires the caller passed.
    ///
    /// Walked at the call site rather than once per callable, because a body
    /// lowered once would have to name its qubits by parameter, and a parameter
    /// holds a different wire at every call. Substituting at the call site is
    /// what turns the parameter back into the caller's own operand.
    fn lower_call_into_body(
        &mut self,
        id: CallableId,
        args: &[Operand],
        output: Option<&Variable>,
    ) -> Result<(), Error> {
        let callable = self.program.get_callable(id);
        let refuse = || Error::UnsupportedCallable {
            name: callable.name.clone(),
        };
        let body = callable.body.ok_or_else(refuse)?;
        if self.call_stack.contains(&id) {
            return Err(Error::RecursiveCallable {
                name: callable.name.clone(),
            });
        }
        // A returned value would have to be threaded back to whatever reads it
        // in the caller, and the only value this stage resolves is a result a
        // measurement wrote.
        if output.is_some() || callable.output_type.is_some() {
            return Err(refuse());
        }
        if callable.input_vars.len() != args.len() || callable.input_type.len() != args.len() {
            return Err(refuse());
        }

        let mut frame = BTreeMap::new();
        for ((parameter, ty), arg) in callable
            .input_vars
            .iter()
            .zip(&callable.input_type)
            .zip(args)
        {
            if is_qubit(*ty) {
                frame.insert(*parameter, self.wire_of(arg).ok_or(Error::DynamicQubit)?);
            }
        }

        // The visit set is scoped to one body walk, so the same body reached
        // from a second call site is walked again rather than reported as a
        // back edge. A body reaching itself is a cycle, and the call stack
        // above is what refuses that.
        let outer_visited = std::mem::take(&mut self.visited);
        let outer_frame = std::mem::replace(&mut self.frame, frame);
        let built_before = self.nodes_built;
        self.call_stack.push(id);
        let lowered = self.lower_chain(body, None);
        self.call_stack.pop();
        self.frame = outer_frame;
        self.visited = outer_visited;
        lowered?;
        if self.nodes_built == built_before {
            // A body holding nothing but a terminator would leave the call with
            // no node, and every caller-visible instruction is supposed to have
            // one. The call itself is carried instead.
            let position = self.next_position();
            self.node(OPAQUE, Vec::new(), NodeOrigin::Instruction(position))?;
        }
        Ok(())
    }

    /// What the producer knows about the wire one qubit operand names.
    ///
    /// `None` for an operand that names no qubit at all, which is not a
    /// refusal: a measurement's result operand and a recording call's tag reach
    /// here too.
    fn wire_of(&self, operand: &Operand) -> Option<Wire> {
        match operand {
            Operand::Literal(Literal::Qubit(qubit)) => {
                Some(Wire::Resource(ResourceId(u64::from(*qubit))))
            }
            Operand::Variable(variable) if is_qubit(variable.ty) => {
                Some(self.wire_of_variable(variable.variable_id))
            }
            _ => None,
        }
    }

    /// The wire a variable names, through the frame that bound it.
    ///
    /// A parameter resolves to whatever the caller passed, which is the whole
    /// reason bodies are walked at the call site. A variable no frame bound is
    /// a binding only while the program assigns it exactly once, which is the
    /// obligation `Wire::Binding` states; a variable assigned more than once
    /// does not hold one wire over the span a caller would compare across, so
    /// it owes `Wire::Opaque` instead.
    fn wire_of_variable(&self, variable: VariableId) -> Wire {
        if let Some(wire) = self.frame.get(&variable) {
            return *wire;
        }
        if self.singly_assigned.contains(&variable) {
            Wire::Binding(variable.0)
        } else {
            Wire::Opaque
        }
    }

    /// The qubits one operation names, in operand order.
    ///
    /// The identity question goes through the shared abstraction rather than
    /// through the raw indices, so this producer and the rewrite that later
    /// reads the graph answer it the same way. `resolved` is all or nothing, so
    /// one operand the producer could not resolve withdraws the whole answer
    /// and the operation is refused rather than lowered against a weaker claim.
    fn qubits_of(&mut self, args: &[Operand]) -> Result<Vec<u32>, Error> {
        let wires: Vec<Wire> = args.iter().filter_map(|arg| self.wire_of(arg)).collect();
        for wire in &wires {
            match wire {
                Wire::Resource(_) => self.census.resource += 1,
                Wire::Binding(_) => self.census.binding += 1,
                Wire::Opaque => self.census.opaque += 1,
            }
        }
        let resolved = OperandWires::new(wires.iter().copied())
            .resolved()
            .ok_or(Error::DynamicQubit)?;
        if resolved.len() != wires.len() {
            return Err(Error::RepeatedQubit);
        }
        let mut qubits = Vec::with_capacity(wires.len());
        for wire in &wires {
            let resource = wire.resource().ok_or(Error::DynamicQubit)?;
            let qubit = u32::try_from(resource.0).map_err(|_| Error::DynamicQubit)?;
            if qubit as usize >= self.live.len() {
                return Err(Error::UnknownQubit { qubit });
            }
            qubits.push(qubit);
        }
        Ok(qubits)
    }

    /// Lowers a branch into a node that owns one region per arm.
    ///
    /// Returns the block the arms rejoin at, which is where the walk resumes.
    fn lower_conditional(
        &mut self,
        predicate: PortId,
        then_block: BlockId,
        else_block: BlockId,
    ) -> Result<Option<BlockId>, Error> {
        let join = self
            .join_of(then_block, else_block)
            .ok_or(Error::Unstructured)?;
        let carried = self.carried_qubits(&[then_block, else_block], join);
        let position = self.next_position();

        // A port an inner arm binds against has to sit in that arm's immediate
        // parent, so a qubit the enclosing arm has not touched yet is handed
        // through here first.
        let owed: Vec<u32> = carried
            .iter()
            .copied()
            .filter(|qubit| self.crossings.contains(qubit))
            .collect();
        for qubit in owed {
            self.carry(qubit, position)?;
        }

        let gamma = self.node(GAMMA, Vec::new(), NodeOrigin::Instruction(position))?;
        let selector = self.port(gamma, Direction::Input, PortKind::Value, Type::Result)?;
        self.connect(predicate, selector, EdgeKind::CopyableValue)?;

        let mut merged = Vec::with_capacity(carried.len());
        for _ in &carried {
            merged.push(self.port(gamma, Direction::Output, PortKind::Capability, Type::Qubit)?);
        }
        let entering: Vec<PortId> = carried
            .iter()
            .map(|qubit| self.live[*qubit as usize])
            .collect();

        for arm in [then_block, else_block] {
            self.lower_arm(gamma, arm, join, &carried, &entering, &merged, position)?;
        }

        for (index, qubit) in carried.iter().enumerate() {
            self.live[*qubit as usize] = merged[index];
        }
        Ok(Some(join))
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_arm(
        &mut self,
        gamma: NodeId,
        block: BlockId,
        join: BlockId,
        carried: &[u32],
        entering: &[PortId],
        merged: &[PortId],
        position: u32,
    ) -> Result<(), Error> {
        let parent = self.region;
        let arm = self.builder.add_region(Some(parent), RegionKind::Gamma);
        // An unowned branch region is refused, so the gamma owns both arms.
        self.builder
            .declare_region_owner(arm, gamma)
            .map_err(build_error)?;
        self.region = arm;
        let enclosing_crossings =
            std::mem::replace(&mut self.crossings, carried.iter().copied().collect());
        // An arm takes its wire from the region around it, whatever that region
        // is. A branch inside a loop body is still a branch.
        let enclosing_role = std::mem::replace(&mut self.crossing_role, InterfaceRole::Argument);
        let enclosing_results = self.result_source.clone();
        // The chain is scoped to a region, because emission buckets a node by
        // the block its site names and a block sits in one region. An arm's
        // accesses order among themselves; what the arm read from outside is
        // still what the region after the join reads from.
        let enclosing_access = self.access_of.clone();
        self.arm_depth += 1;
        for (index, qubit) in carried.iter().enumerate() {
            self.live[*qubit as usize] = entering[index];
        }

        let mut relayed = BTreeMap::new();
        let lowered =
            self.lower_arm_body(arm, block, join, carried, merged, position, &mut relayed);

        self.arm_depth -= 1;
        self.access_of = enclosing_access;
        self.result_source = self.visible_after(enclosing_results, &relayed);
        self.crossing_role = enclosing_role;
        self.crossings = enclosing_crossings;
        self.region = parent;
        lowered
    }

    /// The fallible middle of an arm, split out so the enclosing region's state
    /// is restored on the error path as well as the ordinary one.
    #[allow(clippy::too_many_arguments)]
    fn lower_arm_body(
        &mut self,
        arm: RegionId,
        block: BlockId,
        join: BlockId,
        carried: &[u32],
        merged: &[PortId],
        position: u32,
        relayed: &mut BTreeMap<u32, PortId>,
    ) -> Result<(), Error> {
        self.relay_results(arm, block, join, position, relayed)?;
        self.lower_chain(block, Some(join))?;
        for qubit in carried {
            if self.crossings.contains(qubit) {
                self.carry(*qubit, position)?;
            }
        }
        for (index, qubit) in carried.iter().enumerate() {
            self.bind(
                arm,
                merged[index],
                self.live[*qubit as usize],
                InterfaceRole::Result,
            )?;
        }
        Ok(())
    }

    /// The loop a branch heads, as the arm that repeats and the arm that leaves.
    ///
    /// A branch heads a loop when exactly one of its arms comes back to it.
    /// Both arms coming back is not a loop with two bodies, it is a shape with
    /// no single region to be, so it refuses rather than picking one.
    ///
    /// # Errors
    ///
    /// Returns [`Error::UnsupportedLoop`] for a branch this stage can see is a
    /// loop and cannot name.
    fn loop_shape(
        &self,
        header: BlockId,
        then_block: BlockId,
        else_block: BlockId,
    ) -> Result<Option<(BlockId, BlockId)>, Error> {
        let repeats = |body: BlockId| -> Result<(), Error> {
            if body == header {
                // The body would be the condition, which runs a different
                // number of times than the body of a loop does.
                return Err(Error::UnsupportedLoop {
                    reason: "its body is its own condition".to_string(),
                });
            }
            Ok(())
        };
        match (
            self.reaches(then_block, header),
            self.reaches(else_block, header),
        ) {
            (true, false) => {
                repeats(then_block)?;
                Ok(Some((then_block, else_block)))
            }
            (false, true) => {
                repeats(else_block)?;
                Ok(Some((else_block, then_block)))
            }
            (false, false) => Ok(None),
            (true, true) => Err(Error::UnsupportedLoop {
                reason: "both of its arms come back to the condition".to_string(),
            }),
        }
    }

    /// Whether one block can reach another without crossing a back edge already
    /// accounted for.
    fn reaches(&self, start: BlockId, target: BlockId) -> bool {
        let mut seen = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(block) = queue.pop_front() {
            if block == target {
                return true;
            }
            for next in self.successors(block) {
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        false
    }

    /// Lowers a loop into a node that owns the region its body repeats in.
    ///
    /// Returns the block the walk resumes at, which is the arm that leaves.
    fn lower_loop(
        &mut self,
        header: BlockId,
        body: BlockId,
        exit: BlockId,
    ) -> Result<BlockId, Error> {
        self.loop_headers.insert(header);
        let lowered = self.lower_loop_at(header, body);
        self.loop_headers.remove(&header);
        lowered?;
        Ok(exit)
    }

    fn lower_loop_at(&mut self, header: BlockId, body: BlockId) -> Result<(), Error> {
        let carried = self.carried_qubits(&[body], header);
        let position = self.next_position();

        // Same obligation the gamma has: a port the body binds against must sit
        // in the region the loop node is in.
        let owed: Vec<u32> = carried
            .iter()
            .copied()
            .filter(|qubit| self.crossings.contains(qubit))
            .collect();
        for qubit in owed {
            self.carry(qubit, position)?;
        }

        let theta = self.node(THETA, Vec::new(), NodeOrigin::Instruction(position))?;
        let mut entering = Vec::with_capacity(carried.len());
        for qubit in &carried {
            let port = self.port(theta, Direction::Input, PortKind::Capability, Type::Qubit)?;
            self.connect(self.live[*qubit as usize], port, EdgeKind::AffineCapability)?;
            entering.push(port);
        }
        let mut leaving = Vec::with_capacity(carried.len());
        for _ in &carried {
            leaving.push(self.port(theta, Direction::Output, PortKind::Capability, Type::Qubit)?);
        }

        self.lower_body(theta, header, body, &carried, &entering, &leaving, position)?;

        for (index, qubit) in carried.iter().enumerate() {
            self.live[*qubit as usize] = leaving[index];
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_body(
        &mut self,
        theta: NodeId,
        header: BlockId,
        block: BlockId,
        carried: &[u32],
        entering: &[PortId],
        leaving: &[PortId],
        position: u32,
    ) -> Result<(), Error> {
        let parent = self.region;
        let region = self.builder.add_region(Some(parent), RegionKind::Theta);
        // A body no node owns is a region nothing enters, which the reader
        // refuses by name.
        self.builder
            .declare_region_owner(region, theta)
            .map_err(build_error)?;
        self.region = region;
        let enclosing_crossings =
            std::mem::replace(&mut self.crossings, carried.iter().copied().collect());
        let enclosing_role = std::mem::replace(&mut self.crossing_role, InterfaceRole::Carried);
        let enclosing_entry = std::mem::take(&mut self.carried_entry);
        let enclosing_results = self.result_source.clone();
        let enclosing_access = self.access_of.clone();
        let enclosing_values = std::mem::take(&mut self.carried_values);
        for (index, qubit) in carried.iter().enumerate() {
            self.live[*qubit as usize] = entering[index];
        }

        let mut relayed = BTreeMap::new();
        let lowered = self.lower_body_within(
            region,
            header,
            block,
            carried,
            leaving,
            position,
            &mut relayed,
        );

        self.carried_values = enclosing_values;
        self.access_of = enclosing_access;
        self.result_source = self.visible_after(enclosing_results, &relayed);
        self.carried_entry = enclosing_entry;
        self.crossing_role = enclosing_role;
        self.crossings = enclosing_crossings;
        self.region = parent;
        lowered
    }

    /// The fallible middle of a loop body, split out so the enclosing region's
    /// state is restored on the error path as well as the ordinary one.
    #[allow(clippy::too_many_arguments)]
    fn lower_body_within(
        &mut self,
        region: RegionId,
        header: BlockId,
        block: BlockId,
        carried: &[u32],
        leaving: &[PortId],
        position: u32,
        relayed: &mut BTreeMap<u32, PortId>,
    ) -> Result<(), Error> {
        self.relay_results(region, block, header, position, relayed)?;
        self.lower_chain(block, Some(header))?;
        for qubit in carried {
            if self.crossings.contains(qubit) {
                self.carry(*qubit, position)?;
            }
        }
        for (index, qubit) in carried.iter().enumerate() {
            let produced = self.live[*qubit as usize];
            self.bind(region, leaving[index], produced, InterfaceRole::Result)?;
            let taken =
                self.carried_entry
                    .get(qubit)
                    .copied()
                    .ok_or_else(|| Error::UnsupportedLoop {
                        reason: "its body never took a wire it carries".to_string(),
                    })?;
            // Declared on both ends. The wire leaving one iteration is the wire
            // entering the next, and an edge that says so without the region
            // saying so is an undeclared cycle, which is a different thing.
            self.builder
                .declare_exit(region, ExitKind::Backedge, produced)
                .map_err(build_error)?;
            self.connect(produced, taken, EdgeKind::AffineCapability)?;
        }
        self.close_carried_values(region)
    }

    /// Returns each variable the body took from outside to where it took it.
    ///
    /// Declared on both ends for the same reason the capability back edge is:
    /// the verifier exempts a cycle only when the region names the producing
    /// port as its back-edge exit and the consuming port as carried, so a half
    /// declaration refuses as an undeclared cycle rather than passing.
    fn close_carried_values(&mut self, region: RegionId) -> Result<(), Error> {
        for (variable, taken) in std::mem::take(&mut self.carried_values) {
            let Some((produced, produced_region)) = self.access_of.get(&variable).copied() else {
                continue;
            };
            if produced_region != region {
                continue;
            }
            self.builder
                .declare_exit(region, ExitKind::Backedge, produced)
                .map_err(build_error)?;
            self.connect(produced, taken, EdgeKind::CopyableValue)?;
        }
        Ok(())
    }

    /// Relays into an arm every result a branch inside it reads.
    ///
    /// Building them on entry is what keeps predicate resolution stateless:
    /// inside an arm, a result's source always names a port in the region being
    /// built, so a nested selector edge needs no binding of its own.
    fn relay_results(
        &mut self,
        arm: RegionId,
        block: BlockId,
        join: BlockId,
        position: u32,
        relayed: &mut BTreeMap<u32, PortId>,
    ) -> Result<(), Error> {
        for result in self.results_read_within(block, join) {
            let Some(source) = self.result_source.get(&result).copied() else {
                continue;
            };
            let node = self.node(RELAY, Vec::new(), NodeOrigin::Instruction(position))?;
            let input = self.port(node, Direction::Input, PortKind::Value, Type::Result)?;
            self.bind(arm, source, input, InterfaceRole::Argument)?;
            self.connect(source, input, EdgeKind::CopyableValue)?;
            let output = self.port(node, Direction::Output, PortKind::Value, Type::Result)?;
            self.result_source.insert(result, output);
            relayed.insert(result, output);
        }
        Ok(())
    }

    /// What the enclosing region can still see after an arm was lowered.
    ///
    /// A port the arm produced lives in the arm's region, so nothing outside
    /// can reach it. A result the arm rewrote is dropped rather than reverted.
    fn visible_after(
        &self,
        enclosing: BTreeMap<u32, PortId>,
        relayed: &BTreeMap<u32, PortId>,
    ) -> BTreeMap<u32, PortId> {
        enclosing
            .into_iter()
            .filter(|(result, source)| {
                let current = self.result_source.get(result);
                current == Some(source) || current == relayed.get(result)
            })
            .collect()
    }

    /// Hands a qubit through an arm that never touched it.
    fn carry(&mut self, qubit: u32, position: u32) -> Result<(), Error> {
        let node = self.node(CARRY, Vec::new(), NodeOrigin::Instruction(position))?;
        let input = self.port(node, Direction::Input, PortKind::Capability, Type::Qubit)?;
        self.take(qubit, input)?;
        let output = self.port(node, Direction::Output, PortKind::Capability, Type::Qubit)?;
        self.live[qubit as usize] = output;
        Ok(())
    }

    /// The block both arms rejoin at.
    ///
    /// Within this stage's no-back-edge class the block graph is a DAG, so the
    /// first block reachable from the then-arm that is also reachable from the
    /// else-arm is the join rather than an approximation of one.
    fn join_of(&self, then_block: BlockId, else_block: BlockId) -> Option<BlockId> {
        let from_else = self.reachable(else_block);
        let mut queue = VecDeque::from([then_block]);
        let mut seen = BTreeSet::from([then_block]);
        while let Some(block) = queue.pop_front() {
            if from_else.contains(&block) {
                return Some(block);
            }
            for next in self.successors(block) {
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        None
    }

    fn reachable(&self, start: BlockId) -> BTreeSet<BlockId> {
        let mut seen = BTreeSet::from([start]);
        let mut queue = VecDeque::from([start]);
        while let Some(block) = queue.pop_front() {
            for next in self.successors(block) {
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        seen
    }

    /// The blocks control can pass to next, minus the loop headers already
    /// being lowered.
    ///
    /// Every join and reachability question this stage asks is about one
    /// iteration, and a back edge leaves the iteration it asks about. Dropping
    /// it here is what lets one DAG-shaped answer serve both the code with
    /// loops and the code without them.
    fn successors(&self, block: BlockId) -> Vec<BlockId> {
        let next = match self.program.get_block(block).0.last() {
            Some(Instruction::Jump(next)) => vec![*next],
            Some(Instruction::Branch(_, then_block, else_block, _)) => {
                vec![*then_block, *else_block]
            }
            _ => Vec::new(),
        };
        next.into_iter()
            .filter(|block| !self.loop_headers.contains(block))
            .collect()
    }

    /// Every qubit the given arms touch, at any depth.
    ///
    /// A union rather than an intersection. Two arms of a branch produce the
    /// same interface, and a loop body has to hand back every wire it was
    /// given, so in both cases what crosses is what any of them names.
    fn carried_qubits(&self, arms: &[BlockId], stop: BlockId) -> Vec<u32> {
        let mut qubits = BTreeSet::new();
        for arm in arms {
            for block in self.blocks_within(*arm, stop) {
                for instruction in &self.program.get_block(block).0 {
                    if let Instruction::Call(_, args, _, _) = instruction {
                        for arg in args {
                            if let Operand::Literal(Literal::Qubit(qubit)) = arg {
                                qubits.insert(*qubit);
                            }
                        }
                    }
                }
            }
        }
        qubits.into_iter().collect()
    }

    /// Every result a readout inside an arm names.
    fn results_read_within(&self, start: BlockId, join: BlockId) -> BTreeSet<u32> {
        let mut results = BTreeSet::new();
        for block in self.blocks_within(start, join) {
            for instruction in &self.program.get_block(block).0 {
                if let Instruction::Call(callable, args, _, _) = instruction
                    && self.program.get_callable(*callable).call_type == CallableType::Readout
                {
                    for arg in args {
                        if let Operand::Literal(Literal::Result(result)) = arg {
                            results.insert(*result);
                        }
                    }
                }
            }
        }
        results
    }

    /// The blocks an arm covers, stopping at the join it shares with its sibling.
    fn blocks_within(&self, start: BlockId, join: BlockId) -> BTreeSet<BlockId> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([start]);
        while let Some(block) = queue.pop_front() {
            if block == join || !seen.insert(block) {
                continue;
            }
            for next in self.successors(block) {
                if next != join {
                    queue.push_back(next);
                }
            }
        }
        seen
    }

    /// Consumes the capability standing for a qubit, declaring the region
    /// crossing first when this is the arm's first use of it.
    fn take(&mut self, qubit: u32, input: PortId) -> Result<(), Error> {
        let source = self.live[qubit as usize];
        if self.crossings.remove(&qubit) {
            self.bind(self.region, source, input, self.crossing_role)?;
            if self.crossing_role == InterfaceRole::Carried {
                // Only the first iteration takes the wire from outside, and the
                // one edge this port admits is the back edge that brings it
                // back. The binding is what says where it came from before that.
                self.carried_entry.insert(qubit, input);
                return Ok(());
            }
        }
        self.connect(source, input, EdgeKind::AffineCapability)
    }

    fn bind(
        &mut self,
        region: RegionId,
        outer: PortId,
        inner: PortId,
        role: InterfaceRole,
    ) -> Result<(), Error> {
        self.builder
            .bind_interface(region, InterfaceBinding { outer, inner, role })
            .map_err(build_error)
    }

    fn next_position(&mut self) -> u32 {
        let position = self.position;
        self.position += 1;
        position
    }

    fn node(
        &mut self,
        operation: impl Into<String>,
        effects: Vec<EffectEvent>,
        origin: NodeOrigin,
    ) -> Result<NodeId, Error> {
        self.build_node(
            operation,
            effects,
            NodePayload {
                value: None,
                origin: Some(origin),
            },
        )
    }

    fn valued_node(
        &mut self,
        operation: impl Into<String>,
        origin: NodeOrigin,
        value: NodeValue,
    ) -> Result<NodeId, Error> {
        self.build_node(
            operation,
            Vec::new(),
            NodePayload {
                value: Some(value),
                origin: Some(origin),
            },
        )
    }

    fn build_node(
        &mut self,
        operation: impl Into<String>,
        effects: Vec<EffectEvent>,
        payload: NodePayload,
    ) -> Result<NodeId, Error> {
        // Both counts are maintained here rather than at the call sites,
        // because more than one site builds a carrier and the loop guard
        // subtracts one count from the other.
        let operation = operation.into();
        let carrier = operation == OPAQUE;
        let node = self
            .builder
            .add_node(self.region, operation, effects)
            .map_err(build_error)?;
        self.builder
            .declare_payload(node, payload)
            .map_err(build_error)?;
        self.nodes_built += 1;
        if carrier {
            self.carriers_built += 1;
        }
        if let Some(site) = self.site {
            self.sites.insert(node, site);
        }
        Ok(node)
    }

    fn port(
        &mut self,
        node: NodeId,
        direction: Direction,
        kind: PortKind,
        ty: Type,
    ) -> Result<PortId, Error> {
        let usage = match kind {
            PortKind::Capability => Usage::Linear,
            PortKind::Value => Usage::Copyable,
            _ => Usage::Affine,
        };
        self.builder
            .add_port(node, direction, kind, ty, Stage::Residual, usage)
            .map_err(build_error)
    }

    fn connect(&mut self, source: PortId, target: PortId, kind: EdgeKind) -> Result<(), Error> {
        self.builder
            .connect(qsc_qhdg::Edge {
                source,
                target,
                kind,
            })
            .map_err(build_error)
    }
}

fn build_error(error: impl std::fmt::Debug) -> Error {
    Error::Build {
        reason: format!("{error:?}"),
    }
}

const fn effect(mode: EffectMode, observable: bool) -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode,
        resource: EffectResource::Unknown,
        observable,
    }
}

fn qubit_effect(mode: EffectMode, observable: bool, qubit: u32) -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode,
        resource: EffectResource::Named(format!("qubit:{qubit}")),
        observable,
    }
}

const fn is_qubit(ty: Ty) -> bool {
    matches!(ty, Ty::Prim(qsc_rir::rir::Prim::Qubit))
}

/// The port type a classical value of one RIR type crosses an edge as.
///
/// `None` refuses rather than approximates. A pointer has no counterpart here,
/// and typing it `Unknown` would not help, because an edge requires the same
/// type on both ends. A qubit reaches here only inside an array, where the
/// element type would make the port linear at any depth.
fn port_type(ty: Ty) -> Option<Type> {
    let element = |prim| match prim {
        Prim::Boolean => Some(Type::Bool),
        Prim::Integer => Some(Type::Int),
        Prim::Double => Some(Type::Double),
        Prim::Result => Some(Type::Result),
        Prim::Qubit | Prim::Pointer => None,
    };
    match ty {
        Ty::Prim(prim) => element(prim),
        Ty::Array(_, prim) => element(prim).map(|element| Type::Array(Box::new(element))),
    }
}

/// The variables one instruction reads, and the one it defines.
///
/// A terminator reads variables too, and returns nothing here, because a
/// terminator is split off before lowering and never becomes a node.
fn accesses(instruction: &Instruction) -> (Vec<Variable>, Option<Variable>) {
    fn read(operand: &Operand, into: &mut Vec<Variable>) {
        if let Operand::Variable(variable) = operand {
            into.push(*variable);
        }
    }

    let mut sources = Vec::new();
    let target = match instruction {
        Instruction::Store(value, target)
        | Instruction::LogicalNot(value, target)
        | Instruction::BitwiseNot(value, target)
        | Instruction::Convert(value, target) => {
            read(value, &mut sources);
            Some(*target)
        }
        Instruction::StoreArray(values, target) => {
            for value in values {
                read(value, &mut sources);
            }
            Some(*target)
        }
        Instruction::Add(lhs, rhs, target)
        | Instruction::Sub(lhs, rhs, target)
        | Instruction::Mul(lhs, rhs, target)
        | Instruction::Sdiv(lhs, rhs, target)
        | Instruction::Srem(lhs, rhs, target)
        | Instruction::Shl(lhs, rhs, target)
        | Instruction::Ashr(lhs, rhs, target)
        | Instruction::Fadd(lhs, rhs, target)
        | Instruction::Fsub(lhs, rhs, target)
        | Instruction::Fmul(lhs, rhs, target)
        | Instruction::Fdiv(lhs, rhs, target)
        | Instruction::Frem(lhs, rhs, target)
        | Instruction::LogicalAnd(lhs, rhs, target)
        | Instruction::LogicalOr(lhs, rhs, target)
        | Instruction::BitwiseAnd(lhs, rhs, target)
        | Instruction::BitwiseOr(lhs, rhs, target)
        | Instruction::BitwiseXor(lhs, rhs, target)
        | Instruction::Index(lhs, rhs, target)
        | Instruction::Fcmp(_, lhs, rhs, target)
        | Instruction::Icmp(_, lhs, rhs, target) => {
            read(lhs, &mut sources);
            read(rhs, &mut sources);
            Some(*target)
        }
        Instruction::Call(_, args, target, _) => {
            for arg in args {
                read(arg, &mut sources);
            }
            *target
        }
        Instruction::Phi(sources_by_block, target) => {
            for (operand, _) in sources_by_block {
                read(operand, &mut sources);
            }
            Some(*target)
        }
        Instruction::Load(source, target) => {
            sources.push(*source);
            Some(*target)
        }
        Instruction::Alloca(target) => Some(*target),
        Instruction::Jump(..) | Instruction::Branch(..) | Instruction::Return(..) => None,
    };
    (sources, target)
}

/// The variables the program assigns exactly once.
///
/// The insertion point is before the SSA transform, so a variable here may well
/// be stored to more than once. That is the case [`Wire::Binding`] refuses to
/// cover, and this is where the difference is decided rather than assumed.
/// A parameter counts as assigned, so a parameter reached without the frame
/// that binds it is never mistaken for a binding that holds one wire.
fn singly_assigned(program: &Program) -> BTreeSet<VariableId> {
    let mut seen: BTreeMap<VariableId, usize> = BTreeMap::new();
    let mut record = |variable: VariableId| {
        *seen.entry(variable).or_default() += 1;
    };
    for (_, callable) in program.callables.iter() {
        for parameter in &callable.input_vars {
            record(*parameter);
        }
    }
    for (_, block) in program.blocks.iter() {
        for instruction in &block.0 {
            match instruction {
                Instruction::Call(_, _, Some(variable), _)
                | Instruction::Store(_, variable)
                | Instruction::StoreArray(_, variable)
                | Instruction::Add(_, _, variable)
                | Instruction::Sub(_, _, variable)
                | Instruction::Mul(_, _, variable)
                | Instruction::Sdiv(_, _, variable)
                | Instruction::Srem(_, _, variable)
                | Instruction::Shl(_, _, variable)
                | Instruction::Ashr(_, _, variable)
                | Instruction::Fadd(_, _, variable)
                | Instruction::Fsub(_, _, variable)
                | Instruction::Fmul(_, _, variable)
                | Instruction::Fdiv(_, _, variable)
                | Instruction::Frem(_, _, variable)
                | Instruction::Fcmp(_, _, _, variable)
                | Instruction::Icmp(_, _, _, variable)
                | Instruction::LogicalNot(_, variable)
                | Instruction::LogicalAnd(_, _, variable)
                | Instruction::LogicalOr(_, _, variable)
                | Instruction::BitwiseNot(_, variable)
                | Instruction::BitwiseAnd(_, _, variable)
                | Instruction::BitwiseOr(_, _, variable)
                | Instruction::BitwiseXor(_, _, variable)
                | Instruction::Phi(_, variable)
                | Instruction::Convert(_, variable)
                | Instruction::Load(_, variable)
                | Instruction::Alloca(variable)
                | Instruction::Index(_, _, variable) => record(variable.variable_id),
                Instruction::Call(_, _, None, _)
                | Instruction::Jump(..)
                | Instruction::Branch(..)
                | Instruction::Return(..) => {}
            }
        }
    }
    seen.into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|(variable, _)| variable)
        .collect()
}

fn results_of(args: &[Operand]) -> Vec<u32> {
    args.iter()
        .filter_map(|arg| match arg {
            Operand::Literal(Literal::Result(result)) => Some(*result),
            _ => None,
        })
        .collect()
}

/// The only plain measurement this stage emits.
const MEASURE: &str = "quantum.measure_z";

/// Measure and reset, which is one operation carrying both effects.
const MEASURE_RESET: &str = "quantum.measure_reset_z";

/// The vocabulary name for one measurement intrinsic.
///
/// Exhaustive rather than defaulting, so a measurement variant added to the
/// backend has to be given a graph name deliberately instead of silently
/// arriving as a plain measurement and losing whatever else it does.
fn measurement_name(name: &str) -> Option<&'static str> {
    match name {
        "__quantum__qis__m__body" | "__quantum__qis__mz__body" => Some(MEASURE),
        "__quantum__qis__mresetz__body" => Some(MEASURE_RESET),
        _ => None,
    }
}

/// The vocabulary name for one runtime intrinsic, from the one place that
/// defines the names.
///
/// Exhaustive over the intrinsics the backend emits rather than defaulting.
/// `__quantum__qis__mresetz__body` is deliberately absent: it is measure and
/// reset, which the vocabulary can express only as two nodes, and emitting two
/// would break the one-instruction-to-one-node correspondence the identity
/// round trip rests on. It is refused until the return leg says what it should
/// become.
fn operation_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "__quantum__qis__ccx__body" => "quantum.ccx",
        "__quantum__qis__cnot__body" | "__quantum__qis__cx__body" => "quantum.cx",
        "__quantum__qis__cy__body" => "quantum.cy",
        "__quantum__qis__cz__body" => "quantum.cz",
        "__quantum__qis__h__body" => "quantum.h",
        "__quantum__qis__i__body" => "quantum.i",
        "__quantum__qis__rx__body" => "quantum.rx",
        "__quantum__qis__rxx__body" => "quantum.rxx",
        "__quantum__qis__ry__body" => "quantum.ry",
        "__quantum__qis__ryy__body" => "quantum.ryy",
        "__quantum__qis__rz__body" => "quantum.rz",
        "__quantum__qis__rzz__body" => "quantum.rzz",
        "__quantum__qis__s__adj" => "quantum.s_adj",
        "__quantum__qis__s__body" => "quantum.s",
        "__quantum__qis__swap__body" => "quantum.swap",
        "__quantum__qis__sx__body" => "quantum.sx",
        "__quantum__qis__t__adj" => "quantum.t_adj",
        "__quantum__qis__t__body" => "quantum.t",
        "__quantum__qis__x__body" => "quantum.x",
        "__quantum__qis__y__body" => "quantum.y",
        "__quantum__qis__z__body" => "quantum.z",
        "__quantum__qis__barrier__body" => BARRIER,
        "__quantum__qis__reset__body" => RESET,
        _ => return None,
    })
}

/// Every operation name this producer can emit.
///
/// Declared so the shared vocabulary can be asked to accept it, which is the
/// check that this producer added no name of its own.
pub fn emitted_names() -> impl Iterator<Item = &'static str> {
    const INTRINSICS: [&str; 24] = [
        "__quantum__qis__ccx__body",
        "__quantum__qis__cnot__body",
        "__quantum__qis__cx__body",
        "__quantum__qis__cy__body",
        "__quantum__qis__cz__body",
        "__quantum__qis__h__body",
        "__quantum__qis__i__body",
        "__quantum__qis__rx__body",
        "__quantum__qis__rxx__body",
        "__quantum__qis__ry__body",
        "__quantum__qis__ryy__body",
        "__quantum__qis__rz__body",
        "__quantum__qis__rzz__body",
        "__quantum__qis__s__adj",
        "__quantum__qis__s__body",
        "__quantum__qis__swap__body",
        "__quantum__qis__sx__body",
        "__quantum__qis__t__adj",
        "__quantum__qis__t__body",
        "__quantum__qis__x__body",
        "__quantum__qis__y__body",
        "__quantum__qis__z__body",
        "__quantum__qis__barrier__body",
        "__quantum__qis__reset__body",
    ];
    INTRINSICS.into_iter().filter_map(operation_name).chain([
        MEASURE,
        MEASURE_RESET,
        ALLOCATE,
        RELEASE,
        CLBIT,
        GAMMA,
        THETA,
        CARRY,
        RELAY,
        OPAQUE,
    ])
}
