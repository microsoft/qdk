// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Saving and restoring a circuit.
//!
//! The snapshot describes the circuit, not the data structure that holds it.
//! Operands are written as resolved index lists rather than as handles into the
//! interning tables, so the stored form does not depend on the order patterns
//! happened to be interned in, and a change to the interning strategy does not
//! invalidate stored circuits. Restoring re-interns, which also means a restored
//! circuit carries no operand patterns left behind by instructions that were
//! built and then removed.
//!
//! # Fidelity
//!
//! Save and restore is exact for structurally valid circuits. The restored
//! circuit compares equal to the original on width, instruction sequence,
//! resolved operands, parameters,
//! classical variables, expressions, and block bodies. Encoding is stable, so
//! writing a restored circuit produces the same bytes as writing the original.
//! Low-level construction can assemble cyclic block containment; restoration
//! rejects that invalid structure instead of publishing a recursive body graph.
//!
//! # Trust
//!
//! Decoding applies the same structural validation as building: operand
//! indices, block references, expression references, and parameter references
//! are all checked. Envelope decoding bounds nesting and Cap'n Proto traversal
//! before restoring the circuit. As before, callers reading untrusted circuits
//! must limit how many input bytes they accept.

use serde::{Deserialize, Serialize};

use crate::angle::{AngleArena, AngleExpr};
use crate::classical::{Expr, ParamValue, Parameter, Var};
use crate::control_flow::BlockId;
use crate::operands::{Clbit, Qubit};
use crate::operation::PackedOperation;
use crate::store::{CircuitStore, StoreError};

/// Version of the snapshot schema.
///
/// Read rejects any other version rather than guessing at the difference.
/// The current spike has a single typed wire contract.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = qsc_snapshot::SCHEMA_VERSION;

/// Why a snapshot could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SnapshotError {
    #[error(transparent)]
    Encoding(#[from] qsc_snapshot::Error),
    #[error("snapshot is malformed: {0}")]
    Malformed(String),
    #[error("snapshot schema version {found} is unsupported, expected {expected}")]
    SchemaVersion { found: u32, expected: u32 },
    #[error("snapshot describes an invalid circuit: {0}")]
    Invalid(#[from] StoreError),
}

/// One instruction, with operands resolved to plain indices.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SnapshotInstruction {
    pub operation: PackedOperation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub qubits: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clbits: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<ParamValue>>,
}

/// The stored form of a circuit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub qubit_count: u32,
    pub clbit_count: u32,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub global_phase: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parameters: Vec<Parameter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vars: Vec<Var>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exprs: Vec<Expr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub angles: Vec<AngleExpr>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<Vec<SnapshotInstruction>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<SnapshotInstruction>,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(value: &f64) -> bool {
    value.to_bits() == 0.0_f64.to_bits()
}

