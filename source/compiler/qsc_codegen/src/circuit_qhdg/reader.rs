// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Reading a circuit-vocabulary graph.
//!
//! Two consumers need the same four answers out of a graph: which node runs
//! when, which qubit index a capability stands for, what value a constant node
//! carries, and which classical bit a store node targets. Raising reads them to
//! rebuild a circuit; code generation reads them to emit runtime IR. Answering
//! them twice would give one graph shape two readers free to disagree, and the
//! disagreement would not be loud: a qubit numbered differently on one side
//! emits output that runs and is wrong.
//!
//! What this does not do is decide what an operation means. It checks only that
//! a name belongs to the vocabulary the circuit lowering emits, so a graph the
//! compiler's own importer produced is refused here rather than mis-read. Each
//! consumer maps a name in that vocabulary to its own target, because the sets
//! they can express are not the same one.
//!
//! Region structure is answered here for the same reason. A branch is a region
//! a node owns, so the order its contents run in, the qubits crossing into it,
//! and the results it agrees to produce are all questions about the same walk.
//! A region kind this walk does not handle refuses by name, because reading one
//! as though nothing guarded it produces a program that runs and is wrong.

use std::collections::BTreeMap;

use qsc_qhdg::{
    Angle, ExitKind, Graph, InterfaceRole, NodeId, PortId, PortKind, RegionId, RegionKind,
    StableNodeId,
};
use qsc_semantic_contracts::Type;

use super::{
    ALLOCATE, CARRY, CLBIT, CONSTANT, CircuitOrigin, GAMMA, OPAQUE, PHASE, QUANTUM, RELAY, RELEASE,
    STORE, THETA,
};
use crate::graph_order::{OrderError, region_orders};

/// Why a graph could not be read as a circuit-vocabulary program.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadError {
    /// The graph names an operation from some other vocabulary.
    ///
    /// The compiler's own graph importer produces one, and its nodes carry
    /// control-exit ports and expression names with no operands anywhere, so
    /// reading it as a circuit would invent both.
    #[error("operation '{operation}' is not part of the circuit vocabulary")]
    Vocabulary { operation: String },
    /// The graph nests a region of a kind this traversal does not handle.
    ///
    /// A gamma is read as the conditional it is. The rest refuse here rather
    /// than falling through to a walk that would emit their contents as though
    /// nothing guarded them.
    #[error("a {kind} region has no straight-line form")]
    Region { kind: String },
    /// The graph is inside the vocabulary and not shaped like a lowered circuit.
    #[error("the graph is not circuit-shaped: {reason}")]
    Malformed { reason: String },
}

/// One instruction node with its operands resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadInstruction {
    /// The node this came from, so a caller can key its own state by identity
    /// rather than by position.
    pub id: StableNodeId,
    /// The graph's own name for the operation, left for the consumer to map.
    pub operation: String,
    pub qubits: Vec<u32>,
    pub clbits: Vec<u32>,
    /// One entry per value operand. An angle that still names a parameter is
    /// carried as it stands rather than evaluated.
    pub params: Vec<Angle>,
    pub origin: Option<CircuitOrigin>,
}

/// One step of a program.
///
/// Nested rather than flat with markers, because both consumers need the
/// nesting anyway: the raise rebuilds a block per branch and code generation
/// emits a basic block per branch. Markers would push the same stack tracking
/// into each of them instead of removing it from both.
#[derive(Clone, Debug, PartialEq)]
pub enum ReadItem {
    Instruction(ReadInstruction),
    Conditional(ReadConditional),
    Loop(ReadLoop),
}

/// A conditional and the classical bit that selects its branch.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadConditional {
    pub id: StableNodeId,
    pub clbit: u32,
    pub then_items: Vec<ReadItem>,
    /// `None` when the conditional has no alternative branch, which is a
    /// different program from one whose alternative is empty.
    pub else_items: Option<Vec<ReadItem>>,
    pub origin: Option<CircuitOrigin>,
}

