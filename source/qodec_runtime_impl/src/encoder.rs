// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`SpecEncoder`]: the [`Encoder`] that lowers each upper call with the gadget
//! the layer declares for it, as the qodec spec describes.

use super::{
    actions::PauliExpr,
    backend::Backend,
    layer::{Blueprint, Layer},
    pauli::Pauli,
    plan::PlanError,
    plan::{CompiledArg, GadgetPlan, InstructionPlan, LayerPlan},
    protocol::{
        BlockRef, Call, Correction, Encoder, Fault, Fix, Invocation, Lower, Never, OwnedParam,
        Record, Reply, Status, Step,
    },
    resolver::ResolveCache,
    spec::{
        BoundOperand, CompiledSelect, ParsedCall, bind_operands, code_qubits, encode_arg, matches,
        parse_call, select_status,
    },
};
use qodec::Action;
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::{cmp::Reverse, collections::BinaryHeap, marker::PhantomData, rc::Rc};

/// The plan of a [`SpecEncoder`] for one layer. It accepts corrections of type
/// `C`, after checking that the layer can apply them.
pub struct SpecEncoderPlan<C = Never> {
    layer: Rc<LayerPlan>,
    fix: PhantomData<fn(C)>,
}

/// A [`Fix`] that a [`SpecEncoder`] checks at plan time: the lower ISA must be
/// able to apply this kind of correction.
pub trait PlannedFix: Fix {
    /// Checks that `layer` can apply this kind of correction.
    fn check(layer: &LayerPlan) -> Result<(), PlanError>;
}

impl PlannedFix for Never {
    fn check(_layer: &LayerPlan) -> Result<(), PlanError> {
        Ok(())
    }
}

/// Decoders usually correct with Paulis, so the lower ISA must have `X` and
/// `Z` (see [`LayerPlan::check_pauli_corrections`]). Other steps are checked
/// when they are applied.
impl PlannedFix for Correction {
    fn check(layer: &LayerPlan) -> Result<(), PlanError> {
        layer.check_pauli_corrections()
    }
}

impl<C: PlannedFix> SpecEncoderPlan<C> {
    /// Plans the encoder of `layer`.
    pub fn new(layer: &Rc<LayerPlan>) -> Result<Self, PlanError> {
        C::check(layer)?;
        Ok(Self {
            layer: Rc::clone(layer),
            fix: PhantomData,
        })
    }
}

impl<C: PlannedFix> Blueprint for SpecEncoderPlan<C> {
    type Instance = SpecEncoder<C>;

    fn instantiate(&self) -> SpecEncoder<C> {
        SpecEncoder::new(Rc::clone(&self.layer))
    }
}

/// Lowers each upper call with the layer's gadget for it, or as a logical Pauli
/// or a release when the layer has no gadget (see [`InstructionPlan`]).
///
/// ```text
///   │ Vec<Lower<C>>  upper calls and corrections  ▲ Result<Vec<Record>, Fault>
///  ┌▼─────────────────────────────────────────────┴───────────────────────────┐
///  │ SpecEncoder<C>                                                           │
///  │   live blocks  label → support (lower block ids, in code-qubit order)    │
///  │   lower ids    allocated on demand, densely from 0, lowest released first│
///  └┬─────────────────────────────────────────────▲───────────────────────────┘
///   │ Vec<Call>  lower ISA                         │ Result<Vec<Reply>, Fault>
/// ```
///
/// It buffers the lower calls of a batch and sends them below in one call at
/// the end. A gadget that feeds a readout into a later `bit` argument sends
/// the buffer early, to get that readout, and goes on.
///
/// The results of corrections are not returned. Their recoverable conditions,
/// such as a raised flag, are added to the last record of the batch, or of the
/// next batch that has records.
pub struct SpecEncoder<C = Never> {
    plan: Rc<LayerPlan>,

