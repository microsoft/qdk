// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Circuit amplitude networks, with immutable, shared numerical storage.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use num_complex::Complex64;
use tensornet::{ContractionError, ContractionQuery, Index, Indices, NetworkError, TensorNetwork};

use crate::{MeasurementResult, QubitID};

use super::{
    FixedOutcomeCircuit, FixedOutcomeOperation, OperatorMatrix, QuantumEvolutionRegion,
    UnitaryOperation, unitary_matrix,
};

/// A circuit's tensor shapes, coefficient bank, and amplitude output axes.
///
/// Nodes follow operation order (identity contributes no node).
/// `node_buffer_ids()[v]` identifies the buffer for `network().nodes()[v]`.
/// Buffers are immutable and owned here, separately from the shapes. Repeated
/// gates of the same kind and exact angle bits share a buffer, regardless of
/// their wire identities; all basis boundaries of the same bit share one, so a
/// start |b⟩ and a cap ⟨b| (the same real coefficients) share a buffer too.
///
/// Rx and Sx use axes `[output, input]` and create a fresh output wire. Rzz and
/// Cz are diagonal factors on `[current(q1), current(q2)]`, and S a diagonal
/// factor on `[current(q)]`, preserving the wires; a wire joined by diagonal
/// factors is a hyperedge. All axes have dimension two and buffers are
/// column-major, first axis fastest.
#[derive(Debug)]
pub struct CircuitTensorNetwork {
    network: TensorNetwork,
    buffers: Vec<Box<[Complex64]>>,
    node_buffer_ids: Vec<usize>,
    output_axes: Indices,
    output_qubits: Vec<QubitID>,
}

impl CircuitTensorNetwork {
    /// Builds one reached unitary region starting from the all-zero state.
    ///
    /// Nodes start with one `|0>` boundary per qubit, then follow the region.
    /// Final axes are ordered `q0, q1, ...`, so basis index
    /// `k = sum(b[q] * 2^q)`.
    ///
    /// Supports I, Rx and Rzz only. This is not a continuing-state consumer:
    /// measurements and control remain outside the builder. An empty region
    /// retains all zero-state boundaries; zero qubits and no operations describe
    /// the scalar one. No dense amplitude output is allocated.
    pub fn from_zero_state(
        qubit_count: usize,
        region: &QuantumEvolutionRegion,
    ) -> Result<Self, TensorNetworkBuildError> {
        let mut wire_count = qubit_count;
        for (operation_index, &operation) in region.operations().iter().enumerate() {
            validate_operation(operation_index, operation, qubit_count, GateSet::ZeroState)?;
            if matches!(operation, UnitaryOperation::Rx { .. }) {
                wire_count = wire_count
                    .checked_add(1)
                    .ok_or(TensorNetworkBuildError::TooManyWireIndices)?;
            }
        }
        u32::try_from(wire_count).map_err(|_| TensorNetworkBuildError::TooManyWireIndices)?;
        let mut builder = NetworkBuilder::default();
        let mut wires = (0..qubit_count)
            .map(|_| builder.fresh_wire())
            .collect::<Result<Vec<_>, _>>()?;
        for &wire in &wires {
            builder.add_node(vec![wire], BufferKey::Basis(false))?;
        }
        for &operation in region.operations() {
            match operation {
                UnitaryOperation::I { .. } => {}
                UnitaryOperation::Rx { angle, target } => {
                    let output = builder.fresh_wire()?;
                    builder
                        .add_node(vec![output, wires[target]], BufferKey::Rx(angle.to_bits()))?;
                    wires[target] = output;
                }
                UnitaryOperation::Rzz { angle, q1, q2 } => {
                    builder
                        .add_node(vec![wires[q1], wires[q2]], BufferKey::Rzz(angle.to_bits()))?;
                }
                _ => unreachable!("operations were validated before constructing the network"),
            }
        }
        builder.finish(wires, (0..qubit_count).collect())
    }