/// A loop and the qubits it hands from one iteration to the next.
///
/// The body is kept nested rather than flattened for the reason the whole
/// enum is nested: a consumer that emitted these steps inline would emit a
/// program that runs them once, which is a different program.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadLoop {
    pub id: StableNodeId,
    /// One entry per carried binding, in the order the region declared them.
    /// A qubit here is the same qubit on every iteration, which is what the
    /// back edge asserts and what makes the body readable at all.
    pub carried: Vec<u32>,
    pub body: Vec<ReadItem>,
    pub origin: Option<CircuitOrigin>,
}

/// What a circuit-vocabulary graph says, with every operand resolved.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadProgram {
    pub qubit_count: u32,
    pub clbit_count: u32,
    pub global_phase: f64,
    /// Program order, bookkeeping nodes already removed.
    pub items: Vec<ReadItem>,
}

/// Reads a circuit-vocabulary graph into a resolved instruction sequence.
///
/// # Errors
///
/// Returns [`ReadError::Vocabulary`] naming the operation when the graph comes
/// from a different producer, and [`ReadError::Malformed`] when it is in the
/// vocabulary but not shaped like a lowered circuit.
pub fn read_program(graph: &Graph) -> Result<ReadProgram, ReadError> {
    Reader::new(graph)?.run()
}

/// Which qubits each instruction node acts on, keyed by durable identity.
///
/// A pass reasoning about qubits needs the same answer the raise and code
/// generation get, so this is the same walk rather than a second resolver that
/// would be free to number qubits differently without anything failing.
///
/// # Errors
///
/// Fails for the same reasons [`read_program`] does.
pub fn qubit_operands(graph: &Graph) -> Result<BTreeMap<StableNodeId, Vec<u32>>, ReadError> {
    let mut operands = BTreeMap::new();
    collect_operands(&read_program(graph)?.items, &mut operands);
    Ok(operands)
}

fn collect_operands(items: &[ReadItem], operands: &mut BTreeMap<StableNodeId, Vec<u32>>) {
    for item in items {
        match item {
            ReadItem::Instruction(instruction) => {
                operands.insert(instruction.id, instruction.qubits.clone());
            }
            ReadItem::Conditional(conditional) => {
                collect_operands(&conditional.then_items, operands);
                if let Some(items) = &conditional.else_items {
                    collect_operands(items, operands);
                }
            }
            ReadItem::Loop(loop_item) => collect_operands(&loop_item.body, operands),
        }
    }
}

/// Whether a name belongs to the vocabulary the circuit lowering emits.
///
/// The carrier term is admitted without being a circuit name. The RIR stage
/// reaches this reader to find out which qubits an instruction touches, and a
/// graph from that stage holds carriers, so refusing them here would leave the
/// stage with no facts and declining on every program.
fn in_vocabulary(name: &str) -> bool {
    name.starts_with(QUANTUM)
        || name == CLBIT
        || name == GAMMA
        || name == OPAQUE
        || name.starts_with(STORE)
        || name.starts_with(CONSTANT)
        || name.starts_with(PHASE)
}

/// Whether a node records circuit state rather than an instruction.
///
/// A carrier is neither, and is closer to state from here: it stands for an
/// instruction this reader has no reading of, so it is skipped rather than
/// resolved into one.
fn is_bookkeeping(name: &str) -> bool {
    name == ALLOCATE
        || name == RELEASE
        || name == CLBIT
        || name == OPAQUE
        || name.starts_with(CONSTANT)
        || name.starts_with(STORE)
        || name.starts_with(PHASE)
}