impl CircuitStore {
    /// Builds the stored form of this circuit.
    #[must_use]
    pub fn to_snapshot(&self) -> Snapshot {
        Snapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            qubit_count: self.qubit_count(),
            clbit_count: self.clbit_count(),
            global_phase: self.global_phase(),
            parameters: self.parameters().to_vec(),
            vars: self.vars().to_vec(),
            exprs: self.exprs().nodes().to_vec(),
            angles: self.angles().nodes().to_vec(),
            blocks: (0..self.block_count())
                .map(|index| {
                    let id = BlockId(u32::try_from(index).expect("block count fits in u32"));
                    let block = self.block(id).expect("block index is in range");
                    block
                        .instructions
                        .iter()
                        .map(|instruction| self.snapshot_instruction(instruction))
                        .collect()
                })
                .collect(),
            instructions: self
                .instructions()
                .map(|instruction| self.snapshot_instruction(instruction))
                .collect(),
        }
    }

    fn snapshot_instruction(&self, instruction: &crate::store::Instruction) -> SnapshotInstruction {
        SnapshotInstruction {
            operation: instruction.operation.clone(),
            qubits: self
                .qubits_of(instruction)
                .unwrap_or(&[])
                .iter()
                .map(|qubit| qubit.0)
                .collect(),
            clbits: self
                .clbits_of(instruction)
                .unwrap_or(&[])
                .iter()
                .map(|clbit| clbit.0)
                .collect(),
            params: instruction.params.as_deref().map(<[ParamValue]>::to_vec),
        }
    }

    /// Writes this circuit as a QDK-native Cap'n Proto envelope.
    ///
    /// # Errors
    ///
    /// Returns an error only if serialization itself fails, which the schema
    /// does not otherwise allow.
    pub fn to_bytes(&self) -> Result<Vec<u8>, SnapshotError> {
        let mut message = qsc_snapshot::binary::Message::new_default();
        let mut root = message.init_root::<qsc_snapshot::snapshot_capnp::document::Builder>();
        root.set_version(qsc_snapshot::MODEL_VERSION);
        self.to_snapshot().write_binary(root.init_store())?;
        Ok(qsc_snapshot::binary::encode(
            qsc_snapshot::Kind::CircuitStore,
            &message,
        )?)
    }

    /// Writes the same model as an S-expression envelope for inspection.
    pub fn to_text(&self) -> Result<String, SnapshotError> {
        let mut writer = qsc_snapshot::text::Writer::new(qsc_snapshot::Kind::CircuitStore)?;
        self.to_snapshot().write_text(&mut writer)?;
        Ok(writer.finish()?)
    }

    /// Reads a circuit written by [`CircuitStore::to_bytes`] or [`CircuitStore::to_text`].
    ///
    /// # Errors
    ///
    /// Returns an error when the text is not a snapshot, when its schema
    /// version differs, or when it describes a circuit that violates the
    /// store's own rules.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SnapshotError> {
        use qsc_snapshot::{Format, Kind, binary, payload, snapshot_capnp as wire, text};
        let (format, body) = payload(Kind::CircuitStore, bytes)?;
        let snapshot = match format {
            Format::Binary => {
                let (message, mut budget) = binary::decode(Kind::CircuitStore, body)?;
                let root = message
                    .get_root::<wire::document::Reader>()
                    .map_err(qsc_snapshot::Error::from)?;
                let wire::document::Store(store) =
                    root.which().map_err(qsc_snapshot::Error::from)?
                else {
                    return Err(qsc_snapshot::Error::malformed("expected store document").into());
                };
                let store = store.map_err(qsc_snapshot::Error::from)?;
                let snapshot = Snapshot::read_binary(store, &mut budget)?;
                let mut projection = binary::Message::new_default();
                let mut projected_root = projection.init_root::<wire::document::Builder>();
                projected_root.set_version(qsc_snapshot::MODEL_VERSION);
                snapshot.write_binary(projected_root.init_store())?;
                binary::check_projection::<wire::document::Owned>(
                    root,
                    projection
                        .get_root_as_reader::<wire::document::Reader>()
                        .map_err(qsc_snapshot::Error::from)?,
                )?;
                snapshot
            }
            Format::Text => {
                let parsed = text::parse(Kind::CircuitStore, bytes)?;
                Snapshot::read_text(parsed.record("store")?, &mut parsed.budget())?
            }
        };
        Self::from_snapshot(&snapshot)
    }

    pub fn from_text(text: &str) -> Result<Self, SnapshotError> {
        Self::from_bytes(text.as_bytes())
    }
    /// Rebuilds a circuit from its stored form.
    ///
    /// # Errors
    ///
    /// Returns an error when the schema version differs, or when the snapshot
    /// describes a circuit that violates the store's own rules.
    pub fn from_snapshot(snapshot: &Snapshot) -> Result<Self, SnapshotError> {
        if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(SnapshotError::SchemaVersion {
                found: snapshot.schema_version,
                expected: SNAPSHOT_SCHEMA_VERSION,
            });
        }

        snapshot.validate_expressions()?;
        snapshot.validate_blocks()?;
        let mut store = Self::new(snapshot.qubit_count, snapshot.clbit_count);
        store.set_global_phase(snapshot.global_phase);
        for parameter in &snapshot.parameters {
            store.declare_parameter(parameter.name.clone());
        }
        for var in &snapshot.vars {
            store.declare_var(var.name.clone(), var.ty);
        }
        for expr in &snapshot.exprs {
            store.push_expr(*expr);
        }
        let angles = AngleArena::from_nodes(snapshot.angles.clone()).map_err(|index| {
            SnapshotError::Malformed(format!(
                "angle expression {index} references a node stored after it"
            ))
        })?;
        store.install_angles(angles)?;
        // Blocks are created before any instruction is placed, because a
        // control-flow instruction refers to blocks by identifier and a body
        // can refer to a block declared after it.
        for _ in 0..snapshot.blocks.len() {
            store.new_block();
        }
        for (index, instructions) in snapshot.blocks.iter().enumerate() {
            let id = BlockId(u32::try_from(index).expect("block count fits in u32"));
            for instruction in instructions {
                let (operation, qubits, clbits, params) = parts(instruction);
                store.push_into_block(id, operation, &qubits, &clbits, params)?;
            }
        }
        for instruction in &snapshot.instructions {
            let (operation, qubits, clbits, params) = parts(instruction);
            store.push(operation, &qubits, &clbits, params)?;
        }
        for instruction in snapshot
            .blocks
            .iter()
            .flatten()
            .chain(&snapshot.instructions)
        {
            if let PackedOperation::ControlFlow(control) = &instruction.operation {
                store.validate_control_flow(control)?;
            }
        }
        Ok(store)
    }
}

