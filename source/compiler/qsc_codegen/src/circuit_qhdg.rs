// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Lowering a circuit into the QHDG graph.
//!
//! The two representations disagree about what a qubit is. A circuit addresses
//! qubits by index and reuses an index freely: `h(0)` followed by `x(0)` names
//! the same qubit twice. The graph carries a qubit as a capability that flows
//! along an edge, and its own rules refuse a capability used more than once.
//!
//! Bridging that is the whole of this module. Each instruction consumes the
//! capability standing for each qubit it touches and produces a fresh one, so
//! an index becomes a chain of capabilities rather than a name repeated. The
//! conversion is a named boundary with a verifier rather than an implicit
//! conversion, so the seam can be tested and a malformed conversion is reported
//! against a named rule instead of being discovered later.

#[cfg(test)]
mod tests;

mod provenance;
mod raise;
mod reader;

pub use provenance::{CircuitOrigin, CircuitProvenance, circuit_provenance};
pub use raise::{RaiseError, RaisedCircuit, graph_to_circuit, graph_to_circuit_with_origins};
pub use reader::{
    ReadConditional, ReadError, ReadInstruction, ReadItem, ReadLoop, ReadProgram, qubit_operands,
    read_program,
};

use std::collections::{BTreeMap, BTreeSet};

use qsc_circuit_store::{
    AngleExpr, AngleId, AngleOp as StoreAngleOp, BinaryOp, BlockId, CircuitStore, ControlFlow,
    Expr, ExprId, IfElse, Instruction, Literal, PackedOperation, ParamValue, ParameterId,
    StandardGate,
};
use qsc_qhdg::{
    Angle, AngleOp, Direction, EdgeKind, EffectEvent, Graph, GraphBuilder, InterfaceBinding,
    InterfaceRole, NodeId, NodePayload, NodeValue, PortId, PortKind, Real, RegionId, RegionKind,
    Stage, Usage, verify,
};
use qsc_semantic_contracts::{EffectDomain, EffectMode, Type};

/// The operation names this producer emits, from the one place that defines
/// them.
///
/// Re-exported rather than declared. The vocabulary is a fact about the graph
/// and lives in the graph's own crate, so all producers map onto the same
/// names rather than defining private naming conventions.
pub(crate) use qsc_qhdg::vocabulary::{
    ALLOCATE, BARRIER, CARRY, CLBIT, CONSTANT, GAMMA, OPAQUE, PHASE, QUANTUM, RELAY, RELEASE,
    RESET, STORE, SYMBOLIC, THETA,
};

/// Why a circuit could not be lowered into a graph.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("operation '{operation}' has no graph form")]
    Unsupported { operation: String },
    #[error("the graph could not be built: {reason}")]
    Build { reason: String },
    #[error("instruction {index} names qubit {qubit}, which the circuit does not have")]
    UnknownQubit { index: usize, qubit: u32 },
    #[error("instruction {index} names a parameter or angle the circuit does not declare")]
    SymbolicParameter { index: usize },
    #[error("instruction {index} branches on a block the circuit does not have")]
    UnknownBlock { index: usize },
    #[error("instruction {index} reads classical bit {clbit}, which nothing has measured")]
    UnknownCondition { index: usize, clbit: u32 },
    #[error("the lowered graph failed its own verification: {0}")]
    Verification(String),
}

/// Lowers a circuit into a graph, refusing to return one that does not verify.
///
/// # Errors
///
/// Returns an error when the circuit holds a construct with no graph form, when
/// the graph cannot be assembled, or when the assembled graph fails the
/// verification the graph crate applies to its own programs.
pub fn circuit_to_graph(store: &CircuitStore) -> Result<Graph, Error> {
    let graph = Lowering::new(store).run()?;
    let diagnostics = verify(&graph);
    if !diagnostics.is_empty() {
        let reported = diagnostics
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code.as_str(), diagnostic.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(Error::Verification(reported));
    }
    Ok(graph)
}

/// What a block reaches, at any depth.
#[derive(Default)]
struct Reached {
    /// Qubits any instruction in the block touches.
    qubits: BTreeSet<u32>,
    /// Classical bits the block's conditionals read.
    clbits: BTreeSet<u32>,
}