/// What a theta region must declare before its body can be read as a loop.
///
/// Checked here, before program order, rather than at the loop node. The
/// back-edge declaration is what excludes the back edge from the topological
/// sort, so a theta missing it refuses as an unspecified cycle instead: true,
/// and silent about which declaration is absent.
///
/// Both halves are required because either one alone describes something else.
/// A body with no back edge is a block that runs once, and a back edge that
/// carries nothing hands no wire to the next iteration, so reading either as a
/// loop would invent the part that is missing.
fn check_loop_interface(region: &qsc_qhdg::Region) -> Result<(), ReadError> {
    if !region
        .interface
        .exits
        .iter()
        .any(|(kind, _)| *kind == ExitKind::Backedge)
    {
        return Err(ReadError::Malformed {
            reason: "a loop body declares no back edge, so nothing says it repeats".to_string(),
        });
    }
    if !region
        .interface
        .bindings
        .iter()
        .any(|binding| binding.role == InterfaceRole::Carried)
    {
        return Err(ReadError::Malformed {
            reason: "a loop body carries nothing across its back edge".to_string(),
        });
    }
    Ok(())
}

/// One node, flattened so the walk does not repeatedly search the graph.
struct Entry {
    id: StableNodeId,
    node: NodeId,
    name: String,
    inputs: Vec<PortId>,
    outputs: Vec<PortId>,
    origin: Option<CircuitOrigin>,
    payload: qsc_qhdg::NodePayload,
}

struct Reader<'g> {
    graph: &'g Graph,
    /// The region the program itself is in.
    module: RegionId,
    /// Nodes in program order, one order per region.
    entries: BTreeMap<RegionId, Vec<Entry>>,
    /// What each port is, so an operand's role is known without a search.
    ports: BTreeMap<PortId, (PortKind, Type)>,
    /// Which port feeds each consuming port, so an operand traces back.
    source_of: BTreeMap<PortId, PortId>,
    /// Which qubit a capability currently stands for.
    qubit_of: BTreeMap<PortId, u32>,
    /// The value a constant node produces.
    value_of: BTreeMap<PortId, Angle>,
    /// The classical bit a measurement result is stored into.
    clbit_of: BTreeMap<PortId, u32>,
    qubit_count: u32,
    clbit_count: u32,
    global_phase: f64,
}

impl<'g> Reader<'g> {
    fn new(graph: &'g Graph) -> Result<Self, ReadError> {
        let mut module = None;
        for region in graph.regions() {
            match region.kind {
                RegionKind::ModuleOmega => module = Some(region.id),
                // Gamma and theta are the nested kinds with a traversal here.
                // The rest still refuse by name rather than falling through to
                // a walk that would emit their contents as though nothing
                // guarded them.
                RegionKind::Gamma if region.owner.is_some() => {}
                RegionKind::Theta if region.owner.is_some() => check_loop_interface(region)?,
                RegionKind::Gamma => {
                    return Err(ReadError::Malformed {
                        reason: "a branch region is not the body of any node".to_string(),
                    });
                }
                RegionKind::Theta => {
                    return Err(ReadError::Malformed {
                        reason: "a loop region is not the body of any node".to_string(),
                    });
                }
                kind => {
                    return Err(ReadError::Region {
                        kind: format!("{kind:?}"),
                    });
                }
            }
        }
        let module = module.ok_or_else(|| ReadError::Malformed {
            reason: "the graph has no module region".to_string(),
        })?;
        Ok(Self {
            graph,
            module,
            entries: program_order(graph)?,
            ports: graph
                .ports()
                .map(|port| (port.id, (port.kind, port.ty.clone())))
                .collect(),
            source_of: graph
                .edges()
                .iter()
                .map(|edge| (edge.target, edge.source))
                .collect(),
            qubit_of: BTreeMap::new(),
            value_of: BTreeMap::new(),
            clbit_of: BTreeMap::new(),
            qubit_count: 0,
            clbit_count: 0,
            global_phase: 0.0,
        })
    }

    fn run(mut self) -> Result<ReadProgram, ReadError> {
        self.read_declarations()?;
        let items = self.read_region(self.module)?;
        Ok(ReadProgram {
            qubit_count: self.qubit_count,
            clbit_count: self.clbit_count,
            global_phase: self.global_phase,
            items,
        })
    }

