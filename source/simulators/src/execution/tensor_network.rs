// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A zero-state circuit ket network, with immutable, shared numerical storage.

use std::{collections::BTreeMap, fmt};

use num_complex::Complex64;
use tensornet::{ContractionError, ContractionQuery, Index, Indices, NetworkError, TensorNetwork};

use super::{QuantumEvolutionRegion, UnitaryOperation};

/// A circuit's tensor shapes, coefficient bank, and amplitude output axes.
///
/// Nodes start with one `|0>` boundary per qubit, then follow operation order
/// (identity contributes no node). `node_buffer_ids()[v]` identifies the buffer
/// for `network().nodes()[v]`. Buffers are immutable and owned here, separately
/// from the shapes. Repeated gates of the same kind and exact angle bits share
/// a buffer, regardless of their wire identities; all zero boundaries share one.
///
/// Rx uses axes `[output, input]` and creates a fresh output wire. Rzz is a
/// diagonal factor on `[current(q1), current(q2)]`, preserving both wires.
/// All axes have dimension two and buffers are column-major, first axis fastest.
/// Final axes are ordered `q0, q1, ...`, so basis index `k = sum(b[q] * 2^q)`.
#[derive(Debug)]
pub struct CircuitTensorNetwork {
    network: TensorNetwork,
    buffers: Vec<Box<[Complex64]>>,
    node_buffer_ids: Vec<usize>,
    output_axes: Indices,
}

impl CircuitTensorNetwork {
    /// Builds one reached unitary region starting from the all-zero state.
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
            validate_operation(operation_index, operation, qubit_count)?;
            if matches!(operation, UnitaryOperation::Rx { .. }) {
                wire_count = wire_count
                    .checked_add(1)
                    .ok_or(TensorNetworkBuildError::TooManyWireIndices)?;
            }
        }
        u32::try_from(wire_count).map_err(|_| TensorNetworkBuildError::TooManyWireIndices)?;
        let mut next_wire =
            u32::try_from(qubit_count).map_err(|_| TensorNetworkBuildError::TooManyWireIndices)?;
        let mut wires = (0..next_wire)
            .map(|id| Index::new(id, 2).expect("qubit dimensions are nonzero"))
            .collect::<Vec<_>>();
        let mut nodes = Vec::new();
        let mut buffers = Vec::new();
        let mut node_buffer_ids = Vec::new();
        let mut buffer_ids = BTreeMap::new();
        let mut add_node = |axes: Vec<Index>, key: BufferKey| {
            let axes = Indices::new(axes).map_err(TensorNetworkBuildError::Network)?;
            let buffer_id = *buffer_ids.entry(key).or_insert_with(|| {
                let id = buffers.len();
                buffers.push(coefficients(key, &axes));
                id
            });
            nodes.push(axes);
            node_buffer_ids.push(buffer_id);
            Ok::<_, TensorNetworkBuildError>(())
        };
        for &wire in &wires {
            add_node(vec![wire], BufferKey::Zero)?;
        }
        for &operation in region.operations() {
            match operation {
                UnitaryOperation::I { .. } => {}
                UnitaryOperation::Rx { angle, target } => {
                    let output = Index::new(next_wire, 2).expect("qubit dimensions are nonzero");
                    next_wire += 1;
                    add_node(vec![output, wires[target]], BufferKey::Rx(angle.to_bits()))?;
                    wires[target] = output;
                }
                UnitaryOperation::Rzz { angle, q1, q2 } => {
                    add_node(vec![wires[q1], wires[q2]], BufferKey::Rzz(angle.to_bits()))?;
                }
                _ => unreachable!("operations were validated before constructing the network"),
            }
        }
        let result = Self {
            network: TensorNetwork::new(nodes).map_err(TensorNetworkBuildError::Network)?,
            buffers,
            node_buffer_ids,
            output_axes: Indices::new(wires).map_err(TensorNetworkBuildError::Network)?,
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

    /// Borrows the owned network; does not allocate or evaluate amplitudes.
    pub fn query(&self) -> Result<ContractionQuery<'_>, ContractionError> {
        ContractionQuery::new(&self.network, self.output_axes.clone())
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum BufferKey {
    Zero,
    Rx(u64),
    Rzz(u64),
}

fn coefficients(key: BufferKey, axes: &Indices) -> Box<[Complex64]> {
    let mut values = vec![
        Complex64::new(0.0, 0.0);
        axes.element_count()
            .expect("one or two qubit axes fit in usize")
    ];
    match key {
        BufferKey::Zero => {
            values[axes.offset_of(&[0]).expect("valid zero-state coordinate")] =
                Complex64::new(1.0, 0.0);
        }
        BufferKey::Rx(bits) | BufferKey::Rzz(bits) => {
            let (sine, cosine) = (f64::from_bits(bits) / 2.0).sin_cos();
            for a in 0..2 {
                for b in 0..2 {
                    values[axes.offset_of(&[a, b]).expect("valid gate coordinate")] = match key {
                        BufferKey::Rx(_) if a == b => Complex64::new(cosine, 0.0),
                        BufferKey::Rx(_) => Complex64::new(0.0, -sine),
                        BufferKey::Rzz(_) => {
                            Complex64::new(cosine, if a == b { -sine } else { sine })
                        }
                        BufferKey::Zero => unreachable!("rotation coefficients"),
                    };
                }
            }
        }
    }
    values.into_boxed_slice()
}

fn validate_operation(
    operation_index: usize,
    operation: UnitaryOperation,
    qubit_count: usize,
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
    match operation {
        UnitaryOperation::I { target } => check_qubit(target),
        UnitaryOperation::Rx { angle, target } => {
            check_qubit(target)?;
            check_angle(angle)
        }
        UnitaryOperation::Rzz { angle, q1, q2 } => {
            check_qubit(q1)?;
            check_qubit(q2)?;
            if q1 == q2 {
                return Err(TensorNetworkBuildError::RepeatedOperand {
                    operation_index,
                    qubit: q1,
                });
            }
            check_angle(angle)
        }
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