struct Lowering<'a> {
    store: &'a CircuitStore,
    builder: GraphBuilder,
    region: RegionId,
    /// The capability standing for each qubit as the sequence is walked.
    ///
    /// This is the register-to-linear bridge: an index selects a capability,
    /// and using it replaces it, so no capability is ever read twice.
    live: Vec<PortId>,
    /// The port a measurement wrote each classical bit from.
    ///
    /// A conditional reads its predicate from that port rather than from the
    /// bit's index, so the graph carries the dependency instead of leaving the
    /// order to chance.
    clbit_source: BTreeMap<u32, PortId>,
    /// Qubits whose live capability is still outside the region being built.
    ///
    /// The first use of one inside a branch is what declares the crossing, and
    /// the declaration has to exist before the edge does.
    crossings: BTreeSet<u32>,
}

impl<'a> Lowering<'a> {
    fn new(store: &'a CircuitStore) -> Self {
        let mut builder = GraphBuilder::new();
        let region = builder.add_region(None, RegionKind::ModuleOmega);
        Self {
            store,
            builder,
            region,
            live: Vec::new(),
            clbit_source: BTreeMap::new(),
            crossings: BTreeSet::new(),
        }
    }

    fn run(mut self) -> Result<Graph, Error> {
        self.declare_widths()?;
        for (index, instruction) in self.store.instructions().enumerate() {
            let instruction = instruction.clone();
            self.lower_instruction(index, &instruction)?;
        }
        self.release_qubits()?;
        Ok(self.builder.finish())
    }

    /// Records everything about the circuit that is not an instruction.
    ///
    /// A graph that carried only instructions could not say how wide the
    /// circuit was or what phase it carried, so raising one would have to guess.
    fn declare_widths(&mut self) -> Result<(), Error> {
        let phase = self.store.global_phase();
        let node = self.valued_node(
            format!("{PHASE}{phase:?}"),
            Vec::new(),
            CircuitOrigin::GlobalPhase,
            NodeValue::Number(Real::new(phase)),
        )?;
        self.port(node, Direction::Output, PortKind::Value, Type::Double)?;
        self.allocate_qubits()?;
        for clbit in 0..self.store.clbit_count() {
            let node = self.node(CLBIT, Vec::new(), CircuitOrigin::Clbit(clbit))?;
            self.port(node, Direction::Output, PortKind::Value, Type::Result)?;
        }
        Ok(())
    }

    /// Gives every qubit a capability to start from.
    fn allocate_qubits(&mut self) -> Result<(), Error> {
        for qubit in 0..self.store.qubit_count() {
            let node = self.node(
                ALLOCATE,
                vec![qubit_effect(EffectMode::Allocate, false, qubit)],
                CircuitOrigin::Qubit(qubit),
            )?;
            let port = self.port(node, Direction::Output, PortKind::Capability, Type::Qubit)?;
            self.live.push(port);
        }
        Ok(())
    }

    /// Consumes every capability that is still live.
    ///
    /// Without this the last capability on each qubit is produced and never
    /// used, which is a dangling linear value rather than a released one.
    fn release_qubits(&mut self) -> Result<(), Error> {
        for index in 0..self.live.len() {
            let qubit = u32::try_from(index).expect("a circuit's qubit count is a u32");
            let node = self.node(
                RELEASE,
                vec![qubit_effect(EffectMode::Release, false, qubit)],
                CircuitOrigin::Qubit(qubit),
            )?;
            let input = self.port(node, Direction::Input, PortKind::Capability, Type::Qubit)?;
            self.connect(self.live[index], input, EdgeKind::AffineCapability)?;
        }
        Ok(())
    }