    /// Reads one region's nodes into the steps they stand for.
    fn read_region(&mut self, region: RegionId) -> Result<Vec<ReadItem>, ReadError> {
        let mut items = Vec::new();
        for index in 0..self.entries[&region].len() {
            let name = self.entries[&region][index].name.clone();
            if name == GAMMA {
                let conditional = self.read_conditional(region, index)?;
                items.push(ReadItem::Conditional(conditional));
            } else if name == THETA {
                let loop_item = self.read_loop(region, index)?;
                items.push(ReadItem::Loop(loop_item));
            } else if name == CARRY {
                // Carrying a qubit through a branch that does not touch it is
                // a fact about the interface, not a step of the program, so it
                // resolves its operands and produces nothing.
                self.read_instruction(region, index)?;
            } else if name == RELAY {
                // Relaying a classical bit into the branch that reads it is a
                // fact about the interface too, but it cannot go through
                // `read_instruction`: that resolves a value operand as a
                // constant, and a relayed predicate is a measurement result.
                self.read_relay(region, index)?;
            } else if !is_bookkeeping(&name) {
                items.push(ReadItem::Instruction(self.read_instruction(region, index)?));
            }
        }
        Ok(items)
    }

    /// Carries a classical bit's identity across a relay.
    ///
    /// The relay's output stands for the bit its input was handed, so the
    /// conditional that reads it traces back one hop and finds a known source.
    /// Chains resolve by induction, because a region is read after the region
    /// enclosing it and a relay is ordered before whatever reads it.
    fn read_relay(&mut self, region: RegionId, index: usize) -> Result<(), ReadError> {
        let entry = &self.entries[&region][index];
        let input = *entry.inputs.first().ok_or_else(|| ReadError::Malformed {
            reason: format!("'{RELAY}' reads nothing"),
        })?;
        let output = *entry.outputs.first().ok_or_else(|| ReadError::Malformed {
            reason: format!("'{RELAY}' produces nothing"),
        })?;
        let source = self.trace(input)?;
        let clbit = *self
            .clbit_of
            .get(&source)
            .ok_or_else(|| ReadError::Malformed {
                reason: format!("'{RELAY}' carries a value no measurement produced"),
            })?;
        self.clbit_of.insert(output, clbit);
        Ok(())
    }

    /// Reads a conditional and the branch regions the node owns.
    fn read_conditional(
        &mut self,
        region: RegionId,
        index: usize,
    ) -> Result<ReadConditional, ReadError> {
        let entry = &self.entries[&region][index];
        let (id, node, origin) = (entry.id, entry.node, entry.origin);
        let selector = *entry.inputs.first().ok_or_else(|| ReadError::Malformed {
            reason: "a conditional reads no condition".to_string(),
        })?;
        let source = self.trace(selector)?;
        let clbit = *self
            .clbit_of
            .get(&source)
            .ok_or_else(|| ReadError::Malformed {
                reason: "a conditional reads a value no measurement produced".to_string(),
            })?;

        let branches: Vec<RegionId> = self
            .graph
            .regions()
            .filter(|candidate| candidate.owner == Some(node))
            .map(|candidate| candidate.id)
            .collect();
        let (then_branch, alternative) = match branches.as_slice() {
            [then_branch] => (*then_branch, None),
            [then_branch, else_branch] => (*then_branch, Some(*else_branch)),
            other => {
                return Err(ReadError::Malformed {
                    reason: format!(
                        "a conditional owns {} regions, which is not one branch or two",
                        other.len()
                    ),
                });
            }
        };
        let then_items = self.read_branch(then_branch)?;
        let else_items = alternative
            .map(|branch| self.read_branch(branch))
            .transpose()?;
        Ok(ReadConditional {
            id,
            clbit,
            then_items,
            else_items,
            origin,
        })
    }

