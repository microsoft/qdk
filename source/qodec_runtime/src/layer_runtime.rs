// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The layer runtime: [`LayerRuntime`] implements [`Backend`] for its upper ISA
//! by running gadget bodies, and drives the backend below with its lower ISA.

use super::{
    actions::PauliExpr,
    backend::{
        Backend, BackendInstruction, BatchResult, Bit, InstructionId, Param, ParamView, QodecError,
        Readout, Status,
    },
    decoder::{
        BlockRef, Correction, Decoded, DecoderPlan, DecoderRuntime, Invocation, Operation, Step,
    },
    pauli::Pauli,
    plan::{ArgumentPlan, GadgetPlan, InstructionPlan, LayerPlan},
    resolver::ResolveCache,
    spec::{
        BoundOperand, bind_operands, code_qubits, encode_arg, select_status, validate_arguments,
        value_of,
    },
};
use qodec::{Action, Argument};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap, VecDeque},
    rc::Rc,
};

/// A layer result whose error wraps the lower backend's error.
pub type LayerResult<T, E> = Result<T, QodecError<E>>;

/// A live upper block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveBlock {
    /// The block's current reference.
    pub block_ref: BlockRef,
    /// Lower blocks holding it.
    pub support: Vec<u64>,
    /// Its code qubits, in code order: (lower block, logical index in it).
    pub qubits: Vec<(u64, usize)>,
}

/// A lower result bit that may not be known until its batch is flushed.
#[derive(Clone, Copy, Debug)]
struct LazyBit {
    slot: u64,
    flag: bool,
    index: usize,
}

/// One result-producing buffered lower instruction, with the R shape it must have.
#[derive(Clone, Copy, Debug)]
struct BufferSlot {
    slot: u64,
    outcomes: usize,
    flags: usize,
}

/// Deferred decoder work, in program order.
enum DecoderEvent {
    Decode {
        inv: Invocation,
        /// The body's circuit readout record.
        raw: Vec<LazyBit>,
        /// (slot, index of the body call whose select applies).
        selects: Vec<(u64, usize)>,
        /// Every result-producing slot of the body. This decode is their last
        /// reader and frees them.
        slots: Vec<u64>,
        /// The index in `sinks` that receives the decoded result.
        sink: usize,
    },
    Discarded(Vec<BlockRef>),
}

/// A decoded `BackendInstruction`.
struct Call {
    index: usize,
    operands: Vec<u64>,
    args: BTreeMap<String, Argument>,
}

/// An invocation placed on live lower blocks, ready to run.
struct PreparedInvocation {
    gp: Rc<GadgetPlan>,
    call: Call,
    out_ops: Vec<BoundOperand<u64>>,
    placement: FxHashMap<String, u64>,
}

/// What running a gadget body leaves for its decode: the same `raw`, `selects`
/// and `slots` as [`DecoderEvent::Decode`].
type BodyRecord = (Vec<LazyBit>, Vec<(u64, usize)>, Vec<u64>);

// Lower ids are only created by `allocate_lower` as `type_of.len()`, a
// `usize`, so this cast never truncates.
#[allow(clippy::cast_possible_truncation)]
fn ix(lower_id: u64) -> usize {
    lower_id as usize
}

/// The runtime of one qodec layer for one shot. Implements `Backend` for the
/// upper ISA and consumes the `Backend` below for the lower ISA. It is ready to
/// run once constructed; dropping it ends the shot.
pub struct LayerRuntime<Id, P, R, B> {
    plan: Rc<LayerPlan>,
    deferrable: bool,
    lower: B,