    fn lower_instruction(&mut self, index: usize, instruction: &Instruction) -> Result<(), Error> {
        if let PackedOperation::ControlFlow(control) = &instruction.operation {
            return self.lower_control_flow(index, control);
        }
        let operation = operation_name(&instruction.operation)?;
        let qubits: Vec<u32> = self
            .store
            .qubits_of(instruction)
            .unwrap_or(&[])
            .iter()
            .map(|qubit| qubit.0)
            .collect();
        for qubit in &qubits {
            if *qubit as usize >= self.live.len() {
                return Err(Error::UnknownQubit {
                    index,
                    qubit: *qubit,
                });
            }
        }

        let observable = is_observable(&instruction.operation);
        let effects = if observable {
            qubits
                .iter()
                .map(|qubit| qubit_effect(EffectMode::Irreversible, true, *qubit))
                .collect()
        } else {
            vec![effect(EffectMode::Write, false)]
        };

        // Constants are built before the gate that reads them so their durable
        // identity orders them first when the raise has no edge to go on.
        let mut constants = Vec::new();
        for param in instruction.params.as_deref().unwrap_or(&[]) {
            constants.push(match param {
                ParamValue::Literal(value) => self.constant(*value, index)?,
                symbolic => {
                    let angle = self.angle_of(*symbolic, index)?;
                    self.symbolic_constant(angle, index)?
                }
            });
        }

        let node = self.node(operation, effects, CircuitOrigin::Instruction(index))?;

        // Each qubit's capability is consumed here and replaced by the one this
        // node produces, so the chain stays single-use.
        for qubit in &qubits {
            let input = self.port(node, Direction::Input, PortKind::Capability, Type::Qubit)?;
            self.take(*qubit, input)?;
            let output = self.port(node, Direction::Output, PortKind::Capability, Type::Qubit)?;
            self.live[*qubit as usize] = output;
        }

        for source in constants {
            let input = self.port(node, Direction::Input, PortKind::Value, Type::Double)?;
            self.connect(source, input, EdgeKind::CopyableValue)?;
        }

        let clbits: Vec<u32> = self
            .store
            .clbits_of(instruction)
            .unwrap_or(&[])
            .iter()
            .map(|clbit| clbit.0)
            .collect();
        for clbit in clbits {
            let output = self.port(node, Direction::Output, PortKind::Value, Type::Result)?;
            self.clbit_source.insert(clbit, output);
            let sink = self.valued_node(
                format!("{STORE}{clbit}"),
                Vec::new(),
                CircuitOrigin::Instruction(index),
                NodeValue::ClassicalBit(clbit),
            )?;
            let input = self.port(sink, Direction::Input, PortKind::Value, Type::Result)?;
            self.connect(output, input, EdgeKind::CopyableValue)?;
        }

        // Ordering on each qubit is already carried by the capability chain,
        // so an observable node needs only to be demanded: nothing consumes a
        // measurement, and the retention rule accepts a demanded node.
        if observable {
            self.builder.demand(node);
        }
        Ok(())
    }

    /// Lowers a control-flow construct, which is a conditional or nothing.
    fn lower_control_flow(&mut self, index: usize, control: &ControlFlow) -> Result<(), Error> {
        let ControlFlow::IfElse(if_else) = control else {
            return Err(Error::Unsupported {
                operation: control.name().to_string(),
            });
        };
        self.lower_conditional(index, if_else)
    }

    /// Lowers a conditional into a node that owns one region per branch.
    ///
    /// A branch's argument binds to the port that feeds it, and a binding
    /// crosses exactly one region boundary. That is a real constraint, and a
    /// conditional inside a branch does need a port at the intervening level to
    /// bind through. It is a precondition rather than a barrier, because the
    /// lowering can create that port instead of having to find one: a carried
    /// capability is handed through below, and a predicate is relayed in when
    /// the branch is entered.
    fn lower_conditional(&mut self, index: usize, if_else: &IfElse) -> Result<(), Error> {
        let clbit = self.condition_clbit(if_else.condition, index)?;
        let predicate = *self
            .clbit_source
            .get(&clbit)
            .ok_or(Error::UnknownCondition { index, clbit })?;
        let bodies = [Some(if_else.then_block), if_else.else_block];
        let carried = self.carried_qubits(&bodies, index)?;

        // A port an inner branch binds against has to sit in that branch's
        // immediate parent. A carried qubit the enclosing branch has not
        // touched yet still has its capability one level further up, so hand it
        // through here first: `carry` declares the crossing and leaves a port in
        // the region this conditional is being built in. Only the qubits this
        // conditional carries and the enclosing branch still owes are handed
        // through, so at the top level, where nothing is owed, this does
        // nothing at all.
        let owed: Vec<u32> = carried
            .iter()
            .copied()
            .filter(|qubit| self.crossings.contains(qubit))
            .collect();
        for qubit in owed {
            self.carry(qubit, index)?;
        }

        let gamma = self.node(GAMMA, Vec::new(), CircuitOrigin::Instruction(index))?;
        let selector = self.port(gamma, Direction::Input, PortKind::Value, Type::Result)?;
        self.connect(predicate, selector, EdgeKind::CopyableValue)?;
        let mut merged = Vec::with_capacity(carried.len());
        for _ in &carried {
            merged.push(self.port(gamma, Direction::Output, PortKind::Capability, Type::Qubit)?);
        }

        let entering: Vec<PortId> = carried.iter().map(|q| self.live[*q as usize]).collect();
        for block in bodies.into_iter().flatten() {
            self.lower_branch(index, gamma, block, &carried, &entering, &merged)?;
        }
        for (position, qubit) in carried.iter().enumerate() {
            self.live[*qubit as usize] = merged[position];
        }
        Ok(())
    }