    /// Reads one branch and publishes what it produces on its owner's ports.
    ///
    /// A result crossing is a binding rather than an edge, because both of its
    /// endpoints are outputs and an edge runs from an output to an input. So
    /// nothing traces through it, and the owner's port takes its meaning here
    /// instead.
    fn read_branch(&mut self, region: RegionId) -> Result<Vec<ReadItem>, ReadError> {
        let items = self.read_region(region)?;
        let results: Vec<(PortId, PortId)> = self
            .graph
            .regions()
            .find(|candidate| candidate.id == region)
            .map(|candidate| {
                candidate
                    .interface
                    .bindings
                    .iter()
                    .filter(|binding| binding.role == InterfaceRole::Result)
                    .map(|binding| (binding.outer, binding.inner))
                    .collect()
            })
            .unwrap_or_default();
        for (outer, inner) in results {
            let qubit = *self
                .qubit_of
                .get(&inner)
                .ok_or_else(|| ReadError::Malformed {
                    reason: "a branch produces a capability no qubit reached".to_string(),
                })?;
            self.qubit_of.insert(outer, qubit);
        }
        Ok(items)
    }

    /// Reads a loop and the theta region its node owns.
    ///
    /// A carried wire takes its identity from outside the loop rather than
    /// from tracing the body's own input. Tracing it follows the back edge,
    /// which asks the question whose answer is being established: the body's
    /// first use of a carried qubit is fed by the body's last, and the only
    /// port that already names a qubit is the one the loop was entered with.
    fn read_loop(&mut self, region: RegionId, index: usize) -> Result<ReadLoop, ReadError> {
        let entry = &self.entries[&region][index];
        let (id, node, origin) = (entry.id, entry.node, entry.origin);

        let bodies: Vec<RegionId> = self
            .graph
            .regions()
            .filter(|candidate| {
                candidate.owner == Some(node) && candidate.kind == RegionKind::Theta
            })
            .map(|candidate| candidate.id)
            .collect();
        let [body] = bodies.as_slice() else {
            return Err(ReadError::Malformed {
                reason: format!(
                    "a loop owns {} theta regions, which is not one body",
                    bodies.len()
                ),
            });
        };
        let body = *body;
        let bindings = self
            .graph
            .regions()
            .find(|candidate| candidate.id == body)
            .map(|candidate| candidate.interface.bindings.clone())
            .unwrap_or_default();

        let mut carried = Vec::new();
        for binding in bindings.iter().filter(|b| b.role == InterfaceRole::Carried) {
            // A loop may also carry its effect order across the back edge, which
            // the verifier reads as one declaration and no qubit stands behind.
            if self
                .ports
                .get(&binding.inner)
                .is_none_or(|(kind, _)| *kind != PortKind::Capability)
            {
                continue;
            }
            let source = self.trace(binding.outer)?;
            let qubit = *self
                .qubit_of
                .get(&source)
                .ok_or_else(|| ReadError::Malformed {
                    reason: "a loop carries a capability that no qubit produced".to_string(),
                })?;
            self.qubit_of.insert(binding.inner, qubit);
            carried.push(qubit);
        }

        let items = self.read_region(body)?;

        for binding in bindings.iter().filter(|b| b.role == InterfaceRole::Result) {
            let qubit = *self
                .qubit_of
                .get(&binding.inner)
                .ok_or_else(|| ReadError::Malformed {
                    reason: "a loop produces a capability no qubit reached".to_string(),
                })?;
            self.qubit_of.insert(binding.outer, qubit);
        }

        Ok(ReadLoop {
            id,
            carried,
            body: items,
            origin,
        })
    }

    /// Reads everything the lowering recorded that is not an instruction.
    ///
    /// A store node sits after the measurement it reads, so classical targets
    /// cannot be resolved while walking instructions in order. The module
    /// region is read first because qubit numbering comes off the order its
    /// allocate nodes appear in.
    fn read_declarations(&mut self) -> Result<(), ReadError> {
        let regions: Vec<RegionId> = std::iter::once(self.module)
            .chain(
                self.entries
                    .keys()
                    .copied()
                    .filter(|region| *region != self.module),
            )
            .collect();
        for region in regions {
            for index in 0..self.entries[&region].len() {
                self.read_declaration(region, index)?;
            }
        }
        Ok(())
    }