    /// Builds the closed amplitude network of one fixed-outcome path.
    ///
    /// ```text
    ///  q0: |0⟩─[Sx]──●──[S]──⟨b₀|  |b₀⟩─[Sx]─     reuse: a new wire from |b⟩, or |0⟩ if reset
    ///  q1: |0⟩───────●──────⟨b₁|                  never used again: no node
    ///  q2:                                        never used: no node
    ///
    ///  A(r) = ⟨r| C |0…0⟩ = the contraction of the network;  P(r) = |A(r)|²
    /// ```
    ///
    /// A qubit's wire starts at its first operation: from |0⟩, or after a
    /// `Measure` from |b⟩ (or |0⟩ when `reset`), because after the rank-one
    /// projection the qubit is exactly that basis state. A `Measure` caps the
    /// current wire with ⟨b|. A start with nothing after it would contribute
    /// the factor ⟨b|b⟩ = 1 to every amplitude, so idle wires get no node:
    /// untouched qubits and resets never used again. A qubit measured with no
    /// gate keeps its start and cap, whose product ⟨b|0⟩ can be zero.
    ///
    /// Wires with operations after their last `Measure`, or never measured,
    /// remain open output axes, in qubit order; a circuit that measures every
    /// used wire gives a closed network, whose contraction is the scalar A(r).
    ///
    /// Supports I, Rx, Rzz, S, Sx and Cz.
    pub fn from_fixed_outcome_circuit(
        circuit: &FixedOutcomeCircuit,
    ) -> Result<Self, TensorNetworkBuildError> {
        let qubit_count = circuit.qubit_count();
        for (operation_index, operation) in circuit.operations().iter().enumerate() {
            if let FixedOutcomeOperation::Unitary(operation) = *operation {
                validate_operation(
                    operation_index,
                    operation,
                    qubit_count,
                    GateSet::FixedOutcome,
                )?;
            }
        }
        let mut builder = NetworkBuilder::default();
        let mut wires = FixedOutcomeWires::default();
        for operation in circuit.operations() {
            match *operation {
                FixedOutcomeOperation::Unitary(operation) => match operation {
                    UnitaryOperation::I { .. } => {}
                    UnitaryOperation::Rx { angle, target } => {
                        wires.replace(&mut builder, target, BufferKey::Rx(angle.to_bits()))?;
                    }
                    UnitaryOperation::Sx { target } => {
                        wires.replace(&mut builder, target, BufferKey::Sx)?;
                    }
                    UnitaryOperation::S { target } => {
                        let wire = wires.current(&mut builder, target)?;
                        builder.add_node(vec![wire], BufferKey::S)?;
                    }
                    UnitaryOperation::Rzz { angle, q1, q2 } => {
                        let axes = vec![
                            wires.current(&mut builder, q1)?,
                            wires.current(&mut builder, q2)?,
                        ];
                        builder.add_node(axes, BufferKey::Rzz(angle.to_bits()))?;
                    }
                    UnitaryOperation::Cz { control, target } => {
                        let axes = vec![
                            wires.current(&mut builder, control)?,
                            wires.current(&mut builder, target)?,
                        ];
                        builder.add_node(axes, BufferKey::Cz)?;
                    }
                    _ => unreachable!("operations were validated before constructing the network"),
                },
                FixedOutcomeOperation::Measure {
                    qubit,
                    outcome,
                    reset,
                    ..
                } => {
                    let one = match outcome {
                        MeasurementResult::Zero => false,
                        MeasurementResult::One => true,
                        MeasurementResult::Loss => {
                            unreachable!("fixed-outcome circuits reject loss outcomes")
                        }
                    };
                    let wire = wires.current(&mut builder, qubit)?;
                    builder.add_node(vec![wire], BufferKey::Basis(one))?;
                    wires.close(qubit, one && !reset);
                }
            }
        }
        let (output_qubits, output_axes) = wires.open.into_iter().unzip();
        builder.finish(output_axes, output_qubits)
    }

    #[must_use]
    pub fn network(&self) -> &TensorNetwork {
        &self.network
    }