    /// Lowers one branch into a region the conditional owns.
    ///
    /// Everything the branch changes about the enclosing region's bookkeeping
    /// is restored here rather than cleared, because at one level the enclosing
    /// state is always empty and the two are indistinguishable, while at two
    /// they are not.
    fn lower_branch(
        &mut self,
        index: usize,
        gamma: NodeId,
        block: BlockId,
        carried: &[u32],
        entering: &[PortId],
        merged: &[PortId],
    ) -> Result<(), Error> {
        let parent = self.region;
        let branch = self.builder.add_region(Some(parent), RegionKind::Gamma);
        self.builder
            .declare_region_owner(branch, gamma)
            .map_err(|error| Error::Build {
                reason: format!("{error:?}"),
            })?;
        self.region = branch;
        let enclosing_crossings =
            std::mem::replace(&mut self.crossings, carried.iter().copied().collect());
        let enclosing_clbits = self.clbit_source.clone();
        for (position, qubit) in carried.iter().enumerate() {
            self.live[*qubit as usize] = entering[position];
        }

        let mut relayed = BTreeMap::new();
        let lowered = self.lower_branch_body(index, branch, block, carried, merged, &mut relayed);

        self.clbit_source = self.visible_after(enclosing_clbits, &relayed);
        self.crossings = enclosing_crossings;
        self.region = parent;
        lowered
    }

    /// The fallible middle of a branch, split out so the enclosing region's
    /// state is restored on the error path as well as the ordinary one.
    fn lower_branch_body(
        &mut self,
        index: usize,
        branch: RegionId,
        block: BlockId,
        carried: &[u32],
        merged: &[PortId],
        relayed: &mut BTreeMap<u32, PortId>,
    ) -> Result<(), Error> {
        self.relay_conditions(branch, block, index, relayed)?;
        for instruction in self.body_of(block, index)? {
            self.lower_instruction(index, &instruction)?;
        }
        for qubit in carried {
            if self.crossings.contains(qubit) {
                self.carry(*qubit, index)?;
            }
        }
        for (position, qubit) in carried.iter().enumerate() {
            self.bind(
                branch,
                merged[position],
                self.live[*qubit as usize],
                InterfaceRole::Result,
            )?;
        }
        Ok(())
    }

    /// Relays into a branch every classical bit its conditionals read.
    ///
    /// Building them all on entry is what makes the rule stateless. The
    /// lowering cannot ask the builder where a port lives, so instead it holds
    /// an invariant: inside a branch, a classical bit's source always names a
    /// port in the region being built, which makes a nested conditional's
    /// selector edge an ordinary intra-region edge needing no binding of its
    /// own. A bit the branch supersedes with its own measurement simply
    /// overwrites the relay, and a relay nothing consumes is not a diagnostic.
    fn relay_conditions(
        &mut self,
        branch: RegionId,
        block: BlockId,
        index: usize,
        relayed: &mut BTreeMap<u32, PortId>,
    ) -> Result<(), Error> {
        let mut reached = Reached::default();
        self.reach_block(block, index, &mut reached)?;
        for clbit in reached.clbits {
            // A bit nothing has measured is reported by the conditional that
            // reads it, which says which instruction asked for it.
            let Some(source) = self.clbit_source.get(&clbit).copied() else {
                continue;
            };
            let node = self.node(RELAY, Vec::new(), CircuitOrigin::Instruction(index))?;
            let input = self.port(node, Direction::Input, PortKind::Value, Type::Result)?;
            self.bind(branch, source, input, InterfaceRole::Argument)?;
            self.connect(source, input, EdgeKind::CopyableValue)?;
            let output = self.port(node, Direction::Output, PortKind::Value, Type::Result)?;
            self.clbit_source.insert(clbit, output);
            relayed.insert(clbit, output);
        }
        Ok(())
    }