    /// Live upper blocks, keyed by label: the upper block id that the caller
    /// passes as a `Block` operand.
    blocks: FxHashMap<u64, LiveBlock>,
    /// Released lower ids, reused lowest first.
    free: BinaryHeap<Reverse<u64>>,
    /// Lower type per lower id, `None` while the id is free. A `Vec` works
    /// because ids are dense (see `allocate_lower`).
    type_of: Vec<Option<usize>>,
    /// Last generation given to each upper label. It is kept when the label's
    /// block is released, so a reused label never repeats a generation.
    generation: FxHashMap<u64, u64>,
    next_invocation: u64,
    cache: ResolveCache,
    /// Conditions raised by corrections, not yet added to a record.
    absorbed: Status,
    /// Whether results never come back (see [`Encoder::expect_no_results`]).
    no_results: bool,
    fix: PhantomData<fn(C)>,
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

/// The lower backend an encoder calls.
type Below<'b> = dyn Backend<Vec<Call>, Result<Vec<Reply>, Fault>> + 'b;

/// The work of one `execute`.
struct Batch<'a, 'b> {
    below: &'a mut Below<'b>,
    /// The records of the batch's calls so far.
    records: Vec<Record>,
    /// Per record: how many of its bits have arrived. Bits arrive in order.
    filled: Vec<usize>,
    /// Lower calls not sent below yet.
    buffer: Vec<Call>,
    /// What each result of `buffer` is for, in order.
    expected: Vec<Expected>,
}

impl Batch<'_, '_> {
    /// Starts the record of a call, and returns its index in the batch.
    fn open_record(&mut self, instruction: usize, bits: usize) -> usize {
        self.records.push(Record {
            instruction,
            invocation: None,
            bits: vec![None; bits],
            released: Vec::new(),
            status: Status::default(),
        });
        self.filled.push(0);
        self.records.len() - 1
    }
}

/// What a lower result is for.
enum Expected {
    /// Bits `offset..` of the batch's record `record`.
    Bits {
        record: usize,
        offset: usize,
        outcomes: usize,
        flags: usize,
    },
    /// A correction's result: only its conditions are kept.
    Absorb {
        outcomes: usize,
        flags: usize,
        select: CompiledSelect,
    },
}

impl<C> SpecEncoder<C> {
    /// A fresh encoder for one shot of `plan`'s layer.
    #[must_use]
    pub fn new(plan: Rc<LayerPlan>) -> Self {
        Self {
            plan,
            blocks: FxHashMap::default(),
            free: BinaryHeap::new(),
            type_of: Vec::new(),
            generation: FxHashMap::default(),
            next_invocation: 0,
            cache: ResolveCache::default(),
            absorbed: Status::default(),
            no_results: false,
            fix: PhantomData,
        }
    }

    /// The layer's plan.
    #[must_use]
    pub fn plan(&self) -> &Rc<LayerPlan> {
        &self.plan
    }

    /// The live upper block with this label, if any.
    #[must_use]
    pub fn live_block(&self, label: u64) -> Option<&LiveBlock> {
        self.blocks.get(&label)
    }
}

impl<C: Fix> Layer for SpecEncoder<C> {
    type Input = Vec<Lower<C>>;
    type Output = Result<Vec<Record>, Fault>;
    type LowerInput = Vec<Call>;
    type LowerOutput = Result<Vec<Reply>, Fault>;

    fn execute(
        &mut self,
        input: Vec<Lower<C>>,
        below: &mut dyn Backend<Vec<Call>, Result<Vec<Reply>, Fault>>,
    ) -> Result<Vec<Record>, Fault> {
        let mut b = Batch {
            below,
            records: Vec::with_capacity(input.len()),
            filled: Vec::with_capacity(input.len()),
            buffer: Vec::new(),
            expected: Vec::new(),
        };
        for message in input {
            match message {
                Lower::Call(call) => self.lower_call(&call, &mut b)?,
                Lower::Fix(fix) => self.apply_correction(&fix.into_correction(), &mut b)?,
            }
        }
        self.flush(&mut b)?;
        let mut records = b.records;
        if let Some(last) = records.last_mut() {
            last.status |= std::mem::take(&mut self.absorbed);
        }
        Ok(records)
    }
}

impl<C: Fix> Encoder<C> for SpecEncoder<C> {
    fn expect_no_results(&mut self) {
        self.no_results = true;
    }
}

impl<C: Fix> SpecEncoder<C> {
    /// Sends the buffered lower calls below, and files their results.
    fn flush(&mut self, b: &mut Batch<'_, '_>) -> Result<(), Fault> {
        if b.buffer.is_empty() {
            return Ok(());
        }
        let calls = std::mem::take(&mut b.buffer);
        let expected = std::mem::take(&mut b.expected);
        let results = b.below.execute(calls)?;
        if self.no_results {
            return Ok(());
        }
        if results.len() != expected.len() {
            return Err(Fault::protocol(format!(
                "{} lower results for {} calls that have results",
                results.len(),
                expected.len()
            )));
        }
        for (reply, expected) in results.into_iter().zip(expected) {
            let readout = reply.into_ready();
            let (outcomes, flags) = match &expected {
                Expected::Bits {
                    outcomes, flags, ..
                }
                | Expected::Absorb {
                    outcomes, flags, ..
                } => (*outcomes, *flags),
            };
            if readout.outcomes.len() != outcomes || readout.flags.len() != flags {
                return Err(Fault::protocol(format!(
                    "a lower result with {} outcomes and {} flags, expected {outcomes} and {flags}",
                    readout.outcomes.len(),
                    readout.flags.len()
                )));
            }
            match expected {
                Expected::Bits { record, offset, .. } => {
                    // A gadget's lower results arrive in the order of its calls.
                    debug_assert_eq!(offset, b.filled[record], "record bits arrive in order");
                    let bits = readout.outcomes.iter().chain(&readout.flags);
                    for (slot, &bit) in b.records[record].bits[offset..].iter_mut().zip(bits) {
                        *slot = bit;
                    }
                    b.filled[record] += outcomes + flags;
                    b.records[record].status |= readout.status;
                }
                Expected::Absorb { select, .. } => {
                    self.absorbed |= readout.status | select_status(&select, &readout.flags);
                }
            }
        }
        Ok(())
    }