    /// The immutable coefficient bank. Buffer IDs are positions in this slice.
    #[must_use]
    pub fn buffers(&self) -> &[Box<[Complex64]>] {
        &self.buffers
    }

    /// One buffer ID per network node, in exactly the network's node order.
    #[must_use]
    pub fn node_buffer_ids(&self) -> &[usize] {
        &self.node_buffer_ids
    }

    #[must_use]
    pub fn output_axes(&self) -> &Indices {
        &self.output_axes
    }

    /// The qubit of each output axis, in the same order: the qubits whose
    /// wires stay open. Empty exactly when the network is closed (a scalar).
    #[must_use]
    pub fn output_qubits(&self) -> &[QubitID] {
        &self.output_qubits
    }

    /// Borrows the owned network; does not allocate or evaluate amplitudes.
    pub fn query(&self) -> Result<ContractionQuery<'_>, ContractionError> {
        ContractionQuery::new(&self.network, self.output_axes.clone())
    }
}

/// Nodes, wire ids and the shared coefficient bank of a network under
/// construction.
#[derive(Default)]
struct NetworkBuilder {
    nodes: Vec<Indices>,
    buffers: Vec<Box<[Complex64]>>,
    node_buffer_ids: Vec<usize>,
    buffer_ids: BTreeMap<BufferKey, usize>,
    next_wire: u32,
}

impl NetworkBuilder {
    fn fresh_wire(&mut self) -> Result<Index, TensorNetworkBuildError> {
        // Wire ids stay below `u32::MAX`, so the wire count fits in `u32`.
        if self.next_wire == u32::MAX {
            return Err(TensorNetworkBuildError::TooManyWireIndices);
        }
        let wire = Index::new(self.next_wire, 2).expect("qubit dimensions are nonzero");
        self.next_wire += 1;
        Ok(wire)
    }

    fn add_node(
        &mut self,
        axes: Vec<Index>,
        key: BufferKey,
    ) -> Result<(), TensorNetworkBuildError> {
        let axes = Indices::new(axes).map_err(TensorNetworkBuildError::Network)?;
        let buffer_id = *self.buffer_ids.entry(key).or_insert_with(|| {
            let id = self.buffers.len();
            self.buffers.push(coefficients(key, &axes));
            id
        });
        self.nodes.push(axes);
        self.node_buffer_ids.push(buffer_id);
        Ok(())
    }

    fn finish(
        self,
        output_axes: Vec<Index>,
        output_qubits: Vec<QubitID>,
    ) -> Result<CircuitTensorNetwork, TensorNetworkBuildError> {
        debug_assert_eq!(output_axes.len(), output_qubits.len());
        let result = CircuitTensorNetwork {
            network: TensorNetwork::new(self.nodes).map_err(TensorNetworkBuildError::Network)?,
            buffers: self.buffers,
            node_buffer_ids: self.node_buffer_ids,
            output_axes: Indices::new(output_axes).map_err(TensorNetworkBuildError::Network)?,
            output_qubits,
        };
        let query = result
            .query()
            .map_err(TensorNetworkBuildError::Contraction)?;
        let marginalized = query.marginalized();
        if !marginalized.as_slice().is_empty() {
            return Err(TensorNetworkBuildError::UnconnectedWires { marginalized });
        }
        Ok(result)
    }
}

/// The wires of a fixed-outcome path, created on first use.
///
/// Maps rather than per-qubit vectors, so memory follows the qubits the
/// circuit uses, not its declared qubit count.
#[derive(Default)]
struct FixedOutcomeWires {
    /// The current wire of each qubit with a live wire, in qubit order.
    open: BTreeMap<QubitID, Index>,
    /// Qubits whose next wire starts from |1⟩: measured as one, not reset.
    /// Every other qubit without a live wire is exactly |0⟩.
    starts_in_one: BTreeSet<QubitID>,
}