    /// What the enclosing region can still see of its classical bits after a
    /// branch has been lowered.
    ///
    /// A port the branch produced is in the branch's region, so nothing outside
    /// can reach it. A bit the branch wrote is therefore dropped rather than
    /// reverted: the enclosing region can neither read the value the branch
    /// produced nor honestly claim the one it replaced. A relay is the one
    /// exception, because it stands for the very value it was handed.
    fn visible_after(
        &self,
        enclosing: BTreeMap<u32, PortId>,
        relayed: &BTreeMap<u32, PortId>,
    ) -> BTreeMap<u32, PortId> {
        enclosing
            .into_iter()
            .filter(|(clbit, source)| {
                let current = self.clbit_source.get(clbit);
                current == Some(source) || current == relayed.get(clbit)
            })
            .collect()
    }

    /// Hands a qubit through a branch that never touched it.
    fn carry(&mut self, qubit: u32, index: usize) -> Result<(), Error> {
        let node = self.node(CARRY, Vec::new(), CircuitOrigin::Instruction(index))?;
        let input = self.port(node, Direction::Input, PortKind::Capability, Type::Qubit)?;
        self.take(qubit, input)?;
        let output = self.port(node, Direction::Output, PortKind::Capability, Type::Qubit)?;
        self.live[qubit as usize] = output;
        Ok(())
    }

    /// The classical bit a condition reads, which is all this path supports.
    ///
    /// The published surface writes a conditional as a comparison against one,
    /// and that comparison carries nothing the result does not already carry,
    /// so the direct path folds it away and this reads through it. Anything
    /// else, a comparison against zero included, needs a negation the graph has
    /// no node for.
    fn condition_clbit(&self, condition: ExprId, index: usize) -> Result<u32, Error> {
        let refuse = || Error::Unsupported {
            operation: format!("condition of instruction {index}"),
        };
        let exprs = self.store.exprs();
        match exprs.get(condition).ok_or_else(refuse)? {
            Expr::Clbit(clbit) => Ok(*clbit),
            Expr::Binary(BinaryOp::Eq, left, right) => {
                let (Some(left), Some(right)) = (exprs.get(*left), exprs.get(*right)) else {
                    return Err(refuse());
                };
                match (left, right) {
                    (Expr::Clbit(clbit), other) | (other, Expr::Clbit(clbit)) if is_set(other) => {
                        Ok(*clbit)
                    }
                    _ => Err(refuse()),
                }
            }
            _ => Err(refuse()),
        }
    }

    /// The qubits a conditional has to carry across its branches.
    ///
    /// Every branch produces the same set, so the set is the union of what any
    /// of them touches rather than the intersection.
    fn carried_qubits(&self, bodies: &[Option<BlockId>], index: usize) -> Result<Vec<u32>, Error> {
        let mut reached = Reached::default();
        for block in bodies.iter().copied().flatten() {
            self.reach_block(block, index, &mut reached)?;
        }
        Ok(reached.qubits.into_iter().collect())
    }

