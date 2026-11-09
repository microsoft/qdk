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
    plan::{CompiledArg, GadgetPlan, InstructionPlan, LayerPlan},
    resolver::ResolveCache,
    spec::{
        BoundOperand, bind_operands, code_qubits, encode_arg, matches, select_status,
        validate_arguments, value_of,
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

/// A layer's result. Its error is raised by the layer itself or wraps the
/// lower backend's error `E` in [`QodecError::Lower`].
pub type LayerResult<T, E> = Result<T, QodecError<E>>;

/// The runtime of one qodec layer for one shot. Implements `Backend` for the
/// upper ISA and consumes the `Backend` below for the lower ISA. It is ready to
/// run once constructed; dropping it ends the shot.
pub struct LayerRuntime<Id, P, R, B> {
    plan: Rc<LayerPlan>,
    deferrable: bool,
    lower: B,

    // Shot state
    /// Live upper blocks, keyed by label: the upper block id that the caller
    /// passes as a `Block` operand. (In plan types, a label is a circuit
    /// label instead.)
    blocks: FxHashMap<u64, LiveBlock>,
    /// Released lower ids, reused lowest first.
    free: BinaryHeap<Reverse<u64>>,
    /// Lower type per lower id, `None` while the id is free. A `Vec` works
    /// because ids are dense (see `allocate_lower`).
    type_of: Vec<Option<usize>>,
    /// Last generation given to each upper label. It is kept when the label's
    /// block is released, so a reused label never repeats a generation within
    /// the shot.
    generation: FxHashMap<u64, u64>,
    next_invocation: u64,
    decoder: Box<dyn DecoderRuntime<R>>,
    /// Recoverable conditions raised so far. They stick for the rest of the
    /// shot and are merged into every later result.
    status: Status,
    /// Whether the layer is poisoned: an `Err` came after side effects, so this
    /// layer and the layers below may disagree. Every further `execute_batch`
    /// then fails with `QodecError::Poisoned`.
    failed: bool,
    /// Counts changes a failed batch cannot undo: every emit, every write to
    /// `blocks`, `free`, `type_of`, `generation`, and every decoder call or
    /// queued decoder event. `execute_batch` poisons the layer on `Err` only if
    /// this changed during the batch.
    side_effects: u64,

    // Lower-traffic buffer
    buffer: Vec<BackendInstruction<Id, P>>,
    buffer_slots: Vec<BufferSlot>,
    /// Results of flushed lower instructions, by result slot, kept until their
    /// last reader removes them.
    results: FxHashMap<u64, R>,
    /// The next result slot: the id `emit` gives a buffered lower instruction
    /// that produces a result, to find that result in `results` after a flush.
    /// Unrelated to the slots of a `SlotPlan`.
    next_slot: u64,
    decoder_log: VecDeque<DecoderEvent>,
    /// Decoded upper results of the current batch, one per gadget invocation.
    /// A sink is an index into it. It stays `None` until the invocation's QEC
    /// decode runs, which for a deferrable decoder is at flush time.
    sinks: Vec<Option<Decoded>>,
    cache: ResolveCache,
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

    /// The recoverable conditions raised so far in the shot.
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

    // ── execute_batch ───────────────────────────────────────────────────────

    fn execute_inner(
        &mut self,
        instructions: &[BackendInstruction<Id, P>],
    ) -> LayerResult<BatchResult<R>, B::Error> {
        self.sinks.clear();
        let mut wanted: SmallVec<[usize; 1]> = SmallVec::new();
        for ins in instructions {
            let call = self.parse_instruction(ins)?;
            let needs_result = self.plan.upper_needs_result[call.index];
            let sink = self.execute_one(call)?;
            if needs_result {
                // Only a gadget returns a sink, and only a gadget can need a
                // result: `LogicalPauli` and `Release` instructions have no
                // observe steps and no flags (`is_pauli_only`, `is_release`), and
                // `Unsupported` fails above. So `Protocol` here means a bug.
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

    // ── Parsing an incoming instruction (pure) ──────────────────────────────

    /// Parses `ins` into a [`Call`] and validates it against the upper ISA.
    /// `pseudocode.txt` (§8.5) calls this `decodeInstruction`; it is unrelated to
    /// QEC decoding by `self.decoder`. It only reads state, so a malformed
    /// instruction adds no side effects.
    fn parse_instruction(&self, ins: &BackendInstruction<Id, P>) -> LayerResult<Call, B::Error> {
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

    /// Returns the invocation's sink, or `None` for an instruction without a
    /// gadget, which produces no result.
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

    /// The read-only half of running a gadget; `run_invocation` is the other.
    /// It binds `call.operands`, checks each input against its live block, and
    /// builds the placement (see `PreparedInvocation::placement`).
    ///
    /// It only reads state, so a failure leaves nothing half done. On `Err`,
    /// `execute_batch` poisons the layer only if `side_effects` changed, so a
    /// failure here poisons only when earlier instructions of the same batch
    /// already had side effects.
    fn prepare_invocation(
        &self,
        gp: Rc<GadgetPlan>,
        call: Call,
    ) -> LayerResult<PreparedInvocation, B::Error> {
        // Live types constrain only the inputs (see `bind_operands`), so an
        // instruction may change a block's type.
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

        let mut placement = vec![UNPLACED; gp.labels.len()];
        for (label, slots) in call.operands.iter().zip(&gp.slots.inputs) {
            let b = self
                .blocks
                .get(label)
                .ok_or(QodecError::InvalidInstruction)?;
            if slots.len() != b.support.len() {
                return Err(QodecError::InvalidInstruction);
            }
            for (&slot, &lb) in slots.iter().zip(&b.support) {
                // A slot maps to one lower block, and the live block must already
                // have the slot's lower type: no silent retyping. The first check
                // is defensive: `layout_boundary` rejects a circuit label used
                // twice on one side, so input slots are distinct.
                if (placement[slot] != UNPLACED && placement[slot] != lb)
                    || self.type_of[ix(lb)] != Some(gp.slots.types[slot])
                {
                    return Err(QodecError::InvalidInstruction);
                }
                placement[slot] = lb;
            }
        }
        // Placement is injective: the inputs are distinct labels
        // (`bind_operands` rejects duplicates), and distinct live blocks never
        // share a lower block.
        debug_assert!(
            {
                let mut seen = FxHashSet::default();
                placement
                    .iter()
                    .all(|&lb| lb == UNPLACED || seen.insert(lb))
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

    /// The mutating half of running a gadget, after `prepare_invocation`.
    /// Returns the invocation's sink.
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

        // Operands past the inputs are output-only. A live block under such a
        // label is overwritten, so release it first.
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
        // blocks. It runs while inputs are still published. A deferrable
        // decoder has no `before`.
        if !self.deferrable {
            self.side_effects += 1;
            let first = self.decoder.before(&inv);
            self.drive(first, |layer, r| Ok(layer.decoder.resume_before(r)))?;
        }

        for (slot, lb) in placement.iter_mut().enumerate() {
            if *lb == UNPLACED {
                *lb = self.allocate_lower(gp.slots.types[slot]);
            }
        }

        let (raw, selects, slots) = self.run_body(&gp, &inv, &placement)?;

        // Publish outputs: an output block is the gadget's `out` encoding.
        for label in &inv.operands[..n_in] {
            self.blocks.remove(label);
        }
        self.side_effects += 1;
        // Outputs never share a lower block: a circuit label is used at most
        // once per side (see `layout_boundary`), and placement is injective.
        for ((block_ref, layout), output_slots) in inv
            .output_refs
            .iter()
            .zip(&gp.outputs)
            .zip(&gp.slots.outputs)
        {
            let support: Vec<u64> = output_slots.iter().map(|&s| placement[s]).collect();
            let qubits = code_qubits(layout, output_slots, &placement, &self.plan.lower);
            self.blocks.insert(
                block_ref.label,
                LiveBlock {
                    block_ref: block_ref.clone(),
                    support,
                    qubits,
                },
            );
        }

        // Release every lower block no output holds: scratch, and inputs the
        // gadget does not pass on. Nothing here depends on decoding, so release
        // instructions are emitted now. Blocks the body itself consumed
        // (`SlotPlan::consumed`, e.g. measured away) are already gone below, so
        // they only return to the free list. Releases go out in increasing
        // lower-id order, as in pseudocode §8.6, so the lower instruction
        // stream does not depend on the order of the gadget's labels.
        let mut leftovers: Vec<(u64, usize)> = placement
            .iter()
            .enumerate()
            .filter(|&(slot, _)| !gp.slots.retained[slot])
            .map(|(slot, &lb)| (lb, slot))
            .collect();
        leftovers.sort_unstable();
        for (lb, slot) in leftovers {
            if gp.slots.consumed[slot] {
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
            // Decoding now forces `raw`, which flushes the buffer if a bit is
            // still pending.
            let decoded = self.decode_now(&inv, &raw, &selects, &slots)?;
            self.sinks[sink] = Some(decoded);
            self.side_effects += 1;
            self.decoder.discarded(&inv.input_refs);
        }
        Ok(sink)
    }

    /// Emits one gadget body into the buffer. Returns its readout record as
    /// lazy bits, with the select checks and result slots its decode needs.
    fn run_body(
        &mut self,
        gp: &GadgetPlan,
        inv: &Invocation,
        placement: &[u64],
    ) -> LayerResult<BodyRecord, B::Error> {
        let mut record: Vec<LazyBit> = Vec::with_capacity(gp.body.record_len);
        let mut selects = Vec::new();
        let mut slots = Vec::new();
        for (call_index, (pc, cc)) in gp.body.calls.iter().zip(&gp.slots.calls).enumerate() {
            let args = cc.args.as_ref().ok_or(QodecError::InvalidInstruction)?;
            let mut params: SmallVec<[P; 2]> =
                SmallVec::with_capacity(cc.operands.len() + args.len());
            params.extend(
                cc.operands
                    .iter()
                    .map(|&slot| P::from_view(ParamView::Block(placement[slot]))),
            );
            for arg in args {
                params.push(match arg {
                    CompiledArg::Value(value) => P::from_view(value.view()),
                    CompiledArg::Forwarded { name, kind } => {
                        let value = inv
                            .args
                            .get(name)
                            .filter(|v| matches(*kind, v))
                            .ok_or(QodecError::InvalidInstruction)?;
                        encode_arg(value).ok_or(QodecError::InvalidInstruction)?
                    }
                    CompiledArg::Readout(j) => {
                        // Forcing an earlier bit may flush the buffer. A `None`
                        // bit is recoverable: it is passed as 0 and marks the
                        // shot `unresolved`.
                        let bit = self.force(record[*j])?;
                        if bit.is_none() {
                            self.status |= Status::UNRESOLVED;
                        }
                        P::from_view(ParamView::Bool(bit.unwrap_or(false)))
                    }
                });
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
                // The spec makes a per-call select only a hint
                // (https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/representations/yaml.md#per-call-select).
                // `decode_now` checks it once the flags are known and reports a
                // mismatch as a recoverable status.
                if !pc.select.is_empty() {
                    selects.push((slot, call_index));
                }
            }
        }
        Ok((record, selects, slots))
    }

    /// Runs the QEC decoder on one invocation. It forces the bits of `raw` and
    /// the results named in `selects`, flushing if any is still pending.
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

    /// Runs an upper Pauli-only instruction that has no gadget
    /// (`InstructionPlan::LogicalPauli`): applies the code's logical operator as
    /// Pauli corrections on the block's code qubits.
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

    /// Releases the live upper block `label`, if any. Used for an upper release
    /// instruction without a gadget, and before an output-only operand is
    /// overwritten.
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
            // `lower_release` only picks instructions without observe outcomes
            // or flags, so this emit returns no result slot.
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

    /// Runs a decoder coroutine (`before` or `decode`) to completion: applies
    /// each correction it yields and resumes it with that correction's result,
    /// if any.
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

    /// Lowers one correction and returns the result of its result-producing
    /// lower instruction, if any. A correction that would lower to more than
    /// one result-producing instruction fails with `NoImplementation`, because
    /// the decoder is resumed with a single result.
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
        // The targets as (lower block, logical index) pairs, which
        // `Resolver::resolve` calls slots: neither result slots nor `SlotPlan`
        // slots.
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

    // ── Buffering and flushing ──────────────────────────────────────────────

    /// Buffers one lower instruction. Returns its result slot iff it produces a
    /// result (observe outcomes or flags).
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

    fn force(&mut self, bit: LazyBit) -> LayerResult<Bit, B::Error> {
        self.flush_if_pending(bit.slot)?;
        let r = self.results.get(&bit.slot).ok_or(QodecError::Protocol)?;
        Ok(if bit.flag {
            r.flags()[bit.index]
        } else {
            r.outcomes()[bit.index]
        })
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
}

/// A live upper block: the block a label currently names, from a gadget output
/// that is not yet consumed or released.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveBlock {
    /// The block's current reference.
    pub block_ref: BlockRef,
    /// The lower block ids holding it, in the order of its output encoding's
    /// labels.
    pub support: Vec<u64>,
    /// Its code qubits, in code order: (lower block, logical index in it).
    pub qubits: Vec<(u64, usize)>,
}

/// One result-producing buffered lower instruction: its result slot, and the
/// `R` shape its result must have.
#[derive(Clone, Copy, Debug)]
struct BufferSlot {
    slot: u64,
    outcomes: usize,
    flags: usize,
}

/// Deferred decoder work, in program order. Queued only for a deferrable
/// decoder, and run by `flush`.
enum DecoderEvent {
    Decode {
        inv: Invocation,
        /// The body's circuit readout record.
        raw: Vec<LazyBit>,
        /// (result slot, index into `body.calls` of the call whose select it
        /// checks).
        selects: Vec<(u64, usize)>,
        /// Every result-producing slot of the body. This decode is their last
        /// reader and frees them.
        slots: Vec<u64>,
        /// The index in `sinks` that receives the decoded result.
        sink: usize,
    },
    Discarded(Vec<BlockRef>),
}

/// What running a gadget body leaves for its decode: the same `raw`, `selects`
/// and `slots` as [`DecoderEvent::Decode`].
type BodyRecord = (Vec<LazyBit>, Vec<(u64, usize)>, Vec<u64>);

/// A lower result bit that may not be known until its batch is flushed.
#[derive(Clone, Copy, Debug)]
struct LazyBit {
    slot: u64,
    flag: bool,
    index: usize,
}

/// The output of `prepare_invocation`, consumed by `run_invocation`.
struct PreparedInvocation {
    gp: Rc<GadgetPlan>,
    call: Call,
    out_ops: Vec<BoundOperand<u64>>,
    /// The placement: the lower block id per slot of `gp.slots` (see
    /// [`SlotPlan`](crate::plan::SlotPlan)). Input slots hold their live
    /// blocks' ids; the others stay `UNPLACED` until `run_invocation`
    /// allocates them.
    placement: Vec<u64>,
}

/// Marks a placement slot without a lower block yet. Lower ids are allocated
/// densely from 0, so no real id reaches `u64::MAX`.
const UNPLACED: u64 = u64::MAX;

/// An upper instruction, parsed by `parse_instruction`.
struct Call {
    /// Index into the upper ISA.
    index: usize,
    /// Upper labels, one per block operand.
    operands: Vec<u64>,
    /// Arguments by parameter name, validated against the instruction.
    args: BTreeMap<String, Argument>,
}

// Lower ids are only created by `allocate_lower` as `type_of.len()`, a
// `usize`, so this cast never truncates.
#[allow(clippy::cast_possible_truncation)]
fn ix(lower_id: u64) -> usize {
    lower_id as usize
}