    // ── Parsing an upper call (read-only) ───────────────────────────────────

    fn parse(&self, call: &Call) -> Result<ParsedCall, Fault> {
        parse_call(&self.plan.isa, call.instruction_id, &call.parameters).map_err(Fault::invalid)
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

    // ── Lowering one upper call ─────────────────────────────────────────────

    fn lower_call(&mut self, call: &Call, b: &mut Batch<'_, '_>) -> Result<(), Fault> {
        let parsed = self.parse(call)?;
        let instruction_plan = self.plan.instructions[parsed.index].clone();
        match instruction_plan {
            InstructionPlan::Gadget(gp) => self.start_gadget(&gp, parsed, b),
            InstructionPlan::LogicalPauli => {
                self.apply_logical_pauli(&parsed, b)?;
                b.open_record(parsed.index, 0);
                Ok(())
            }
            InstructionPlan::Release => {
                let [label] = parsed.operands.as_slice() else {
                    return Err(Fault::invalid("a release takes one block"));
                };
                let released = self.release(*label, b);
                let record = b.open_record(parsed.index, 0);
                b.records[record].released.extend(released);
                Ok(())
            }
            InstructionPlan::Unsupported => Err(Fault::no_implementation(format!(
                "no gadget implements `{}`",
                self.plan.isa.instructions[parsed.index].mnemonic
            ))),
        }
    }

    /// Binds a gadget's operands and places its inputs, without changing any
    /// state, then runs it.
    fn start_gadget(
        &mut self,
        gp: &GadgetPlan,
        parsed: ParsedCall,
        b: &mut Batch<'_, '_>,
    ) -> Result<(), Fault> {
        // Live types constrain only the inputs (see `bind_operands`), so an
        // instruction may change a block's type.
        let input_types = self.input_types(&parsed.operands);
        let (in_ops, out_ops) = bind_operands(
            &self.plan.upper,
            gp.instruction(),
            &parsed.operands,
            &input_types,
        )
        .map_err(Fault::invalid)?;
        if in_ops.len() != gp.inputs.len() || out_ops.len() != gp.outputs.len() {
            return Err(Fault::invalid(
                "operands do not match the gadget's encodings",
            ));
        }

        let mut placement = vec![UNPLACED; gp.labels.len()];
        for (label, slots) in parsed.operands.iter().zip(&gp.slots.inputs) {
            let live = self
                .blocks
                .get(label)
                .ok_or_else(|| Fault::invalid(format!("block {label} is not live")))?;
            if slots.len() != live.support.len() {
                return Err(Fault::invalid(format!(
                    "block {label} does not fit the gadget's input encoding"
                )));
            }
            for (&slot, &lb) in slots.iter().zip(&live.support) {
                // A slot maps to one lower block, and the live block must already
                // have the slot's lower type: no silent retyping.
                if (placement[slot] != UNPLACED && placement[slot] != lb)
                    || self.type_of[ix(lb)] != Some(gp.slots.types[slot])
                {
                    return Err(Fault::invalid(format!(
                        "block {label} does not fit the gadget's input encoding"
                    )));
                }
                placement[slot] = lb;
            }
        }
        self.run_gadget(gp, parsed, &out_ops, placement, b)
    }

    /// The mutating half of starting a gadget: binds outputs, allocates the
    /// lower blocks the inputs don't provide, and runs the body.
    fn run_gadget(
        &mut self,
        gp: &GadgetPlan,
        parsed: ParsedCall,
        out_ops: &[BoundOperand<u64>],
        mut placement: Vec<u64>,
        b: &mut Batch<'_, '_>,
    ) -> Result<(), Fault> {
        let n_in = gp.inputs.len();
        // Operands past the inputs are output-only. A live block under such a
        // label is overwritten, so release it first.
        let mut released = Vec::new();
        for &label in &parsed.operands[n_in..] {
            released.extend(self.release(label, b));
        }
        let input_refs: Vec<BlockRef> = parsed.operands[..n_in]
            .iter()
            .map(|l| self.blocks[l].block_ref.clone())
            .collect();
        let output_refs: Vec<BlockRef> = out_ops
            .iter()
            .map(|o| self.fresh_ref(o.label, &o.block_type))
            .collect();

        for (slot, lb) in placement.iter_mut().enumerate() {
            if *lb == UNPLACED {
                *lb = self.allocate_lower(gp.slots.types[slot]);
            }
        }

        let id = self.next_invocation;
        self.next_invocation += 1;
        let record = b.open_record(parsed.index, gp.body.record_len);
        b.records[record].released = released;
        b.records[record].invocation = Some(Invocation {
            id,
            gadget: gp.id,
            operands: parsed.operands,
            args: parsed.args,
            input_refs,
            output_refs,
        });
        self.run_body(gp, record, &placement, b)?;
        self.finish_gadget(gp, record, &placement, b);
        Ok(())
    }

    /// Emits a gadget's body calls. A call that takes an earlier readout of the
    /// body sends the buffer below first, if that readout has not arrived.
    fn run_body(
        &mut self,
        gp: &GadgetPlan,
        record: usize,
        placement: &[u64],
        b: &mut Batch<'_, '_>,
    ) -> Result<(), Fault> {
        let mut offset = 0;
        for (pc, cc) in gp.body.calls.iter().zip(&gp.slots.calls) {
            let args = cc.args.as_ref().ok_or_else(|| {
                Fault::invalid(format!(
                    "gadget `{}` passes invalid arguments to `{}`",
                    gp.name, self.plan.lower_isa.instructions[pc.instruction].mnemonic
                ))
            })?;
            let mut params: SmallVec<[OwnedParam; 2]> =
                SmallVec::with_capacity(cc.operands.len() + args.len());
            params.extend(
                cc.operands
                    .iter()
                    .map(|&slot| OwnedParam::Block(placement[slot])),
            );
            for arg in args {
                params.push(match arg {
                    CompiledArg::Value(value) => value.clone(),
                    CompiledArg::Forwarded { name, kind } => {
                        let inv = b.records[record]
                            .invocation
                            .as_ref()
                            .expect("a gadget record has an invocation");
                        let value = inv
                            .args
                            .get(name)
                            .filter(|v| matches(*kind, v))
                            .ok_or_else(|| {
                                Fault::invalid(format!("argument `{name}` does not fit its use"))
                            })?;
                        encode_arg(value).ok_or_else(|| {
                            Fault::invalid(format!("argument `{name}` does not fit its use"))
                        })?
                    }
                    CompiledArg::Readout(j) => {
                        if *j >= b.filled[record] {
                            if self.no_results {
                                return Err(Fault::no_implementation(format!(
                                    "gadget `{}` needs a readout to go on, and no results come back",
                                    gp.name
                                )));
                            }
                            self.flush(b)?;
                        }
                        let bit = b.records[record].bits[*j];
                        // A `None` bit is recoverable: it is passed as 0 and
                        // marks the record `unresolved`.
                        if bit.is_none() {
                            b.records[record].status |= Status::UNRESOLVED;
                        }
                        OwnedParam::Bool(bit.unwrap_or(false))
                    }
                });
            }
            b.buffer.push(Call {
                instruction_id: pc.instruction,
                parameters: params,
            });
            let count = pc.observe_count + pc.flag_count;
            if count > 0 && !self.no_results {
                b.expected.push(Expected::Bits {
                    record,
                    offset,
                    outcomes: pc.observe_count,
                    flags: pc.flag_count,
                });
            }
            offset += count;
        }
        Ok(())
    }

    /// Publishes a gadget's outputs and releases the lower blocks no output
    /// holds.
    fn finish_gadget(
        &mut self,
        gp: &GadgetPlan,
        record: usize,
        placement: &[u64],
        b: &mut Batch<'_, '_>,
    ) {
        let inv = b.records[record]
            .invocation
            .as_ref()
            .expect("a gadget record has an invocation");
        for label in &inv.operands[..gp.inputs.len()] {
            self.blocks.remove(label);
        }
        // Outputs never share a lower block: a circuit label is used at most
        // once per side (see `layout_boundary`), and placement is injective.
        for ((block_ref, layout), output_slots) in inv
            .output_refs
            .iter()
            .zip(&gp.outputs)
            .zip(&gp.slots.outputs)
        {
            let support: Vec<u64> = output_slots.iter().map(|&s| placement[s]).collect();
            let qubits = code_qubits(layout, output_slots, placement, &self.plan.lower);
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
        // gadget does not pass on. Blocks the body itself consumed are already
        // gone below, so they only return to the free list. Releases go out in
        // increasing lower-id order, so the lower calls do not depend on the
        // order of the gadget's labels.
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
                self.release_lower(lb, b);
            }
        }
    }

