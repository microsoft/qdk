// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The device the code runs on: the toy (`tests/toy/simulator.rs`),
//! accepting its instructions as calls, those the code calls only. Its set
//! declares what each instruction does, as Qodec would, so that the code's
//! Stim-named instructions (`R`, `M`, `X`) are matched to the device's
//! (`reset`, `measure`, `x`) by action.

use std::sync::Arc;

use executor_protocol::{
    Capabilities, Close, Describe, Executor, InstructionBatch, InstructionBatchRef,
    InstructionCall, InstructionSet, OpcodeTable, Operand, OperandKind, Pauli, Rejected,
};

use crate::repetition3::{measure, pauli, reset};
use crate::toy::Op;

use OperandKind::{Block, Tag};

/// The device's instruction set.
pub fn device_set() -> InstructionSet {
    InstructionSet::new("device")
        .with_action("x", [Block], pauli(Pauli::x(0)))
        .with_action("measure", [Block, Tag], measure())
        .with_action("reset", [Block], reset())
}

/// What the toy runs for a call of one instruction.
type ToOp = fn(&InstructionCall) -> Op;

/// The number of `call`'s operand `i`, a qubit or a tag.
fn number(call: &InstructionCall, i: usize) -> u32 {
    match call.operands[i] {
        Operand::Block(n) | Operand::Tag(n) => n,
        other => unreachable!("the set takes a block or a tag here, not {other:?}"),
    }
}

/// The device: the toy, or any executor of its instructions, taking them
/// as calls by opcode, those of [`device_set`] only. It describes `qubits`
/// qubits, whatever the executor it wraps has: a device that claims more
/// than it has shows a refusal downstream of the code.
pub struct Device<E> {
    inner: E,
    qubits: Option<u32>,
    set: Arc<InstructionSet>,
    table: OpcodeTable<ToOp>,
}

impl<E> Device<E> {
    /// The device on `inner`, described as having `qubits` qubits.
    pub fn new(qubits: u32, inner: E) -> Self {
        let set = device_set();
        let table = set.table(|mnemonic| {
            let to_op: ToOp = match mnemonic {
                "x" => |call| Op::X(number(call, 0)),
                "measure" => |call| {
                    let (qubit, tag) = (number(call, 0), number(call, 1));
                    Op::Measure { qubit, tag }
                },
                "reset" => |call| Op::Reset(number(call, 0)),
                _ => return None,
            };
            Some(to_op)
        });
        Self {
            inner,
            qubits: Some(qubits),
            set: Arc::new(set),
            table: table.expect("the toy runs each instruction of the set"),
        }
    }

    /// This device, not saying how many qubits it has.
    pub fn uncounted(self) -> Self {
        Self {
            qubits: None,
            ..self
        }
    }
}

impl<E: Describe> Describe for Device<E> {
    fn capabilities(&self) -> Capabilities {
        let mut offered =
            (self.inner.capabilities()).with_instruction_set(Some(Arc::clone(&self.set)));
        offered.qubits = self.qubits;
        offered
    }
}

impl<E: Executor<InstructionBatch<Op>>> Executor<InstructionBatch<InstructionCall>> for Device<E> {
    type Reply = E::Reply;

    /// Checks each call against the device's set, then runs it on the
    /// executor it wraps by its opcode, one instruction per call.
    ///
    /// # Errors
    ///
    /// [`Rejected::Instruction`] at the first call outside the set, before
    /// anything runs; the wrapped executor's rejection otherwise.
    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        self.set.check_batch(batch.instructions)?;
        let ops: Vec<Op> = (batch.instructions.iter())
            .map(|call| self.table[call.opcode](call))
            .collect();
        self.inner.call(InstructionBatchRef {
            shots: batch.shots,
            instructions: &ops,
        })
    }
}

impl<E: Close> Close for Device<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}
