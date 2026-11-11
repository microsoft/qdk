// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`TypeChecker`]: a layer that rejects ill-typed batches before they reach a
//! runtime.

use super::{
    backend::Backend,
    isa::Isa,
    layer::Layer,
    protocol::{BasicReadout, Call, Fault},
    runtime::QodecRuntimeError,
};
use rustc_hash::FxHashMap;
use std::{marker::PhantomData, rc::Rc};

/// Rejects a batch of top-ISA calls, with nothing run, if one of its calls is
/// ill-typed: an input operand that is not a live block of the type its
/// instruction declares. Stack it on a [`Runtime`](crate::Runtime), which
/// would otherwise lose the shot over such a call.
///
/// ```text
///   │ Vec<Call>   top ISA        ▲ Result<Vec<BasicReadout>, QodecRuntimeError<E>>
///  ┌▼────────────────────────────┴┐
///  │ TypeChecker<E>               │  live blocks: label → block type
///  │                              │  ill-typed batch: Rejected, nothing below
///  └┬────────────────────────────▲┘
///   │ Vec<Call>   well typed     │ the same
///   ▼ a runtime
/// ```
///
/// It tracks blocks from the `in` and `out` operands the top ISA declares, and
/// knows nothing of gadgets: a call it accepts may still fail below, for
/// example when no gadget implements its instruction.
pub struct TypeChecker<E> {
    isa: Rc<Isa>,
    /// Live blocks: label → block type.
    live: FxHashMap<u64, usize>,
    error: PhantomData<fn() -> E>,
}

/// What each change to the live blocks replaced, to undo a batch.
type Undo = Vec<(u64, Option<usize>)>;

impl<E> TypeChecker<E> {
    /// A checker for `isa`, with no live block.
    #[must_use]
    pub fn new(isa: &Rc<Isa>) -> Self {
        Self {
            isa: Rc::clone(isa),
            live: FxHashMap::default(),
            error: PhantomData,
        }
    }

    /// The type of the live block `label`, if any.
    #[must_use]
    pub fn live_type(&self, label: u64) -> Option<&str> {
        self.live
            .get(&label)
            .map(|&t| self.isa.types.names[t].as_str())
    }

    /// Checks `calls` in order, each against the blocks the earlier ones left
    /// live. If all pass, keeps their effect on the live blocks; otherwise
    /// keeps nothing and explains the first failure.
    pub fn check(&mut self, calls: &[Call]) -> Result<(), String> {
        self.apply_all(calls).map(drop)
    }

    /// Like [`Self::check`], returning how to undo the batch.
    fn apply_all(&mut self, calls: &[Call]) -> Result<Undo, String> {
        let mut undo = Undo::new();
        for call in calls {
            if let Err(e) = self.apply(call, &mut undo) {
                self.undo(undo);
                return Err(e);
            }
        }
        Ok(undo)
    }

    /// Checks one call, and applies its effect: its inputs end, and its
    /// outputs become live with their declared types.
    fn apply(&mut self, call: &Call, undo: &mut Undo) -> Result<(), String> {
        let isa = Rc::clone(&self.isa);
        let parsed = isa.parse_call(call.instruction_id, &call.parameters)?;
        let instruction = &isa.instructions[parsed.index];
        let known: FxHashMap<u64, usize> = parsed
            .operands
            .iter()
            .filter_map(|l| self.live.get(l).map(|&t| (*l, t)))
            .collect();
        let (ins, outs) = instruction.bind(&isa.types, &parsed.operands, &known)?;
        if let Some(input) = ins.iter().find(|b| !self.live.contains_key(&b.label)) {
            return Err(format!("block {} is not live", input.label));
        }
        for input in &ins {
            undo.push((input.label, self.live.remove(&input.label)));
        }
        for output in outs {
            undo.push((
                output.label,
                self.live.insert(output.label, output.block_type),
            ));
        }
        Ok(())
    }

    fn undo(&mut self, undo: Undo) {
        for (label, previous) in undo.into_iter().rev() {
            match previous {
                Some(block_type) => self.live.insert(label, block_type),
                None => self.live.remove(&label),
            };
        }
    }
}

impl<E> Layer for TypeChecker<E> {
    type Input = Vec<Call>;
    type Output = Result<Vec<BasicReadout>, QodecRuntimeError<E>>;
    type LowerInput = Vec<Call>;
    type LowerOutput = Result<Vec<BasicReadout>, QodecRuntimeError<E>>;

    fn execute(
        &mut self,
        calls: Vec<Call>,
        below: &mut dyn Backend<Vec<Call>, Self::LowerOutput>,
    ) -> Self::Output {
        let undo = self
            .apply_all(&calls)
            .map_err(|e| QodecRuntimeError::Rejected(Fault::invalid(e).at_layer(0)))?;
        let result = below.execute(calls);
        // The batch did not run below either: forget it here too.
        if let Err(QodecRuntimeError::Rejected(_)) = result {
            self.undo(undo);
        }
        result
    }
}