    fn read_declaration(&mut self, region: RegionId, index: usize) -> Result<(), ReadError> {
        let name = self.entries[&region][index].name.clone();
        if !in_vocabulary(&name) {
            return Err(ReadError::Vocabulary { operation: name });
        }
        let declares_program_state = name.starts_with(PHASE) || name == ALLOCATE || name == CLBIT;
        if region != self.module && declares_program_state {
            // Width and phase describe the whole program, so a branch
            // declaring one would make what the program is depend on which
            // branch ran.
            return Err(ReadError::Malformed {
                reason: format!("'{name}' declares program state inside a branch"),
            });
        }
        if name.starts_with(PHASE) {
            self.global_phase = self.number(region, index)?;
        } else if name == ALLOCATE {
            let port = self.first_output(region, index)?;
            self.qubit_of.insert(port, self.qubit_count);
            self.qubit_count += 1;
        } else if name == CLBIT {
            self.clbit_count += 1;
        } else if name.starts_with(CONSTANT) {
            let value = self.angle(region, index)?;
            let port = self.first_output(region, index)?;
            self.value_of.insert(port, value);
        } else if name.starts_with(STORE) {
            self.read_store(region, index, &name)?;
        }
        Ok(())
    }

    /// Records which classical bit a store node's measurement wrote.
    fn read_store(&mut self, region: RegionId, index: usize, name: &str) -> Result<(), ReadError> {
        let entry = &self.entries[&region][index];
        let clbit = entry
            .payload
            .classical_bit_value()
            .ok_or_else(|| ReadError::Malformed {
                reason: format!("'{name}' does not name a classical bit"),
            })?;
        let target = *entry.inputs.first().ok_or_else(|| ReadError::Malformed {
            reason: format!("'{name}' reads nothing"),
        })?;
        let source = self.trace(target)?;
        self.clbit_of.insert(source, clbit);
        Ok(())
    }

    /// The real constant a declaration node carries.
    fn number(&self, region: RegionId, index: usize) -> Result<f64, ReadError> {
        let entry = &self.entries[&region][index];
        entry
            .payload
            .number_value()
            .ok_or_else(|| ReadError::Malformed {
                reason: format!("'{}' carries no value", entry.name),
            })
    }

    /// The angle a constant node carries, literal or still symbolic.
    fn angle(&self, region: RegionId, index: usize) -> Result<Angle, ReadError> {
        let entry = &self.entries[&region][index];
        if let Some(angle) = entry.payload.angle_value() {
            return Ok(angle.clone());
        }
        entry
            .payload
            .number_value()
            .map(Angle::literal)
            .ok_or_else(|| ReadError::Malformed {
                reason: format!("'{}' carries no value", entry.name),
            })
    }

    fn read_instruction(
        &mut self,
        region: RegionId,
        index: usize,
    ) -> Result<ReadInstruction, ReadError> {
        let entry = &self.entries[&region][index];
        let (id, origin) = (entry.id, entry.origin);
        let name = entry.name.clone();
        let inputs = entry.inputs.clone();
        let outputs = entry.outputs.clone();

        // Capability inputs and outputs were created in matching order, so the
        // qubit an input names is the qubit its paired output carries onward.
        let taken = self.filter_kind(&inputs, PortKind::Capability);
        let given = self.filter_kind(&outputs, PortKind::Capability);
        if taken.len() != given.len() {
            return Err(ReadError::Malformed {
                reason: format!(
                    "'{name}' takes {} qubits and returns {}",
                    taken.len(),
                    given.len()
                ),
            });
        }

        let mut qubits = Vec::with_capacity(taken.len());
        for (input, output) in taken.iter().zip(given.iter()) {
            let qubit = self.qubit_at(*input, &name)?;
            self.qubit_of.insert(*output, qubit);
            qubits.push(qubit);
        }

        let mut params = Vec::new();
        for input in self.filter_kind(&inputs, PortKind::Value) {
            let source = self.trace(input)?;
            let value = self
                .value_of
                .get(&source)
                .ok_or_else(|| ReadError::Malformed {
                    reason: format!("'{name}' reads a value that is not a constant"),
                })?
                .clone();
            params.push(value);
        }

        let clbits = outputs
            .iter()
            .filter_map(|port| self.clbit_of.get(port).copied())
            .collect::<Vec<_>>();

        Ok(ReadInstruction {
            id,
            operation: name,
            qubits,
            clbits,
            params,
            origin,
        })
    }

