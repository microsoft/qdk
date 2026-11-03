// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use qsc_circuit_store::{
    BinaryOp, CircuitStore, Clbit, ControlFlow, Expr, ExprId, Literal, PackedOperation, ParamValue,
    ParameterId, Qubit, Snapshot, SnapshotError, StoreError,
};
use qsc_data_structures::target::TargetCapabilityFlags;
use serde::{Deserialize, Serialize};

use crate::builder::OpenScope;

/// Failures in shared authoring policy or one of its native engines.
#[derive(Debug, thiserror::Error)]
pub enum CircuitError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Snapshot(#[from] SnapshotError),
    #[error(transparent)]
    Encoding(#[from] qsc_snapshot::Error),
    #[error(transparent)]
    Qir(#[from] qsc_codegen::circuit::Error),
    #[error(transparent)]
    Graph(#[from] qsc_codegen::circuit_qhdg::Error),
    #[error("{0}")]
    Invalid(String),
    #[error("cannot {0} a circuit while a control-flow block is open")]
    Unfinished(String),
}

/// A checked, named range in one of the circuit's two fixed index spaces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Register {
    pub name: String,
    pub start: u32,
    pub size: u32,
}

/// An unplaced register declaration.
pub enum RegisterSpec {
    Quantum { name: String, size: u32 },
    Classical { name: String, size: u32 },
}

/// A complete circuit document and its transient construction session.
///
/// The store remains authoritative for instructions, operands and expressions.
/// Register metadata and builder state are shared by language adapters here.
#[derive(Debug)]
pub struct Circuit {
    pub(crate) store: CircuitStore,
    registers: Vec<Register>,
    clbit_registers: Vec<Register>,
    pub(crate) open_scopes: Vec<OpenScope>,
    pub(crate) identity: Arc<()>,
    pub(crate) next_builder: u64,
    loop_counter: u32,
}

impl Circuit {
    #[must_use]
    pub fn new(qubits: u32, clbits: u32) -> Self {
        Self::of(CircuitStore::new(qubits, clbits))
    }

    /// Wraps compiler output without inventing lost register metadata.
    #[must_use]
    pub fn of(store: CircuitStore) -> Self {
        Self {
            store,
            registers: Vec::new(),
            clbit_registers: Vec::new(),
            open_scopes: Vec::new(),
            identity: Arc::new(()),
            next_builder: 0,
            loop_counter: 0,
        }
    }

    pub fn from_registers(
        registers: impl IntoIterator<Item = RegisterSpec>,
        clbits: u32,
    ) -> Result<Self, CircuitError> {
        let mut quantum = Vec::new();
        let mut classical = Vec::new();
        let mut qubit_next = 0u32;
        let mut clbit_next = 0u32;
        for register in registers {
            let (records, next, name, size) = match register {
                RegisterSpec::Quantum { name, size } => (&mut quantum, &mut qubit_next, name, size),
                RegisterSpec::Classical { name, size } => {
                    (&mut classical, &mut clbit_next, name, size)
                }
            };
            if records.iter().any(|r: &Register| r.name == name) {
                return Err(CircuitError::Invalid(format!(
                    "duplicate register name '{name}'"
                )));
            }
            let end = next.checked_add(size).ok_or_else(|| {
                CircuitError::Invalid(format!("register '{name}' has an invalid range"))
            })?;
            records.push(Register {
                name,
                start: *next,
                size,
            });
            *next = end;
        }
        let width = clbit_next
            .checked_add(clbits)
            .ok_or_else(|| CircuitError::Invalid("classical bit count exceeds u32 range".into()))?;
        let mut circuit = Self::new(qubit_next, width);
        circuit.registers = quantum;
        circuit.clbit_registers = classical;
        Ok(circuit)
    }

    #[must_use]
    pub fn store(&self) -> &CircuitStore {
        &self.store
    }

    /// Low-level arena access; authoring appends should use `append` so that
    /// instructions are routed into the current body.
    pub fn store_mut(&mut self) -> &mut CircuitStore {
        &mut self.store
    }

    #[must_use]
    pub fn quantum_registers(&self) -> &[Register] {
        &self.registers
    }

    #[must_use]
    pub fn classical_registers(&self) -> &[Register] {
        &self.clbit_registers
    }

    pub fn require_closed(&self, verb: &str) -> Result<(), CircuitError> {
        if self.open_scopes.is_empty() {
            Ok(())
        } else {
            Err(CircuitError::Unfinished(verb.into()))
        }
    }

    pub fn closed_store(&self, verb: &str) -> Result<&CircuitStore, CircuitError> {
        self.require_closed(verb)?;
        Ok(&self.store)
    }

    pub fn try_clone(&self) -> Result<Self, CircuitError> {
        self.require_closed("copy")?;
        let mut copy = Self::of(self.store.clone());
        copy.registers.clone_from(&self.registers);
        copy.clbit_registers.clone_from(&self.clbit_registers);
        copy.loop_counter = self.loop_counter;
        Ok(copy)
    }

    pub fn bind_parameters(&self, values: &[(ParameterId, f64)]) -> Result<Self, CircuitError> {
        let store = self.store.bind_parameters(values)?;
        let mut bound = Self::of(store);
        bound.registers.clone_from(&self.registers);
        bound.clbit_registers.clone_from(&self.clbit_registers);
        bound.loop_counter = self.loop_counter;
        Ok(bound)
    }

    pub fn append(
        &mut self,
        operation: PackedOperation,
        qubits: &[Qubit],
        clbits: &[Clbit],
        params: Option<Box<[ParamValue]>>,
    ) -> Result<(), StoreError> {
        match self.current_block() {
            Some(block) => self
                .store
                .push_into_block(block, operation, qubits, clbits, params),
            None => self.store.push(operation, qubits, clbits, params),
        }
    }

    pub fn append_control_flow(&mut self, control: ControlFlow) -> Result<(), StoreError> {
        match self.current_block() {
            Some(block) => self.store.push_control_flow_into_block(block, control),
            None => self.store.push_control_flow(control),
        }
    }

    pub fn append_jump(&mut self, control: ControlFlow, name: &str) -> Result<(), CircuitError> {
        if !self.open_scopes.iter().any(|scope| scope.is_loop_body) {
            return Err(CircuitError::Invalid(format!(
                "{name} is only meaningful inside a loop body"
            )));
        }
        self.append_control_flow(control)?;
        Ok(())
    }

    /// Builds the existing per-bit equality condition for a named register.
    pub fn register_condition(&mut self, name: &str, value: i64) -> Result<ExprId, CircuitError> {
        let register = self
            .clbit_registers
            .iter()
            .find(|r| r.name == name)
            .cloned()
            .ok_or_else(|| {
                CircuitError::Invalid(format!(
                    "this circuit has no classical register named '{name}'"
                ))
            })?;
        if value < 0 {
            return Err(CircuitError::Invalid(format!(
                "register '{name}' cannot equal the negative value {value}"
            )));
        }
        if register.size < i64::BITS && value >> register.size != 0 {
            return Err(CircuitError::Invalid(format!(
                "register '{name}' holds {} bits and can never equal {value}",
                register.size
            )));
        }
        let mut condition = None;
        for offset in 0..register.size {
            let bit = self.store.push_expr(Expr::Clbit(register.start + offset));
            let wanted = offset < i64::BITS && (value >> offset) & 1 == 1;
            let expected = self.store.push_expr(Expr::Literal(Literal::Bool(wanted)));
            let test = self
                .store
                .push_expr(Expr::Binary(BinaryOp::Eq, bit, expected));
            condition = Some(match condition {
                None => test,
                Some(previous) => self
                    .store
                    .push_expr(Expr::Binary(BinaryOp::And, previous, test)),
            });
        }
        condition
            .ok_or_else(|| CircuitError::Invalid(format!("register '{name}' has no bits to test")))
    }

    pub fn next_loop_name(&mut self) -> String {
        loop {
            let name = format!("_loop_i_{}", self.loop_counter);
            self.loop_counter += 1;
            if !self.store.parameters().iter().any(|p| p.name == name)
                && !self.store.vars().iter().any(|v| v.name == name)
            {
                return name;
            }
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, CircuitError> {
        self.require_closed("save")?;
        Ok(self.document().to_bytes()?)
    }

    pub fn to_text(&self) -> Result<String, CircuitError> {
        self.require_closed("save")?;
        Ok(self.document().to_text()?)
    }

    fn document(&self) -> CircuitDocument {
        CircuitDocument {
            snapshot: self.store.to_snapshot(),
            registers: self.registers.clone(),
            classical_registers: self.clbit_registers.clone(),
        }
    }

    pub fn from_text(text: &str) -> Result<Self, CircuitError> {
        Self::from_bytes(text.as_bytes())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CircuitError> {
        let document = CircuitDocument::from_bytes(bytes)?;
        let store = CircuitStore::from_snapshot(&document.snapshot)?;
        validate_registers(&document.registers, store.qubit_count(), "qubits")?;
        validate_registers(&document.classical_registers, store.clbit_count(), "bits")?;
        let mut circuit = Self::of(store);
        circuit.registers = document.registers;
        circuit.clbit_registers = document.classical_registers;
        Ok(circuit)
    }

    pub fn to_qir(&self, capabilities: TargetCapabilityFlags) -> Result<String, CircuitError> {
        Ok(qsc_codegen::circuit::circuit_to_qir(
            self.closed_store("compile")?,
            capabilities,
        )?)
    }

    pub fn to_graph(&self) -> Result<qsc_qhdg::Graph, CircuitError> {
        Ok(qsc_codegen::circuit_qhdg::circuit_to_graph(
            self.closed_store("lower")?,
        )?)
    }

    pub fn to_qasm(&self) -> Result<String, CircuitError> {
        qsc_codegen::circuit_qasm::circuit_to_qasm(self.closed_store("export")?)
            .map_err(|error| CircuitError::Invalid(error.to_string()))
    }

    pub fn from_qasm(source: &str) -> Result<Self, CircuitError> {
        qsc_codegen::circuit_qasm::circuit_from_qasm(source)
            .map(Self::of)
            .map_err(|error| CircuitError::Invalid(error.to_string()))
    }
}

fn validate_registers(records: &[Register], width: u32, unit: &str) -> Result<(), CircuitError> {
    for (index, record) in records.iter().enumerate() {
        let end = record.start.checked_add(record.size).ok_or_else(|| {
            CircuitError::Invalid(format!("register '{}' has an invalid range", record.name))
        })?;
        if end > width {
            return Err(CircuitError::Invalid(format!(
                "register '{}' covers {unit} up to {end} but the circuit has {width}",
                record.name
            )));
        }
        let previous = &records[..index];
        if previous.iter().any(|existing| existing.name == record.name) {
            return Err(CircuitError::Invalid(format!(
                "duplicate register name '{}'",
                record.name
            )));
        }
        if previous
            .iter()
            .any(|existing| record.start < existing.start + existing.size && existing.start < end)
        {
            return Err(CircuitError::Invalid(format!(
                "register '{}' overlaps another register",
                record.name
            )));
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct CircuitDocument {
    snapshot: Snapshot,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    registers: Vec<Register>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    classical_registers: Vec<Register>,
}

impl CircuitDocument {
    fn write(
        &self,
        mut out: qsc_snapshot::snapshot_capnp::circuit_document::Builder<'_>,
    ) -> Result<(), qsc_snapshot::Error> {
        use qsc_snapshot::length;
        self.snapshot.write_binary(out.reborrow().init_store())?;
        let mut registers = out
            .reborrow()
            .init_quantum_registers(length(self.registers.len())?);
        for (i, v) in self.registers.iter().enumerate() {
            let mut out = registers.reborrow().get(length(i)?);
            out.set_name(&v.name);
            out.set_start(v.start);
            out.set_size(v.size);
        }
        let mut registers = out.init_classical_registers(length(self.classical_registers.len())?);
        for (i, v) in self.classical_registers.iter().enumerate() {
            let mut out = registers.reborrow().get(length(i)?);
            out.set_name(&v.name);
            out.set_start(v.start);
            out.set_size(v.size);
        }
        Ok(())
    }

    fn to_bytes(&self) -> Result<Vec<u8>, qsc_snapshot::Error> {
        use qsc_snapshot::{binary, snapshot_capnp as wire};
        let mut message = binary::Message::new_default();
        let mut root = message.init_root::<wire::document::Builder>();
        root.set_version(qsc_snapshot::MODEL_VERSION);
        self.write(root.init_circuit())?;
        binary::encode(qsc_snapshot::Kind::Circuit, &message)
    }

    fn to_text(&self) -> Result<String, qsc_snapshot::Error> {
        let mut writer = qsc_snapshot::text::Writer::new(qsc_snapshot::Kind::Circuit)?;
        writer.open("circuit")?;
        writer.field("store", |w| self.snapshot.write_text(w))?;
        for (field, registers) in [
            ("quantumRegisters", &self.registers),
            ("classicalRegisters", &self.classical_registers),
        ] {
            writer.field(field, |w| {
                w.open("list")?;
                for v in registers {
                    w.open("register")?;
                    w.field("name", |w| w.string(&v.name))?;
                    w.field("start", |w| {
                        w.atom(v.start);
                        Ok(())
                    })?;
                    w.field("size", |w| {
                        w.atom(v.size);
                        Ok(())
                    })?;
                    w.close()?;
                }
                w.close()
            })?;
        }
        writer.close()?;
        writer.finish()
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, qsc_snapshot::Error> {
        use qsc_snapshot::{Error, Format, Kind, binary, payload, snapshot_capnp as wire, text};
        let (format, body) = payload(Kind::Circuit, bytes)?;
        match format {
            Format::Binary => {
                let (message, mut budget) = binary::decode(Kind::Circuit, body)?;
                let root = message.get_root::<wire::document::Reader>()?;
                let wire::document::Circuit(v) = root.which()? else {
                    return Err(Error::malformed("expected circuit document"));
                };
                let v = v?;
                let snapshot = Snapshot::read_binary(v.get_store()?, &mut budget)?;
                let quantum = v.get_quantum_registers()?;
                let classical = v.get_classical_registers()?;
                budget.charge(quantum.len() as usize)?;
                budget.charge(classical.len() as usize)?;
                let read = |v: wire::register::Reader<'_>| -> Result<Register, Error> {
                    Ok(Register {
                        name: qsc_snapshot::string(v.get_name()?)?,
                        start: v.get_start(),
                        size: v.get_size(),
                    })
                };
                let document = Self {
                    snapshot,
                    registers: quantum.iter().map(read).collect::<Result<_, _>>()?,
                    classical_registers: classical.iter().map(read).collect::<Result<_, _>>()?,
                };
                let mut projection = binary::Message::new_default();
                let mut out = projection.init_root::<wire::document::Builder>();
                out.set_version(qsc_snapshot::MODEL_VERSION);
                document.write(out.init_circuit())?;
                binary::check_projection::<wire::document::Owned>(
                    root,
                    projection.get_root_as_reader::<wire::document::Reader>()?,
                )?;
                Ok(document)
            }
            Format::Text => {
                let parsed = text::parse(Kind::Circuit, bytes)?;
                let mut record = parsed.record("circuit")?;
                let mut budget = parsed.budget();
                let snapshot = Snapshot::read_text(
                    text::Record::new(record.field("store")?, "store")?,
                    &mut budget,
                )?;
                let mut read = |value| -> Result<Vec<Register>, Error> {
                    text::sequence(value, &mut budget)?
                        .into_iter()
                        .map(|v| {
                            let mut v = text::Record::new(v, "register")?;
                            let register = Register {
                                name: text::string(v.field("name")?)?,
                                start: text::u32(v.field("start")?)?,
                                size: text::u32(v.field("size")?)?,
                            };
                            v.finish()?;
                            Ok(register)
                        })
                        .collect()
                };
                let registers = read(record.field("quantumRegisters")?)?;
                let classical_registers = read(record.field("classicalRegisters")?)?;
                record.finish()?;
                Ok(Self {
                    snapshot,
                    registers,
                    classical_registers,
                })
            }
        }
    }
}