impl FixedOutcomeWires {
    /// The qubit's live wire, starting one from its known basis state if it
    /// has none.
    fn current(
        &mut self,
        builder: &mut NetworkBuilder,
        qubit: QubitID,
    ) -> Result<Index, TensorNetworkBuildError> {
        if let Some(&wire) = self.open.get(&qubit) {
            return Ok(wire);
        }
        let wire = builder.fresh_wire()?;
        let one = self.starts_in_one.remove(&qubit);
        builder.add_node(vec![wire], BufferKey::Basis(one))?;
        self.open.insert(qubit, wire);
        Ok(wire)
    }

    /// Applies a one-qubit `[output, input]` gate, moving the qubit to a fresh wire.
    fn replace(
        &mut self,
        builder: &mut NetworkBuilder,
        qubit: QubitID,
        key: BufferKey,
    ) -> Result<(), TensorNetworkBuildError> {
        let input = self.current(builder, qubit)?;
        let output = builder.fresh_wire()?;
        builder.add_node(vec![output, input], key)?;
        self.open.insert(qubit, output);
        Ok(())
    }

    /// Ends the qubit's live wire after its cap; the next wire starts from
    /// |1⟩ when `starts_in_one`, otherwise from |0⟩.
    fn close(&mut self, qubit: QubitID, starts_in_one: bool) {
        self.open.remove(&qubit);
        if starts_in_one {
            self.starts_in_one.insert(qubit);
        } else {
            self.starts_in_one.remove(&qubit);
        }
    }
}

/// What a buffer holds, so equal keys can share storage.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum BufferKey {
    /// The basis vector of the bit (`true` = |1⟩), as a start |b⟩ or a cap ⟨b|:
    /// both have the same real coefficients.
    Basis(bool),
    Rx(u64),
    Rzz(u64),
    S,
    Sx,
    Cz,
}

fn coefficients(key: BufferKey, axes: &Indices) -> Box<[Complex64]> {
    let mut values = vec![
        Complex64::new(0.0, 0.0);
        axes.element_count()
            .expect("one or two qubit axes fit in usize")
    ];
    let mut set = |coords: &[usize], value| {
        values[axes.offset_of(coords).expect("valid gate coordinate")] = value;
    };
    match key {
        BufferKey::Basis(one) => set(&[usize::from(one)], Complex64::new(1.0, 0.0)),
        // Diagonal gates bind only their diagonal, over the operands' current
        // wires: axes `[first, second]` for basis index `2 · first + second`.
        BufferKey::S | BufferKey::Rzz(_) | BufferKey::Cz => {
            let matrix = gate_matrix(key);
            assert!(matrix.is_diagonal(), "{key:?} must be diagonal");
            for index in 0..matrix.dimension() {
                let coords = (0..matrix.qubit_count())
                    .rev()
                    .map(|bit| (index >> bit) & 1)
                    .collect::<Vec<_>>();
                set(&coords, matrix.entry(index, index));
            }
        }
        // Other gates bind `[output, input]`.
        BufferKey::Rx(_) | BufferKey::Sx => {
            let matrix = gate_matrix(key);
            for output in 0..2 {
                for input in 0..2 {
                    set(&[output, input], matrix.entry(output, input));
                }
            }
        }
    }
    values.into_boxed_slice()
}

/// The shared-table matrix of a gate buffer, in the key's operand order.
fn gate_matrix(key: BufferKey) -> OperatorMatrix {
    let operation = match key {
        BufferKey::Rx(bits) => UnitaryOperation::Rx {
            angle: f64::from_bits(bits),
            target: 0,
        },
        BufferKey::Rzz(bits) => UnitaryOperation::Rzz {
            angle: f64::from_bits(bits),
            q1: 0,
            q2: 1,
        },
        BufferKey::S => UnitaryOperation::S { target: 0 },
        BufferKey::Sx => UnitaryOperation::Sx { target: 0 },
        BufferKey::Cz => UnitaryOperation::Cz {
            control: 0,
            target: 1,
        },
        BufferKey::Basis(_) => unreachable!("basis vectors are not gates"),
    };
    unitary_matrix(operation).expect("the shared table defines every exact-network gate")
}