    /// Unions what a block reaches into `reached`, descending into any
    /// conditional the block holds.
    ///
    /// A control-flow instruction's own operand list says nothing about what
    /// its blocks touch, so a walk that reads only that list misses everything
    /// a nested conditional uses. Two things go wrong then. The enclosing
    /// branch never rebases the missing qubit's live capability, so an inner
    /// branch binds against a port two levels up and `bind_interface` refuses
    /// it. And `live` is re-seated and restored only for carried qubits, so a
    /// qubit touched only inside a nested conditional would leak its
    /// branch-local port into the enclosing region, to be consumed there by a
    /// later gate or by the release. Narrowing this recursion breaks the second
    /// one silently.
    fn reach_block(
        &self,
        block: BlockId,
        index: usize,
        reached: &mut Reached,
    ) -> Result<(), Error> {
        for instruction in self.body_of(block, index)? {
            for qubit in self.store.qubits_of(&instruction).unwrap_or(&[]) {
                if qubit.0 as usize >= self.live.len() {
                    return Err(Error::UnknownQubit {
                        index,
                        qubit: qubit.0,
                    });
                }
                reached.qubits.insert(qubit.0);
            }
            // Only a conditional is descended into. The other control-flow
            // variants are refused when the body is lowered, so collecting what
            // they reach would describe a circuit this path never builds.
            if let PackedOperation::ControlFlow(control) = &instruction.operation
                && let ControlFlow::IfElse(if_else) = control.as_ref()
            {
                // A condition this path cannot read is refused when the body is
                // lowered. Naming it here would move that refusal earlier and
                // report it against the wrong construct.
                if let Ok(clbit) = self.condition_clbit(if_else.condition, index) {
                    reached.clbits.insert(clbit);
                }
                for block in [Some(if_else.then_block), if_else.else_block]
                    .into_iter()
                    .flatten()
                {
                    self.reach_block(block, index, reached)?;
                }
            }
        }
        Ok(())
    }

    /// Reads a block's instructions, so they can be lowered without borrowing.
    fn body_of(&self, block: BlockId, index: usize) -> Result<Vec<Instruction>, Error> {
        self.store
            .block(block)
            .map(|body| body.instructions.clone())
            .ok_or(Error::UnknownBlock { index })
    }