    // Shot state
    blocks: FxHashMap<u64, LiveBlock>,
    /// Released lower ids, reused lowest first.
    free: BinaryHeap<Reverse<u64>>,
    /// Lower type of every allocated id, indexed by lower id. Ids are dense, so
    /// it grows by one per fresh id.
    type_of: Vec<Option<usize>>,
    generation: FxHashMap<u64, u64>,
    next_invocation: u64,
    decoder: Box<dyn DecoderRuntime<R>>,
    /// Recoverable conditions raised so far. They stick for the rest of the
    /// shot and are merged into every later result.
    status: Status,
    /// Poisoned: every further `execute_batch` fails with `Poisoned`.
    failed: bool,
    /// Every emit, every write to blocks, free, `type_of`, generation, and every
    /// decoder call or queued decoder event increments it. It decides whether an
    /// `Err` poisons the layer.
    side_effects: u64,

    // Lower-traffic buffer
    buffer: Vec<BackendInstruction<Id, P>>,
    buffer_slots: Vec<BufferSlot>,
    /// Resolved lower results that still have a reader.
    results: FxHashMap<u64, R>,
    next_slot: u64,
    decoder_log: VecDeque<DecoderEvent>,
    /// Decoded upper results of the current batch.
    sinks: Vec<Option<Decoded>>,
    cache: ResolveCache,
}