/// The gates a builder supports; anything else is `UnsupportedOperation`.
#[derive(Clone, Copy)]
enum GateSet {
    /// I, Rx and Rzz.
    ZeroState,
    /// I, Rx, Rzz, S, Sx and Cz.
    FixedOutcome,
}

fn validate_operation(
    operation_index: usize,
    operation: UnitaryOperation,
    qubit_count: usize,
    gate_set: GateSet,
) -> Result<(), TensorNetworkBuildError> {
    let check_qubit = |qubit| {
        if qubit < qubit_count {
            Ok(())
        } else {
            Err(TensorNetworkBuildError::QubitOutOfRange {
                operation_index,
                qubit,
                qubit_count,
            })
        }
    };
    let check_angle = |angle: f64| {
        if angle.is_finite() {
            Ok(())
        } else {
            Err(TensorNetworkBuildError::NonfiniteAngle { operation_index })
        }
    };
    let check_pair = |q1: QubitID, q2: QubitID| {
        check_qubit(q1)?;
        check_qubit(q2)?;
        if q1 == q2 {
            Err(TensorNetworkBuildError::RepeatedOperand {
                operation_index,
                qubit: q1,
            })
        } else {
            Ok(())
        }
    };
    let fixed_outcome = matches!(gate_set, GateSet::FixedOutcome);
    match operation {
        UnitaryOperation::I { target } => check_qubit(target),
        UnitaryOperation::Rx { angle, target } => {
            check_qubit(target)?;
            check_angle(angle)
        }
        UnitaryOperation::Rzz { angle, q1, q2 } => {
            check_pair(q1, q2)?;
            check_angle(angle)
        }
        UnitaryOperation::S { target } | UnitaryOperation::Sx { target } if fixed_outcome => {
            check_qubit(target)
        }
        UnitaryOperation::Cz { control, target } if fixed_outcome => check_pair(control, target),
        operation => Err(TensorNetworkBuildError::UnsupportedOperation {
            operation_index,
            operation,
        }),
    }
}

#[derive(Debug, PartialEq)]
pub enum TensorNetworkBuildError {
    UnsupportedOperation {
        operation_index: usize,
        operation: UnitaryOperation,
    },
    QubitOutOfRange {
        operation_index: usize,
        qubit: usize,
        qubit_count: usize,
    },
    RepeatedOperand {
        operation_index: usize,
        qubit: usize,
    },
    NonfiniteAngle {
        operation_index: usize,
    },
    TooManyWireIndices,
    Network(NetworkError),
    Contraction(ContractionError),
    UnconnectedWires {
        marginalized: Indices,
    },
}

impl fmt::Display for TensorNetworkBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedOperation {
                operation_index,
                operation,
            } => write!(
                f,
                "tensor-network operation {operation_index} is unsupported: {operation:?}"
            ),
            Self::QubitOutOfRange {
                operation_index,
                qubit,
                qubit_count,
            } => write!(
                f,
                "tensor-network operation {operation_index} uses qubit {qubit}, outside 0..{qubit_count}"
            ),
            Self::RepeatedOperand {
                operation_index,
                qubit,
            } => write!(
                f,
                "tensor-network operation {operation_index} repeats qubit {qubit}"
            ),
            Self::NonfiniteAngle { operation_index } => write!(
                f,
                "tensor-network operation {operation_index} has a nonfinite angle"
            ),
            Self::TooManyWireIndices => {
                write!(f, "tensor-network wire count exceeds the u32 range")
            }
            Self::Network(error) => error.fmt(f),
            Self::Contraction(error) => error.fmt(f),
            Self::UnconnectedWires { marginalized } => write!(
                f,
                "circuit tensor network has unconnected wires: {marginalized:?}"
            ),
        }
    }
}

impl std::error::Error for TensorNetworkBuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Network(error) => Some(error),
            Self::Contraction(error) => Some(error),
            _ => None,
        }
    }
}