    /// Consumes a qubit's live capability, declaring the crossing when it is one.
    ///
    /// A cross-region edge is legal only where an interface declares it, and
    /// the declaration has to exist first, so the first use of an argument
    /// inside a branch is what registers it.
    fn take(&mut self, qubit: u32, input: PortId) -> Result<(), Error> {
        let source = self.live[qubit as usize];
        if self.crossings.remove(&qubit) {
            self.bind(self.region, source, input, InterfaceRole::Argument)?;
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
            .map_err(|error| Error::Build {
                reason: format!("{error:?}"),
            })
    }

    /// Builds a node standing for one literal parameter value.
    ///
    /// The value is on the node's payload. The name still spells it out because
    /// the constant-folding rewrite still matches on the name, and retiring the
    /// two together is a separate change.
    fn constant(&mut self, value: f64, index: usize) -> Result<PortId, Error> {
        let node = self.valued_node(
            format!("{CONSTANT}{value:?}"),
            Vec::new(),
            CircuitOrigin::Instruction(index),
            NodeValue::Number(Real::new(value)),
        )?;
        self.port(node, Direction::Output, PortKind::Value, Type::Double)
    }

    /// Builds a node standing for one angle that still names a parameter.
    ///
    /// The name carries no value at all here: an angle is a tree, and the
    /// operation name was never able to hold one, which is why a parameterized
    /// circuit could not enter the graph before the payload existed.
    fn symbolic_constant(&mut self, angle: Angle, index: usize) -> Result<PortId, Error> {
        let node = self.valued_node(
            format!("{CONSTANT}{SYMBOLIC}"),
            Vec::new(),
            CircuitOrigin::Instruction(index),
            NodeValue::Angle(angle),
        )?;
        self.port(node, Direction::Output, PortKind::Value, Type::Double)
    }

    /// The graph's angle for one the circuit holds.
    ///
    /// A parameter is carried by name, so the resulting graph is readable
    /// without the store that produced it.
    fn angle_of(&self, param: ParamValue, index: usize) -> Result<Angle, Error> {
        match param {
            ParamValue::Literal(value) => Ok(Angle::literal(value)),
            ParamValue::Parameter(id) => Ok(Angle::Parameter {
                name: self.parameter_name(id, index)?,
            }),
            ParamValue::Expr(id) => self.angle_expr(id, index),
        }
    }

    fn angle_expr(&self, id: AngleId, index: usize) -> Result<Angle, Error> {
        let node = self
            .store
            .angles()
            .get(id)
            .ok_or(Error::SymbolicParameter { index })?;
        Ok(match *node {
            AngleExpr::Literal(value) => Angle::literal(value),
            AngleExpr::Parameter(parameter) => Angle::Parameter {
                name: self.parameter_name(parameter, index)?,
            },
            AngleExpr::Neg(inner) => Angle::Neg {
                operand: Box::new(self.angle_expr(inner, index)?),
            },
            AngleExpr::Binary(op, left, right) => Angle::Binary {
                op: match op {
                    StoreAngleOp::Add => AngleOp::Add,
                    StoreAngleOp::Sub => AngleOp::Sub,
                    StoreAngleOp::Mul => AngleOp::Mul,
                    StoreAngleOp::Div => AngleOp::Div,
                },
                left: Box::new(self.angle_expr(left, index)?),
                right: Box::new(self.angle_expr(right, index)?),
            },
        })
    }

    fn parameter_name(&self, id: ParameterId, index: usize) -> Result<String, Error> {
        self.store
            .parameters()
            .get(id.0 as usize)
            .map(|parameter| parameter.name.clone())
            .ok_or(Error::SymbolicParameter { index })
    }

    /// Adds a node and records which circuit construct it came from, so the
    /// origin is attached at the moment the node is created rather than
    /// reconstructed later from operation names.
    fn node(
        &mut self,
        operation: impl Into<String>,
        effects: Vec<EffectEvent>,
        origin: CircuitOrigin,
    ) -> Result<NodeId, Error> {
        self.build_node(
            operation,
            effects,
            NodePayload {
                value: None,
                origin: Some(origin.into()),
            },
        )
    }

    /// Adds a node that carries a value as well as an origin.
    fn valued_node(
        &mut self,
        operation: impl Into<String>,
        effects: Vec<EffectEvent>,
        origin: CircuitOrigin,
        value: NodeValue,
    ) -> Result<NodeId, Error> {
        self.build_node(
            operation,
            effects,
            NodePayload {
                value: Some(value),
                origin: Some(origin.into()),
            },
        )
    }

    fn build_node(
        &mut self,
        operation: impl Into<String>,
        effects: Vec<EffectEvent>,
        payload: NodePayload,
    ) -> Result<NodeId, Error> {
        let node = self
            .builder
            .add_node(self.region, operation, effects)
            .map_err(|error| Error::Build {
                reason: format!("{error:?}"),
            })?;
        self.builder
            .declare_payload(node, payload)
            .map_err(|error| Error::Build {
                reason: format!("{error:?}"),
            })?;
        Ok(node)
    }

    fn port(
        &mut self,
        node: NodeId,
        direction: Direction,
        kind: PortKind,
        ty: Type,
    ) -> Result<PortId, Error> {
        // A capability is linear because a qubit cannot be duplicated; a value
        // may be read freely; anything else is an effect token, which is not
        // copied either.
        let usage = match kind {
            PortKind::Capability => Usage::Linear,
            PortKind::Value => Usage::Copyable,
            _ => Usage::Affine,
        };
        self.builder
            .add_port(node, direction, kind, ty, Stage::Residual, usage)
            .map_err(|error| Error::Build {
                reason: format!("{error:?}"),
            })
    }

    fn connect(&mut self, source: PortId, target: PortId, kind: EdgeKind) -> Result<(), Error> {
        self.builder
            .connect(qsc_qhdg::Edge {
                source,
                target,
                kind,
            })
            .map_err(|error| Error::Build {
                reason: format!("{error:?}"),
            })
    }
}

const fn effect(mode: EffectMode, observable: bool) -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode,
        resource: qsc_qhdg::EffectResource::Unknown,
        observable,
    }
}

/// An effect on one named qubit.
///
/// Only allocation, release, and observation name their resource. Naming one on
/// an ordinary gate would add a heap allocation per node to a field no graph
/// consumer reads yet.
fn qubit_effect(mode: EffectMode, observable: bool, qubit: u32) -> EffectEvent {
    EffectEvent {
        domain: EffectDomain::Quantum,
        mode,
        resource: qsc_qhdg::EffectResource::Named(format!("qubit:{qubit}")),
        observable,
    }
}

/// Whether a condition's other operand stands for a set result.
fn is_set(expr: &Expr) -> bool {
    matches!(expr, Expr::Literal(Literal::Int(1) | Literal::Bool(true)))
}