    fn fresh_ref(&mut self, label: u64, block_type: &str) -> BlockRef {
        let generation = self.generation.entry(label).or_insert(0);
        *generation += 1;
        BlockRef {
            label,
            generation: *generation,
            block_type: block_type.to_string(),
        }
    }

    // ── Logical Paulis and corrections ──────────────────────────────────────

    /// Runs an upper Pauli-only instruction that has no gadget: applies the
    /// code's logical operator as corrections on the block's code qubits.
    fn apply_logical_pauli(
        &mut self,
        call: &ParsedCall,
        b: &mut Batch<'_, '_>,
    ) -> Result<(), Fault> {
        let plan = Rc::clone(&self.plan);
        let instruction = &plan.isa.instructions[call.index];
        // Signs are dropped: the spec defines Pauli steps only up to global
        // phase (https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#actions).
        let mut product = Pauli::default();
        for step in &instruction.action {
            if let Action::Pauli(text) = &step.action {
                let p = PauliExpr::parse(instruction, text)
                    .and_then(|e| e.resolve(&call.args))
                    .map_err(Fault::invalid)?;
                product = product.mul_unsigned(&p);
            }
        }
        let input_types = self.input_types(&call.operands);
        let (ins, _) = bind_operands(&plan.upper, instruction, &call.operands, &input_types)
            .map_err(Fault::invalid)?;
        for bound in ins {
            let block_ref = self
                .blocks
                .get(&bound.label)
                .ok_or_else(|| Fault::invalid(format!("block {} is not live", bound.label)))?
                .block_ref
                .clone();
            let code = plan.codes.get(&bound.block_type).ok_or_else(|| {
                Fault::no_implementation(format!("no code for `{}`", bound.block_type))
            })?;
            let mut logical = Pauli::default();
            for i in 0..bound.width {
                if let Some(op) = product.get(bound.offset + i) {
                    let missing = || Fault::no_implementation("the code lacks a logical operator");
                    if op.has_x() {
                        logical = logical.mul_unsigned(code.x.get(i).ok_or_else(missing)?);
                    }
                    if op.has_z() {
                        logical = logical.mul_unsigned(code.z.get(i).ok_or_else(missing)?);
                    }
                }
            }
            if !logical.is_identity() {
                self.apply_correction(
                    &Correction {
                        blocks: vec![block_ref],
                        step: Step::Pauli(logical),
                    },
                    b,
                )?;
            }
        }
        Ok(())
    }