impl Snapshot {
    fn validate_blocks(&self) -> Result<(), SnapshotError> {
        let mut state = vec![0_u8; self.blocks.len()];
        let mut pending = Vec::new();
        for root in 0..self.blocks.len() {
            pending.push((root, false));
            while let Some((id, exit)) = pending.pop() {
                if exit {
                    state[id] = 2;
                    continue;
                }
                match state[id] {
                    2 => continue,
                    1 => {
                        return Err(SnapshotError::Malformed(format!("block {id} is cyclic")));
                    }
                    _ => {}
                }
                state[id] = 1;
                pending.push((id, true));
                // These are containment edges; executing a loop repeatedly
                // does not make its body contain itself.
                for instruction in &self.blocks[id] {
                    if let PackedOperation::ControlFlow(control) = &instruction.operation {
                        for child in control.blocks() {
                            let index = child.0 as usize;
                            if index >= self.blocks.len() {
                                return Err(StoreError::UnknownBlock(child.0).into());
                            }
                            pending.push((index, false));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_expressions(&self) -> Result<(), SnapshotError> {
        let operands = |expr: Expr| -> [Option<usize>; 2] {
            match expr {
                Expr::Not(value) => [Some(value.0 as usize), None],
                Expr::Binary(_, left, right) => [Some(left.0 as usize), Some(right.0 as usize)],
                _ => [None, None],
            }
        };
        for (index, expr) in self.exprs.iter().enumerate() {
            let valid = match expr {
                Expr::Var(id) => (id.0 as usize) < self.vars.len(),
                Expr::Clbit(id) => *id < self.clbit_count,
                _ => true,
            };
            if !valid
                || operands(*expr)
                    .into_iter()
                    .flatten()
                    .any(|id| id >= self.exprs.len())
            {
                return Err(SnapshotError::Malformed(format!(
                    "expression {index} has an invalid reference"
                )));
            }
        }
        let mut state = vec![0_u8; self.exprs.len()];
        let mut pending = Vec::new();
        for root in 0..self.exprs.len() {
            pending.push((root, false));
            while let Some((id, exit)) = pending.pop() {
                if exit {
                    state[id] = 2;
                    continue;
                }
                match state[id] {
                    2 => continue,
                    1 => {
                        return Err(SnapshotError::Malformed(format!(
                            "expression {id} is cyclic"
                        )));
                    }
                    _ => {}
                }
                state[id] = 1;
                pending.push((id, true));
                for child in operands(self.exprs[id]).into_iter().flatten() {
                    pending.push((child, false));
                }
            }
        }
        Ok(())
    }

    pub fn write_binary(
        &self,
        out: qsc_snapshot::snapshot_capnp::circuit_store::Builder<'_>,
    ) -> Result<(), qsc_snapshot::Error> {
        crate::snapshot_binary::write(self, out)
    }

    pub fn read_binary(
        input: qsc_snapshot::snapshot_capnp::circuit_store::Reader<'_>,
        budget: &mut qsc_snapshot::Budget,
    ) -> Result<Self, qsc_snapshot::Error> {
        crate::snapshot_binary::read(input, budget)
    }

    pub fn write_text(
        &self,
        writer: &mut qsc_snapshot::text::Writer,
    ) -> Result<(), qsc_snapshot::Error> {
        crate::snapshot_text::write(self, writer)
    }

    pub fn read_text(
        input: qsc_snapshot::text::Record<'_>,
        budget: &mut qsc_snapshot::Budget,
    ) -> Result<Self, qsc_snapshot::Error> {
        crate::snapshot_text::read(input, budget)
    }
}

type InstructionParts = (
    PackedOperation,
    Vec<Qubit>,
    Vec<Clbit>,
    Option<Box<[ParamValue]>>,
);

fn parts(instruction: &SnapshotInstruction) -> InstructionParts {
    (
        instruction.operation.clone(),
        instruction.qubits.iter().copied().map(Qubit).collect(),
        instruction.clbits.iter().copied().map(Clbit).collect(),
        instruction
            .params
            .as_ref()
            .map(|params| params.clone().into_boxed_slice()),
    )
}
