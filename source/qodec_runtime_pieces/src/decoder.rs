// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Passive decoders: decoders that never pend and never correct. Most of a
//! [`Decoder`](crate::protocol::Decoder)'s protocol is the same for all of
//! them, so [`Passive`] implements it once, around the one part that differs:
//! how a [`Record`] becomes a result ([`Decode`]).

use super::{
    backend::Backend,
    isa::{CompiledSelect, Isa, select_status},
    layer::Layer,
    protocol::{BasicReadout, Call, Fault, Lower, Record, Reply, Status},
};
use std::{ops::Range, rc::Rc};

/// What a decoder needs to know about the records of a layer: the upper
/// ISA, and the `select` hints of each gadget's body.
#[derive(Clone, Debug, Default)]
pub struct RecordLayout {
    /// The upper ISA.
    pub upper: Rc<Isa>,
    /// Per gadget, indexed by [`GadgetId`](crate::GadgetId): the body calls
    /// that have a `select` hint.
    pub gadgets: Vec<Vec<BodySelect>>,
}

/// The `select` hint of one call of a gadget body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodySelect {
    /// Where the call's flags are in the gadget's record.
    pub flags: Range<usize>,
    /// The hint, over the call's flags.
    pub select: CompiledSelect,
}

/// The inference of a passive decoder: turns one call's readout record into
/// the call's result.
pub trait Decode {
    /// The result of `record`'s call: its observe outcomes, its flags, and the
    /// recoverable conditions raised while decoding. Called for every record,
    /// in call order, including those of calls without a result, which only
    /// update the decoder's state. Its outcomes and flags are then ignored.
    fn decode(&mut self, record: &Record) -> BasicReadout;
}

/// Turns a [`Decode`] into a `Decoder<Never, Never>`: it sends its whole
/// batch below in one call, decodes each record, and answers with the results
/// of the calls that have observe outcomes or flags.
///
/// ```text
///   │ Vec<Call>          upper ISA        ▲ Result<Vec<Reply>, Fault>  in order
///  ┌▼─────────────────────────────────────┴───────────────────────────────────┐
///  │ Passive<D>                                                               │
///  │   status   conditions raised so far; they stick for the rest of the shot │
///  └┬─────────────────────────────────────▲───────────────────────────────────┘
///   │ Vec<Lower<Never>>  the same calls   │ Result<Vec<Record>, Fault>
/// ```
///
/// It also checks the per-call `select` hints of each gadget body against the
/// recorded flags, for every decoder: a mismatch is the recoverable status
/// `rejected`.
pub struct Passive<D> {
    layout: Rc<RecordLayout>,
    inner: D,
    status: Status,
}

impl<D> Passive<D> {
    /// Wraps `inner`, which decodes records laid out as `layout` says.
    pub fn new(layout: Rc<RecordLayout>, inner: D) -> Self {
        Self {
            layout,
            inner,
            status: Status::default(),
        }
    }

    /// The wrapped decoder.
    pub fn inner(&self) -> &D {
        &self.inner
    }
}

impl<D: Decode> Layer for Passive<D> {
    type Input = Vec<Call>;
    type Output = Result<Vec<Reply>, Fault>;
    type LowerInput = Vec<Lower>;
    type LowerOutput = Result<Vec<Record>, Fault>;

    fn execute(
        &mut self,
        calls: Vec<Call>,
        below: &mut dyn Backend<Vec<Lower>, Result<Vec<Record>, Fault>>,
    ) -> Result<Vec<Reply>, Fault> {
        let n = calls.len();
        let records = below.execute(calls.into_iter().map(Lower::Call).collect())?;
        if records.len() != n {
            return Err(Fault::protocol(format!(
                "{} records for {n} calls",
                records.len()
            )));
        }
        let mut replies = Vec::new();
        for record in &records {
            if let Some(reply) = self.decode(record)? {
                replies.push(reply);
            }
        }
        Ok(replies)
    }
}

impl<D: Decode> Passive<D> {
    /// Decodes one record, and returns its call's result if the call has one.
    fn decode(&mut self, record: &Record) -> Result<Option<Reply>, Fault> {
        let layout = Rc::clone(&self.layout);
        let instruction = layout
            .upper
            .instructions
            .get(record.instruction)
            .ok_or_else(|| Fault::protocol("a record of an unknown instruction"))?;
        let mut status = record.status;
        if let Some(inv) = &record.invocation {
            let selects = layout
                .gadgets
                .get(inv.gadget.0)
                .ok_or_else(|| Fault::protocol("a record of an unknown gadget"))?;
            status |= body_select_status(selects, record);
        }
        let decoded = self.inner.decode(record);
        self.status |= status | decoded.status;
        if !instruction.has_result() {
            return Ok(None);
        }
        if decoded.outcomes.len() != instruction.outcomes
            || decoded.flags.len() != instruction.flags
        {
            return Err(Fault::protocol(format!(
                "the decoder gave `{}` {} outcomes and {} flags",
                instruction.mnemonic,
                decoded.outcomes.len(),
                decoded.flags.len()
            )));
        }
        Ok(Some(Reply::Ready(BasicReadout {
            outcomes: decoded.outcomes,
            flags: decoded.flags,
            status: self.status,
        })))
    }
}

/// The status of a gadget body's per-call `select` hints under the recorded
/// flags. The spec makes [`select`](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#per-call-select)
/// a hint, so a mismatch is recoverable.
fn body_select_status(selects: &[BodySelect], record: &Record) -> Status {
    let mut status = Status::default();
    for call in selects {
        if let Some(flags) = record.bits.get(call.flags.clone()) {
            status |= select_status(&call.select, flags);
        }
    }
    status
}