/// Whether an operation is one the graph must not discard.
///
/// Exhaustive rather than defaulting, because nothing may become observable
/// without also touching a qubit: the capability chain is the only thing
/// ordering an observable node, so a new variant has to be classified here.
fn is_observable(operation: &PackedOperation) -> bool {
    match operation {
        PackedOperation::Standard(gate) => matches!(
            gate,
            StandardGate::MeasureX | StandardGate::MeasureY | StandardGate::MeasureZ
        ),
        PackedOperation::Reset => true,
        PackedOperation::Custom(_)
        | PackedOperation::Unitary(_)
        | PackedOperation::Barrier { .. }
        | PackedOperation::Delay { .. }
        | PackedOperation::ControlFlow(_) => false,
    }
}

/// The graph's name for a circuit operation.
///
/// Named rather than derived, so a gate added to the circuit has to be given a
/// graph name deliberately instead of arriving under whatever string the
/// circuit happened to print.
fn operation_name(operation: &PackedOperation) -> Result<&'static str, Error> {
    let gate = match operation {
        PackedOperation::Standard(gate) => *gate,
        PackedOperation::Reset => return Ok(RESET),
        PackedOperation::Barrier { .. } => return Ok(BARRIER),
        PackedOperation::ControlFlow(control) => {
            return Err(Error::Unsupported {
                operation: control.name().to_string(),
            });
        }
        other => {
            return Err(Error::Unsupported {
                operation: other.name().to_string(),
            });
        }
    };
    Ok(match gate {
        StandardGate::I => "quantum.i",
        StandardGate::X => "quantum.x",
        StandardGate::Y => "quantum.y",
        StandardGate::Z => "quantum.z",
        StandardGate::H => "quantum.h",
        StandardGate::S => "quantum.s",
        StandardGate::SDag => "quantum.s_adj",
        StandardGate::SqrtX => "quantum.sx",
        StandardGate::SqrtXDag => "quantum.sx_adj",
        StandardGate::T => "quantum.t",
        StandardGate::TDag => "quantum.t_adj",
        StandardGate::Cx => "quantum.cx",
        StandardGate::Cy => "quantum.cy",
        StandardGate::Cz => "quantum.cz",
        StandardGate::Swap => "quantum.swap",
        StandardGate::Ccx => "quantum.ccx",
        StandardGate::Cswap => "quantum.cswap",
        StandardGate::Rx => "quantum.rx",
        StandardGate::Ry => "quantum.ry",
        StandardGate::Rz => "quantum.rz",
        StandardGate::Rxx => "quantum.rxx",
        StandardGate::Ryy => "quantum.ryy",
        StandardGate::Rzz => "quantum.rzz",
        StandardGate::Crx => "quantum.crx",
        StandardGate::Cry => "quantum.cry",
        StandardGate::Crz => "quantum.crz",
        StandardGate::MeasureX => "quantum.measure_x",
        StandardGate::MeasureY => "quantum.measure_y",
        StandardGate::MeasureZ => "quantum.measure_z",
        StandardGate::PrepareX => "quantum.prepare_x",
        StandardGate::PrepareY => "quantum.prepare_y",
        StandardGate::PrepareZ => "quantum.prepare_z",
    })
}

/// The gate a graph operation name stands for.
///
/// Derived by searching the forward mapping rather than written as a second
/// table, so the two directions cannot drift apart. A name this does not
/// recognise is outside the circuit-expressible subset, which the raise reports
/// by name rather than treating as a bug.
pub(crate) fn gate_from_name(name: &str) -> Option<StandardGate> {
    raise::ALL_GATES.into_iter().find(|gate| {
        operation_name(&PackedOperation::Standard(*gate)).is_ok_and(|known| known == name)
    })
}

/// Every operation name a standard gate lowers to.
///
/// Read from the same table rather than restated, so anything asking what this
/// compiler's circuit vocabulary is gets the answer from the one place that
/// decides it.
pub(crate) fn gate_names() -> impl Iterator<Item = &'static str> {
    raise::ALL_GATES
        .into_iter()
        .filter_map(|gate| operation_name(&PackedOperation::Standard(gate)).ok())
}