    /// Lowers one correction through the lower ISA's resolver.
    fn apply_correction(&mut self, c: &Correction, b: &mut Batch<'_, '_>) -> Result<(), Fault> {
        let mut seen = FxHashSet::default();
        if !c.blocks.iter().all(|r| seen.insert(r.label)) {
            return Err(Fault::invalid("a correction targets a block twice"));
        }
        let mut qubits = Vec::new();
        for block_ref in &c.blocks {
            let live = self
                .blocks
                .get(&block_ref.label)
                .filter(|live| live.block_ref == *block_ref) // same generation
                .ok_or_else(|| {
                    Fault::invalid(format!(
                        "a correction targets block {} generation {}, which is not live",
                        block_ref.label, block_ref.generation
                    ))
                })?;
            qubits.extend_from_slice(&live.qubits);
        }
        // The step on its own qubits, and their (lower block, logical index)
        // addresses.
        let (support, local) = c.step.localize().map_err(Fault::invalid)?;
        let slots: Vec<(u64, usize)> = support
            .iter()
            .map(|&i| {
                qubits.get(i).copied().ok_or_else(|| {
                    Fault::invalid(format!("a correction targets code qubit {i}, out of range"))
                })
            })
            .collect::<Result<_, _>>()?;

        let plan = Rc::clone(&self.plan);
        let type_of = &self.type_of;
        let parts = plan
            .resolver
            .resolve(
                &local,
                &slots,
                |lb| type_of[ix(lb)].unwrap_or_default(),
                &plan.lower,
                &mut self.cache,
            )
            .ok_or_else(|| {
                Fault::no_implementation(format!("the lower ISA cannot apply {} there", c.step))
            })?;
        for part in parts {
            let instruction = &plan.lower_isa.instructions[part.resolved.index];
            let mut params: SmallVec<[OwnedParam; 2]> = part
                .operands
                .iter()
                .map(|&o| OwnedParam::Block(o))
                .collect();
            for parameter in &instruction.parameters {
                let arg = part.resolved.args.get(&parameter.name).ok_or_else(|| {
                    Fault::invalid(format!("no argument for `{}`", parameter.name))
                })?;
                params.push(encode_arg(arg).ok_or_else(|| {
                    Fault::invalid(format!("no argument for `{}`", parameter.name))
                })?);
            }
            b.buffer.push(Call {
                instruction_id: part.resolved.index,
                parameters: params,
            });
            let (outcomes, flags) = plan.lower_shapes[part.resolved.index];
            if outcomes + flags > 0 && !self.no_results {
                b.expected.push(Expected::Absorb {
                    outcomes,
                    flags,
                    select: part.resolved.select.clone(),
                });
            }
        }
        Ok(())
    }

