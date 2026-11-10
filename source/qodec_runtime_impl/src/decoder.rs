// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Passive decoders: decoders that never pend and never correct. Most of a
//! [`Decoder`](crate::protocol::Decoder)'s protocol is the same for all of
//! them, so [`Passive`] implements it once, around the one part that differs:
//! how a [`Record`] becomes a result ([`Decode`]).

use super::{
    backend::Backend,
    layer::Layer,
    plan::{GadgetPlan, LayerPlan},
    protocol::{BasicReadout, Call, Fault, Lower, Record, Reply, Status},
    spec::select_status,
};
use std::rc::Rc;

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
    layer: Rc<LayerPlan>,
    inner: D,
    status: Status,
}

impl<D> Passive<D> {
    /// Wraps `inner`, which decodes records of `layer`.
    pub fn new(layer: Rc<LayerPlan>, inner: D) -> Self {
        Self {
            layer,
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
        let instruction = self
            .layer
            .isa
            .instructions
            .get(record.instruction)
            .ok_or_else(|| Fault::protocol("a record of an unknown instruction"))?;
        let mut status = record.status;
        if let Some(inv) = &record.invocation {
            let gadget = self
                .layer
                .gadgets
                .get(inv.gadget.0)
                .ok_or_else(|| Fault::protocol("a record of an unknown gadget"))?;
            status |= body_select_status(gadget, record);
        }
        let decoded = self.inner.decode(record);
        self.status |= status | decoded.status;
        if !self.layer.upper_needs_result[record.instruction] {
            return Ok(None);
        }
        if decoded.outcomes.len() != instruction.observe_count()
            || decoded.flags.len() != instruction.flags.len()
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
fn body_select_status(gadget: &GadgetPlan, record: &Record) -> Status {
    let mut status = Status::default();
    let mut offset = 0;
    for call in &gadget.body.calls {
        if !call.select.is_empty() {
            let flags = offset + call.observe_count..offset + call.observe_count + call.flag_count;
            if let Some(flags) = record.bits.get(flags) {
                status |= select_status(&call.select, flags);
            }
        }
        offset += call.observe_count + call.flag_count;
    }
    status
}