impl<Id, P, R, B> LayerRuntime<Id, P, R, B>
where
    Id: InstructionId,
    P: Param,
    R: Readout,
    B: Backend<Id, P, R>,
{
    /// Instantiates the runtime of one layer for one shot, over `lower`. Lower
    /// block ids are allocated on demand, so it needs no resources.
    pub fn new(plan: Rc<LayerPlan>, decoder_plan: &dyn DecoderPlan<R>, lower: B) -> Self {
        Self {
            plan,
            deferrable: decoder_plan.deferrable(),
            lower,
            blocks: FxHashMap::default(),
            free: BinaryHeap::new(),
            type_of: Vec::new(),
            generation: FxHashMap::default(),
            next_invocation: 0,
            // Generations start over in every shot, so BlockRefs repeat across
            // shots: a decoder runtime must never outlive its shot.
            decoder: decoder_plan.instantiate(),
            status: Status::default(),
            failed: false,
            side_effects: 0,
            buffer: Vec::new(),
            buffer_slots: Vec::new(),
            results: FxHashMap::default(),
            next_slot: 0,
            decoder_log: VecDeque::new(),
            sinks: Vec::new(),
            cache: ResolveCache::default(),
        }
    }

    /// The layer's plan.
    #[must_use]
    pub fn plan(&self) -> &Rc<LayerPlan> {
        &self.plan
    }

    /// The backend below.
    #[must_use]
    pub fn lower(&self) -> &B {
        &self.lower
    }

    /// The backend below, mutably.
    pub fn lower_mut(&mut self) -> &mut B {
        &mut self.lower
    }

    /// Ends the shot and returns the backend below.
    pub fn into_lower(self) -> B {
        self.lower
    }

    /// The sticky status of the shot.
    #[must_use]
    pub fn status(&self) -> Status {
        self.status
    }

    /// Whether the shot is lost: every further `execute_batch` fails.
    #[must_use]
    pub fn is_poisoned(&self) -> bool {
        self.failed
    }

    /// The live upper block with this label, if any.
    #[must_use]
    pub fn live_block(&self, label: u64) -> Option<&LiveBlock> {
        self.blocks.get(&label)
    }

    // ── execute_batch, buffering and flushing ───────────────────────────────

    fn execute_inner(
        &mut self,
        instructions: &[BackendInstruction<Id, P>],
    ) -> LayerResult<BatchResult<R>, B::Error> {
        self.sinks.clear();
        let mut wanted: SmallVec<[usize; 1]> = SmallVec::new();
        for ins in instructions {
            let call = self.decode_instruction(ins)?;
            let needs_result = self.plan.upper_needs_result[call.index];
            let sink = self.execute_one(call)?;
            if needs_result {
                wanted.push(sink.ok_or(QodecError::Protocol)?);
            }
        }
        self.flush()?;
        let mut results = SmallVec::with_capacity(wanted.len());
        for sink in wanted {
            let d = self.sinks[sink].take().ok_or(QodecError::Protocol)?;
            results.push(R::new(&d.outcomes, &d.flags, self.status | d.status));
        }
        Ok(BatchResult { results })
    }

    /// Buffers one lower instruction. Returns a slot iff it requires a result.
    fn emit(&mut self, index: usize, parameters: SmallVec<[P; 2]>) -> Option<u64> {
        self.side_effects += 1;
        self.buffer.push(BackendInstruction {
            instruction_id: Id::from_index(index),
            parameters,
        });
        let (outcomes, flags) = self.plan.lower_shapes[index];
        if outcomes + flags == 0 {
            return None;
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        self.buffer_slots.push(BufferSlot {
            slot,
            outcomes,
            flags,
        });
        Some(slot)
    }

    /// Sends the buffer to the backend below and resolves its slots.
    fn flush_lower(&mut self) -> LayerResult<(), B::Error> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        let batch = self
            .lower
            .execute_batch(&self.buffer)
            .map_err(QodecError::Lower)?;
        if batch.results.len() != self.buffer_slots.len() {
            return Err(QodecError::Protocol);
        }
        // Validate everything before writing anything, so a Protocol error never
        // leaves partial state (it still poisons, because emit() already counted).
        let shapes_match = self
            .buffer_slots
            .iter()
            .zip(&batch.results)
            .all(|(s, r)| r.outcomes().len() == s.outcomes && r.flags().len() == s.flags);
        if !shapes_match {
            return Err(QodecError::Protocol);
        }
        for (s, r) in self.buffer_slots.drain(..).zip(batch.results) {
            // Recoverable lower conditions bubble up.
            self.status |= r.status();
            self.results.insert(s.slot, r);
        }
        self.buffer.clear();
        Ok(())
    }

    fn flush_if_pending(&mut self, slot: u64) -> LayerResult<(), B::Error> {
        if !self.results.contains_key(&slot) {
            self.flush()?;
        }
        Ok(())
    }

    /// Full flush: resolve lower results, then run the deferred decoder work in
    /// program order. Deferred decodes never emit, so one pass is enough.
    fn flush(&mut self) -> LayerResult<(), B::Error> {
        self.flush_lower()?;
        while let Some(event) = self.decoder_log.pop_front() {
            match event {
                DecoderEvent::Decode {
                    inv,
                    raw,
                    selects,
                    slots,
                    sink,
                } => {
                    let decoded = self.decode_now(&inv, &raw, &selects, &slots)?;
                    self.sinks[sink] = Some(decoded);
                }
                DecoderEvent::Discarded(refs) => self.decoder.discarded(&refs),
            }
        }
        Ok(())
    }

    fn force(&mut self, bit: LazyBit) -> LayerResult<Bit, B::Error> {
        self.flush_if_pending(bit.slot)?;
        let r = self.results.get(&bit.slot).ok_or(QodecError::Protocol)?;
        Ok(if bit.flag {
            r.flags()[bit.index]
        } else {
            r.outcomes()[bit.index]
        })
    }

    // ── Decoding an incoming instruction (pure) ─────────────────────────────

    fn decode_instruction(&self, ins: &BackendInstruction<Id, P>) -> LayerResult<Call, B::Error> {
        let index = ins.instruction_id.index();
        let instruction = self
            .plan
            .isa
            .instructions
            .get(index)
            .ok_or(QodecError::InvalidInstruction)?;
        let operands: Vec<u64> = ins
            .parameters
            .iter()
            .map_while(|p| match p.view() {
                ParamView::Block(b) => Some(b),
                _ => None,
            })
            .collect();
        let rest = &ins.parameters[operands.len()..];
        if rest.len() != instruction.parameters.len() {
            return Err(QodecError::InvalidInstruction);
        }
        let mut args = BTreeMap::new();
        for (parameter, p) in instruction.parameters.iter().zip(rest) {
            let value = value_of(p.view()).ok_or(QodecError::InvalidInstruction)?;
            args.insert(parameter.name.clone(), value);
        }
        validate_arguments(instruction, &args).map_err(|_| QodecError::InvalidInstruction)?;
        Ok(Call {
            index,
            operands,
            args,
        })
    }

    // ── Executing one instruction ───────────────────────────────────────────

    /// Returns the sink that will hold the decoded result, if any.
    fn execute_one(&mut self, call: Call) -> LayerResult<Option<usize>, B::Error> {
        let instruction_plan = self.plan.instructions[call.index].clone();
        match instruction_plan {
            InstructionPlan::Gadget(gp) => {
                let prepared = self.prepare_invocation(gp, call)?;
                let id = self.next_invocation;
                self.next_invocation += 1;
                self.run_invocation(prepared, id).map(Some)
            }
            InstructionPlan::LogicalPauli => {
                self.apply_logical_pauli(&call)?;
                Ok(None)
            }
            InstructionPlan::Release => {
                let [label] = call.operands.as_slice() else {
                    return Err(QodecError::InvalidInstruction);
                };
                self.release(*label)?;
                Ok(None)
            }
            InstructionPlan::Unsupported => Err(QodecError::NoImplementation),
        }
    }

    fn input_types(&self, operands: &[u64]) -> FxHashMap<u64, &str> {
        operands
            .iter()
            .filter_map(|l| {
                self.blocks
                    .get(l)
                    .map(|b| (*l, b.block_ref.block_type.as_str()))
            })
            .collect()
    }

    /// Places one invocation on live lower blocks. Pure: it only reads state,
    /// so a failure here never poisons.
    fn prepare_invocation(
        &self,
        gp: Rc<GadgetPlan>,
        call: Call,
    ) -> LayerResult<PreparedInvocation, B::Error> {
        // Only inputs are constrained by live types; outputs take the
        // instruction's declared types, so type-changing instructions are allowed.
        let input_types = self.input_types(&call.operands);
        let (in_ops, out_ops) = bind_operands(
            &self.plan.upper,
            gp.instruction(),
            &call.operands,
            &input_types,
        )
        .map_err(|_| QodecError::InvalidInstruction)?;
        if in_ops.len() != gp.inputs.len() || out_ops.len() != gp.outputs.len() {
            return Err(QodecError::InvalidInstruction);
        }

        let mut placement: FxHashMap<String, u64> = FxHashMap::default();
        for (label, layout) in call.operands.iter().zip(&gp.inputs) {
            let b = self
                .blocks
                .get(label)
                .ok_or(QodecError::InvalidInstruction)?;
            if layout.labels.len() != b.support.len() {
                return Err(QodecError::InvalidInstruction);
            }
            for (lab, &lb) in layout.labels.iter().zip(&b.support) {
                // One label -> one lower block, and no silent retyping.
                if placement.get(lab).is_some_and(|&p| p != lb)
                    || self.type_of[ix(lb)] != Some(gp.label_type[lab])
                {
                    return Err(QodecError::InvalidInstruction);
                }
                placement.insert(lab.clone(), lb);
            }
        }
        // Placement is injective because live blocks never share lower blocks.
        debug_assert!(
            {
                let mut seen = FxHashSet::default();
                placement.values().all(|lb| seen.insert(*lb))
            },
            "two labels of one invocation should never share a lower block"
        );
        Ok(PreparedInvocation {
            gp,
            call,
            out_ops,
            placement,
        })
    }

    fn run_invocation(
        &mut self,
        prepared: PreparedInvocation,
        id: u64,
    ) -> LayerResult<usize, B::Error> {
        let PreparedInvocation {
            gp,
            call,
            out_ops,
            mut placement,
        } = prepared;
        let n_in = gp.inputs.len();

        // Overwrite output-only operands.
        for &label in &call.operands[n_in..] {
            self.release(label)?;
        }

        let input_refs: Vec<BlockRef> = call.operands[..n_in]
            .iter()
            .map(|l| self.blocks[l].block_ref.clone())
            .collect();
        let output_refs: Vec<BlockRef> = out_ops
            .iter()
            .map(|o| self.fresh_ref(o.label, &o.block_type))
            .collect();
        let inv = Invocation {
            id,
            gadget: Rc::clone(&gp),
            operands: call.operands,
            args: call.args,
            input_refs,
            output_refs,
        };

        // Correction contract: `before` may target input refs and other live
        // blocks. It runs while inputs are still published.
        if !self.deferrable {
            self.side_effects += 1;
            let first = self.decoder.before(&inv);
            self.drive(first, |layer, r| Ok(layer.decoder.resume_before(r)))?;
        }

        for lab in &gp.labels {
            if !placement.contains_key(lab) {
                let lb = self.allocate_lower(gp.label_type[lab]);
                placement.insert(lab.clone(), lb);
            }
        }

        let (raw, selects, slots) = self.run_body(&gp, &inv, &placement)?;

        // Publish outputs: an output block is the gadget's `out` encoding.
        for label in &inv.operands[..n_in] {
            self.blocks.remove(label);
        }
        self.side_effects += 1;
        let mut retained: FxHashSet<u64> = FxHashSet::default();
        for (block_ref, layout) in inv.output_refs.iter().zip(&gp.outputs) {
            let support: Vec<u64> = layout.labels.iter().map(|l| placement[l]).collect();
            // Two outputs must not share a lower block, or both live blocks
            // would own it and release it twice.
            if support.iter().any(|lb| retained.contains(lb)) {
                return Err(QodecError::InvalidInstruction);
            }
            retained.extend(&support);
            let qubits = code_qubits(layout, &placement, &self.plan.lower);
            self.blocks.insert(
                block_ref.label,
                LiveBlock {
                    block_ref: block_ref.clone(),
                    support,
                    qubits,
                },
            );
        }

        // Release scratch and consumed lower blocks. Nothing here depends on
        // decoding, so it is emitted immediately. Blocks the body consumed
        // itself (e.g. measured away) are only returned to the free list.
        let mut leftovers: Vec<(u64, &String)> = gp
            .labels
            .iter()
            .map(|lab| (placement[lab], lab))
            .filter(|(lb, _)| !retained.contains(lb))
            .collect();
        leftovers.sort_unstable();
        for (lb, lab) in leftovers {
            if gp.body.consumed.contains(lab) {
                self.forget_lower(lb);
            } else {
                self.release_lower(lb);
            }
        }

        let sink = self.sinks.len();
        self.sinks.push(None);
        if self.deferrable {
            let input_refs = inv.input_refs.clone();
            self.decoder_log.push_back(DecoderEvent::Decode {
                inv,
                raw,
                selects,
                slots,
                sink,
            });
            // Queue the discard after the decode, which still reads these refs.
            self.decoder_log
                .push_back(DecoderEvent::Discarded(input_refs));
            self.side_effects += 2;
        } else {
            // Decoding now forces `raw`, which flushes the buffer.
            let decoded = self.decode_now(&inv, &raw, &selects, &slots)?;
            self.sinks[sink] = Some(decoded);
            self.side_effects += 1;
            self.decoder.discarded(&inv.input_refs);
        }
        Ok(sink)
    }

    /// Runs one gadget body. It never sees the backend: it emits through
    /// the buffer and works with lazy bits.
    fn run_body(
        &mut self,
        gp: &GadgetPlan,
        inv: &Invocation,
        placement: &FxHashMap<String, u64>,
    ) -> LayerResult<BodyRecord, B::Error> {
        let plan = Rc::clone(&self.plan);
        let mut record: Vec<LazyBit> = Vec::with_capacity(gp.body.record_len);
        let mut selects = Vec::new();
        let mut slots = Vec::new();
        for (call_index, pc) in gp.body.calls.iter().enumerate() {
            let instruction = &plan.lower_isa.instructions[pc.instruction];
            let mut args = BTreeMap::new();
            for (name, arg) in &pc.args {
                let value = match arg {
                    ArgumentPlan::Value(v) => v.clone(),
                    ArgumentPlan::Forwarded(param) => inv
                        .args
                        .get(param)
                        .cloned()
                        .ok_or(QodecError::InvalidInstruction)?,
                    ArgumentPlan::Readout(j) => {
                        // Forcing an earlier bit may flush the buffer. A `None`
                        // bit is recoverable: it is passed as 0 and marks the
                        // shot `unresolved`.
                        let bit = self.force(record[*j])?;
                        if bit.is_none() {
                            self.status |= Status::UNRESOLVED;
                        }
                        Argument::Boolean(bit.unwrap_or(false))
                    }
                };
                args.insert(name.clone(), value);
            }
            validate_arguments(instruction, &args).map_err(|_| QodecError::InvalidInstruction)?;
            let mut params: SmallVec<[P; 2]> = pc
                .operands
                .iter()
                .map(|o| P::from_view(ParamView::Block(placement[o])))
                .collect();
            for parameter in &instruction.parameters {
                params.push(
                    encode_arg(&args[&parameter.name]).ok_or(QodecError::InvalidInstruction)?,
                );
            }
            let slot = self.emit(pc.instruction, params);
            if pc.observe_count + pc.flag_count > 0 {
                let slot = slot.ok_or(QodecError::Protocol)?;
                slots.push(slot);
                record.extend((0..pc.observe_count).map(|index| LazyBit {
                    slot,
                    flag: false,
                    index,
                }));
                record.extend((0..pc.flag_count).map(|index| LazyBit {
                    slot,
                    flag: true,
                    index,
                }));
                // The per-call select is a spec hint. It is checked lazily and
                // becomes a recoverable status.
                if !pc.select.is_empty() {
                    selects.push((slot, call_index));
                }
            }
        }
        Ok((record, selects, slots))
    }

    /// Decodes one invocation. `raw` and `selects` are forced here.
    fn decode_now(
        &mut self,
        inv: &Invocation,
        raw: &[LazyBit],
        selects: &[(u64, usize)],
        slots: &[u64],
    ) -> LayerResult<Decoded, B::Error> {
        let bits: Vec<Bit> = raw
            .iter()
            .map(|b| self.force(*b))
            .collect::<Result<_, _>>()?;
        let mut st = Status::default();
        for &(slot, call_index) in selects {
            self.flush_if_pending(slot)?;
            let r = self.results.get(&slot).ok_or(QodecError::Protocol)?;
            st |= select_status(&inv.gadget.body.calls[call_index].select, r.flags());
        }
        // This decode is the last reader of these results, so free them.
        for slot in slots {
            self.results.remove(slot);
        }
        self.side_effects += 1;
        let first = self.decoder.decode(inv, &bits);
        let d = self.drive(first, |layer, r| Ok(layer.decoder.resume_decode(r)))?;
        let instruction = inv.gadget.instruction();
        if d.outcomes.len() != instruction.observe_count()
            || d.flags.len() != instruction.flags.len()
        {
            return Err(QodecError::Protocol);
        }
        let status = st | d.status;
        self.status |= status;
        Ok(Decoded {
            outcomes: d.outcomes,
            flags: d.flags,
            status,
        })
    }

    fn fresh_ref(&mut self, label: u64, block_type: &str) -> BlockRef {
        self.side_effects += 1;
        let generation = self.generation.entry(label).or_insert(0);
        *generation += 1;
        BlockRef {
            label,
            generation: *generation,
            block_type: block_type.to_string(),
        }
    }

    /// Upper Pauli instruction without a gadget: apply the code's logical
    /// operator as Pauli corrections on the block's code qubits.
    fn apply_logical_pauli(&mut self, call: &Call) -> LayerResult<(), B::Error> {
        let plan = Rc::clone(&self.plan);
        let instruction = &plan.isa.instructions[call.index];
        // Signs are dropped: the spec defines Pauli steps only up to global
        // phase (https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#actions).
        let mut product = Pauli::default();
        for step in &instruction.action {
            if let Action::Pauli(text) = &step.action {
                let p = PauliExpr::parse(instruction, text)
                    .and_then(|e| e.resolve(&call.args))
                    .map_err(|_| QodecError::InvalidInstruction)?;
                product = product.mul_unsigned(&p);
            }
        }
        let input_types = self.input_types(&call.operands);
        let (ins, _) = bind_operands(&plan.upper, instruction, &call.operands, &input_types)
            .map_err(|_| QodecError::InvalidInstruction)?;
        for bound in ins {
            let block_ref = self
                .blocks
                .get(&bound.label)
                .ok_or(QodecError::InvalidInstruction)?
                .block_ref
                .clone();
            let code = plan
                .codes
                .get(&bound.block_type)
                .ok_or(QodecError::NoImplementation)?;
            let mut logical = Pauli::default();
            for i in 0..bound.width {
                if let Some(op) = product.get(bound.offset + i) {
                    if op.has_x() {
                        let x = code.x.get(i).ok_or(QodecError::NoImplementation)?;
                        logical = logical.mul_unsigned(x);
                    }
                    if op.has_z() {
                        let z = code.z.get(i).ok_or(QodecError::NoImplementation)?;
                        logical = logical.mul_unsigned(z);
                    }
                }
            }
            for &(q, op) in logical.terms() {
                self.apply_correction(&Correction {
                    blocks: vec![block_ref.clone()],
                    op: Operation {
                        name: op.lower().to_string(),
                        targets: vec![q],
                        angle: None,
                    },
                })?;
            }
        }
        Ok(())
    }

    /// Upper release instruction without a gadget, and output-only overwrite.
    fn release(&mut self, label: u64) -> LayerResult<(), B::Error> {
        let Some(b) = self.blocks.remove(&label) else {
            return Ok(());
        };
        self.side_effects += 1;
        for &lb in &b.support {
            self.release_lower(lb);
        }
        if self.deferrable {
            self.decoder_log
                .push_back(DecoderEvent::Discarded(vec![b.block_ref]));
        } else {
            self.decoder.discarded(&[b.block_ref]);
        }
        Ok(())
    }

    /// Releases a lower block that is still live below. Eager when the lower ISA
    /// has a release instruction for its type; otherwise lazy: nothing is
    /// emitted, and the block stays live below until this layer reuses the id.
    fn release_lower(&mut self, lb: u64) {
        let t = self.type_of[ix(lb)].unwrap_or_default();
        if let Some(index) = self.plan.lower_release[t] {
            // A release has no observe outcomes or flags, so it produces no
            // result.
            self.emit(
                index,
                SmallVec::from_iter([P::from_view(ParamView::Block(lb))]),
            );
        }
        self.forget_lower(lb);
    }

    /// Returns a lower block id to the free list without emitting anything.
    fn forget_lower(&mut self, lb: u64) {
        self.side_effects += 1;
        self.type_of[ix(lb)] = None;
        self.free.push(Reverse(lb));
    }

    /// Allocates a lower block id on demand: the lowest released id
    /// first, otherwise a fresh one. Ids therefore stay dense, so the next fresh
    /// id is `type_of.len()`. There is no limit here: a backend below that
    /// cannot hold the id fails it.
    fn allocate_lower(&mut self, lower_type: usize) -> u64 {
        self.side_effects += 1;
        if let Some(Reverse(lb)) = self.free.pop() {
            self.type_of[ix(lb)] = Some(lower_type);
            lb
        } else {
            self.type_of.push(Some(lower_type));
            (self.type_of.len() - 1) as u64
        }
    }

    // ── Corrections (decoder → lower layer) ─────────────────────────────────

    /// Runs a correction coroutine to completion.
    fn drive<T>(
        &mut self,
        first: Step<T>,
        mut resume: impl FnMut(&mut Self, Option<&R>) -> LayerResult<Step<T>, B::Error>,
    ) -> LayerResult<T, B::Error> {
        let mut step = first;
        loop {
            match step {
                Step::Done(value) => return Ok(value),
                Step::Correct(correction) => {
                    let result = self.apply_correction(&correction)?;
                    self.side_effects += 1;
                    step = resume(self, result.as_ref())?;
                }
            }
        }
    }

    /// Lowers one correction. It lowers to at most ONE result-producing lower
    /// instruction, whose result is returned.
    fn apply_correction(&mut self, c: &Correction) -> LayerResult<Option<R>, B::Error> {
        let mut seen = FxHashSet::default();
        if !c.blocks.iter().all(|r| seen.insert(r.label)) {
            return Err(QodecError::InvalidInstruction);
        }
        let mut qubits = Vec::new();
        for block_ref in &c.blocks {
            let b = self
                .blocks
                .get(&block_ref.label)
                .filter(|b| b.block_ref == *block_ref) // live, same generation
                .ok_or(QodecError::InvalidInstruction)?;
            qubits.extend_from_slice(&b.qubits);
        }
        let slots: Vec<(u64, usize)> =
            c.op.targets
                .iter()
                .map(|&i| qubits.get(i).copied().ok_or(QodecError::InvalidInstruction))
                .collect::<Result<_, _>>()?;

        let plan = Rc::clone(&self.plan);
        let type_of = &self.type_of;
        let parts = plan
            .resolver
            .resolve(
                &c.op.name,
                &slots,
                |lb| type_of[ix(lb)].unwrap_or_default(),
                c.op.angle,
                &plan.lower,
                &mut self.cache,
            )
            .ok_or(QodecError::NoImplementation)?;
        let producing = parts
            .iter()
            .filter(|p| {
                let (o, f) = self.plan.lower_shapes[p.resolved.index];
                o + f > 0
            })
            .count();
        if producing > 1 {
            return Err(QodecError::NoImplementation);
        }

        let mut result = None;
        for part in parts {
            let instruction = &plan.lower_isa.instructions[part.resolved.index];
            let mut params: SmallVec<[P; 2]> = part
                .operands
                .iter()
                .map(|&o| P::from_view(ParamView::Block(o)))
                .collect();
            for parameter in &instruction.parameters {
                let arg = part
                    .resolved
                    .args
                    .get(&parameter.name)
                    .ok_or(QodecError::InvalidInstruction)?;
                params.push(encode_arg(arg).ok_or(QodecError::InvalidInstruction)?);
            }
            if let Some(slot) = self.emit(part.resolved.index, params) {
                // The decoder needs this result now, and nothing else reads it.
                self.flush_if_pending(slot)?;
                let r = self.results.remove(&slot).ok_or(QodecError::Protocol)?;
                if !part.resolved.select.is_empty() {
                    self.status |= select_status(&part.resolved.select, r.flags());
                }
                result = Some(r);
            }
        }
        Ok(result)
    }
}

impl<Id, P, R, B> Backend<Id, P, R> for LayerRuntime<Id, P, R, B>
where
    Id: InstructionId,
    P: Param,
    R: Readout,
    B: Backend<Id, P, R>,
{
    type Error = QodecError<B::Error>;

    fn execute_batch(
        &mut self,
        instructions: &[BackendInstruction<Id, P>],
    ) -> Result<BatchResult<R>, Self::Error> {
        if self.failed {
            return Err(QodecError::Poisoned);
        }
        let mark = self.side_effects;
        let result = self.execute_inner(instructions);
        if result.is_err() && self.side_effects != mark {
            // This layer and the layers below may disagree.
            self.failed = true;
        }
        self.sinks.clear();
        result
    }
}