    // ── Lower block ids ─────────────────────────────────────────────────────

    /// Releases the live upper block `label`, if any, and returns its
    /// reference.
    fn release(&mut self, label: u64, b: &mut Batch<'_, '_>) -> Option<BlockRef> {
        let live = self.blocks.remove(&label)?;
        for &lb in &live.support {
            self.release_lower(lb, b);
        }
        Some(live.block_ref)
    }

    /// Releases a lower block that is still live below. Eager when the lower ISA
    /// has a release instruction for its type; otherwise lazy: nothing is
    /// emitted, and the block stays live below until this layer reuses the id.
    fn release_lower(&mut self, lb: u64, b: &mut Batch<'_, '_>) {
        let t = self.type_of[ix(lb)].unwrap_or_default();
        if let Some(index) = self.plan.lower_release[t] {
            // `lower_release` only picks instructions without results.
            b.buffer.push(Call {
                instruction_id: index,
                parameters: SmallVec::from_iter([OwnedParam::Block(lb)]),
            });
        }
        self.forget_lower(lb);
    }

    /// Returns a lower block id to the free list without emitting anything.
    fn forget_lower(&mut self, lb: u64) {
        self.type_of[ix(lb)] = None;
        self.free.push(Reverse(lb));
    }

    /// Allocates a lower block id on demand: the lowest released id first,
    /// otherwise a fresh one. Ids therefore stay dense, so the next fresh id is
    /// `type_of.len()`. There is no limit here: a backend below that cannot
    /// hold the id fails it.
    fn allocate_lower(&mut self, lower_type: usize) -> u64 {
        if let Some(Reverse(lb)) = self.free.pop() {
            self.type_of[ix(lb)] = Some(lower_type);
            lb
        } else {
            self.type_of.push(Some(lower_type));
            (self.type_of.len() - 1) as u64
        }
    }
}

/// Marks a placement slot without a lower block yet. Lower ids are allocated
/// densely from 0, so no real id reaches `u64::MAX`.
const UNPLACED: u64 = u64::MAX;

// Lower ids are only created by `allocate_lower` as `type_of.len()`, a
// `usize`, so this cast never truncates.
#[allow(clippy::cast_possible_truncation)]
fn ix(lower_id: u64) -> usize {
    lower_id as usize
}
