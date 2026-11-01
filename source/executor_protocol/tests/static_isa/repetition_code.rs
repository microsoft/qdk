// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The repetition code as an adapter, [`RepetitionCode`]: it describes what
//! it accepts and sends, so that `compose` checks it against an executor
//! before any shot is made, and, composed with an executor, it puts its
//! [`Repetition3`] upstream of that executor.

use std::ops::Shr;
use std::sync::Arc;

use executor_protocol::{
    Adapter, AdapterBoundary, Apply, Close, Compose, Executor, InstructionBatch, InstructionSet,
    WithExecutor, compose,
};

use crate::logical::{Logical, logical_set};
use crate::repetition3::{Repetition3, gadget_instructions};
use crate::toy::{Op, ToySimulator};

/// The repetition code as an adapter.
#[derive(Clone, Copy, Debug, Default)]
pub struct RepetitionCode {
    /// The blocks the downstream executor holds, once built against it.
    blocks: Option<u32>,
}

impl Adapter for RepetitionCode {
    type Accepts = Logical;
    type Sends = Op;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::new(gadget_instructions()))
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::new(logical_set()))
    }

    /// Built against the executor downstream, once: it offers a logical
    /// qubit per block of three qubits there.
    fn build(self, boundary: &mut AdapterBoundary<'_>) -> Self {
        let blocks = boundary.downstream().qubits.map(|q| q / 3);
        boundary.offered_upstream().qubits = blocks;
        Self { blocks }
    }
}

impl<E: Executor<InstructionBatch<Op>> + Close> Apply<E> for RepetitionCode {
    type Output = Repetition3<E>;

    /// The code upstream of the physical executor: it draws nothing, so
    /// each shot's draws are the physical shot's.
    fn apply(self, downstream: E) -> Repetition3<E> {
        Repetition3 {
            inner: downstream,
            blocks: self.blocks.unwrap_or(u32::MAX),
        }
    }
}

/// `code >> executor`: the crate that defines an adapter may give it the
/// `>>` of `compose`, here with the toy simulator.
impl Shr<ToySimulator> for RepetitionCode {
    type Output = <Self as Compose<ToySimulator, WithExecutor>>::Output;

    fn shr(self, executor: ToySimulator) -> Self::Output {
        compose(self, executor)
    }
}