    /// The qubit a capability operand names.
    ///
    /// An operand normally traces back to whatever produced it. A carried wire
    /// cannot: the only edge into it is the back edge from the iteration
    /// before, which is the answer rather than the question. So a port the
    /// loop's interface published already stands for a qubit, and is read
    /// before anything is traced.
    fn qubit_at(&self, port: PortId, name: &str) -> Result<u32, ReadError> {
        if let Some(qubit) = self.qubit_of.get(&port) {
            return Ok(*qubit);
        }
        let source = self.trace(port)?;
        self.qubit_of
            .get(&source)
            .copied()
            .ok_or_else(|| ReadError::Malformed {
                reason: format!("'{name}' reads a capability that no qubit produced"),
            })
    }

    fn filter_kind(&self, ports: &[PortId], kind: PortKind) -> Vec<PortId> {
        ports
            .iter()
            .copied()
            .filter(|port| {
                self.ports
                    .get(port)
                    .is_some_and(|(found, _)| *found == kind)
            })
            .collect()
    }

    fn first_output(&self, region: RegionId, index: usize) -> Result<PortId, ReadError> {
        let entry = &self.entries[&region][index];
        entry
            .outputs
            .first()
            .copied()
            .ok_or_else(|| ReadError::Malformed {
                reason: format!("'{}' produces nothing", entry.name),
            })
    }

    fn trace(&self, port: PortId) -> Result<PortId, ReadError> {
        self.source_of
            .get(&port)
            .copied()
            .ok_or_else(|| ReadError::Malformed {
                reason: "an operand is not connected to anything".to_string(),
            })
    }
}

impl From<OrderError> for ReadError {
    fn from(error: OrderError) -> Self {
        ReadError::Malformed {
            reason: error.to_string(),
        }
    }
}

/// The nodes in the order the program runs them, one order per region.
///
/// The order itself comes from [`region_orders`], which the RIR stage reads
/// too. This flattens each node into an [`Entry`] so the walk below does not
/// repeatedly search the graph.
fn program_order(graph: &Graph) -> Result<BTreeMap<RegionId, Vec<Entry>>, ReadError> {
    let mut pending: BTreeMap<StableNodeId, Entry> = BTreeMap::new();
    for node in graph.nodes() {
        let id = graph
            .stable_id(node.id)
            .ok_or_else(|| ReadError::Malformed {
                reason: format!("'{}' has no durable identity", node.operation.0),
            })?;
        pending.insert(
            id,
            Entry {
                id,
                node: node.id,
                name: node.operation.0.clone(),
                inputs: node.inputs.clone(),
                outputs: node.outputs.clone(),
                origin: node
                    .payload
                    .origin
                    .and_then(|origin| CircuitOrigin::try_from(origin).ok()),
                payload: node.payload.clone(),
            },
        );
    }

    region_orders(graph)?
        .into_iter()
        .map(|(region, order)| {
            let entries = order
                .into_iter()
                .map(|id| pending.remove(&id).expect("an ordered node was flattened"))
                .collect();
            Ok((region, entries))
        })
        .collect()
}
