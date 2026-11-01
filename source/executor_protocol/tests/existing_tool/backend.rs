// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! An existing backend: one written before the protocol, to an interface of
//! its own, which names its instructions by string and returns its results
//! in the order of the batch. It stays as it is; the bridge
//! (`existing_tool/bridge.rs`) runs it on the protocol.

use std::collections::BTreeSet;

/// One instruction, as the backend's interface names it.
pub struct NamedInstruction {
    pub name: &'static str,
    pub qubits: Vec<u32>,
}

/// One result, as the backend's interface reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reading {
    Zero,
    One,
    /// The qubit was lost before it was read.
    Lost,
}

impl From<bool> for Reading {
    fn from(bit: bool) -> Self {
        if bit { Reading::One } else { Reading::Zero }
    }
}

/// The backend's interface.
pub trait ExistingInterface {
    /// Runs `batch`, and returns one result per `read`, in the order of the
    /// batch; an `Err` when it cannot run it, which does not say whether any
    /// of it ran.
    fn execute_batch(&mut self, batch: &[NamedInstruction]) -> Result<Vec<Reading>, String>;
}

/// The names the backend gives the toy's instructions, by the toy's
/// mnemonic. It has no `coin`.
pub const NAMES: [(&str, &str); 4] = [
    ("x", "flip"),
    ("cx", "cnot"),
    ("measure", "read"),
    ("reset", "clear"),
];

/// A backend of classical bits, some of which are lost.
pub struct Bits {
    bits: Vec<bool>,
    lost: BTreeSet<u32>,
}

impl Bits {
    /// `qubits` zeros, of which `lost` are lost.
    pub fn new(qubits: u32, lost: impl IntoIterator<Item = u32>) -> Self {
        Bits {
            bits: vec![false; qubits as usize],
            lost: lost.into_iter().collect(),
        }
    }
}

impl ExistingInterface for Bits {
    /// # Errors
    ///
    /// An instruction on a qubit it does not have, or one it does not name;
    /// the instructions before it have run.
    fn execute_batch(&mut self, batch: &[NamedInstruction]) -> Result<Vec<Reading>, String> {
        let mut results = Vec::new();
        let count = self.bits.len();
        for instruction in batch {
            let qubit = |k: usize| {
                let q = instruction.qubits[k] as usize;
                (q < count)
                    .then_some(q)
                    .ok_or_else(|| "out of qubits".to_owned())
            };
            match instruction.name {
                "flip" => self.bits[qubit(0)?] ^= true,
                "cnot" => {
                    let (c, t) = (qubit(0)?, qubit(1)?);
                    self.bits[t] ^= self.bits[c];
                }
                "read" if self.lost.contains(&instruction.qubits[0]) => {
                    results.push(Reading::Lost);
                }
                "read" => results.push(self.bits[qubit(0)?].into()),
                "clear" => self.bits[qubit(0)?] = false,
                other => return Err(format!("unknown instruction `{other}`")),
            }
        }
        Ok(results)
    }
}

/// A backend that gives every batch the same reply, as a faulty or failing
/// one would.
pub struct FixedReply(pub Result<Vec<Reading>, String>);

impl ExistingInterface for FixedReply {
    fn execute_batch(&mut self, _: &[NamedInstruction]) -> Result<Vec<Reading>, String> {
        self.0.clone()
    }
}
