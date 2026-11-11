// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [`GadgetEncoder`]: the [`Encoder`] that lowers each upper call with the gadget
//! the layer declares for it, as the qodec spec describes; the
//! [`EncoderProgram`] it runs; and how that program is planned from a
//! [`LayerModel`].

use super::{
    action::{ParsedPauli, StepAction},
    backend::Backend,
    error::PlanError,
    isa::{BoundOperand, Isa, Operand, ParsedCall, Signature},
    layer::{Blueprint, Layer},
    layer_model::{Arg, GadgetModel, LayerModel},
    pauli::{Pauli, PauliOp},
    protocol::{
        BlockRef, Call, Correction, Encoder, Fault, Fix, GadgetId, Invocation, Lower, Never,
        OwnedParam, Record, Reply, Status, Step,
    },
    resolver::{Resolver, ResolverFixes},
    select::{self, CompiledSelect, SelectPolicy, accepts},
};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::{cmp::Reverse, collections::BinaryHeap, marker::PhantomData, sync::Arc};

/// What a [`GadgetEncoder`] runs on: how to lower each call of one layer's
/// upper ISA to calls of its lower ISA.
#[derive(Clone)]
pub struct EncoderProgram {
    /// The upper ISA, whose calls the encoder takes.
    pub upper: Arc<Isa>,
    /// The lower ISA, whose calls it emits.
    pub lower: Arc<Isa>,
    /// Per upper instruction: how the encoder lowers it.
    pub instructions: Vec<Lowering>,
    /// Per upper block type: the logical operators of its code, which
    /// [`Lowering::LogicalPauli`] applies, if the layer binds a code to it.
    pub codes: Vec<Option<LogicalOps>>,
    /// Per lower type `t`: the lower instruction that releases a block of type
    /// `t`. It has no result. If a type has none, releasing a block of that
    /// type emits nothing: the block stays live below until the encoder reuses
    /// its id.
    pub lower_release: Vec<Option<usize>>,
    /// How to apply a correction with lower calls.
    pub fixes: Arc<dyn Fixes>,
    /// What a correction's flags, under its `select` hint, add to the batch.
    pub select_policy: SelectPolicy,
}

impl std::fmt::Debug for EncoderProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EncoderProgram")
            .field("upper", &self.upper)
            .field("lower", &self.lower)
            .field("instructions", &self.instructions)
            .field("codes", &self.codes)
            .field("lower_release", &self.lower_release)
            .finish_non_exhaustive()
    }
}

impl EncoderProgram {
    /// The program of `layer`'s encoders, which apply corrections with
    /// `resolver`, a resolver of the lower ISA. Upper Pauli-only instructions
    /// without a gadget are applied as corrections, so a layer with one needs
    /// lower Paulis (see [`check_pauli_corrections`]).
    pub fn plan(layer: &LayerModel, resolver: &Arc<Resolver>) -> Result<Self, PlanError> {
        let instructions: Vec<Lowering> = layer
            .upper
            .instructions
            .iter()
            .zip(&layer.gadget_of)
            .map(|(instruction, gadget)| -> Result<Lowering, PlanError> {
                Ok(if let Some(id) = gadget {
                    let gadget = &layer.gadgets[id.0];
                    let program = GadgetProgram::plan(gadget, layer)
                        .map_err(|e| PlanError(format!("gadget `{}`: {e}", gadget.name)))?;
                    Lowering::Gadget(Arc::new(program))
                } else if is_pauli_only(instruction) {
                    Lowering::LogicalPauli(
                        instruction
                            .action
                            .iter()
                            .filter_map(|step| match &step.action {
                                StepAction::Pauli(p) => Some(p.clone()),
                                _ => None,
                            })
                            .collect(),
                    )
                } else if is_release(instruction) {
                    Lowering::Release
                } else {
                    Lowering::Unsupported
                })
            })
            .collect::<Result<_, _>>()?;
        if instructions
            .iter()
            .any(|l| matches!(l, Lowering::LogicalPauli(_)))
        {
            check_pauli_corrections(resolver, &layer.lower)?;
        }
        let lower = &layer.lower;
        let lower_release = (0..lower.types.len())
            .map(|t| {
                lower.instructions.iter().position(|i| {
                    matches!(i.inputs.as_slice(), [Operand { block, variadic: false }] if *block == t)
                        && i.outputs.is_empty()
                        && i.outcomes == 0
                        && i.flags == 0
                })
            })
            .collect();
        Ok(Self {
            upper: Arc::clone(&layer.upper),
            lower: Arc::clone(lower),
            instructions,
            codes: layer
                .codes
                .iter()
                .map(|code| {
                    code.as_ref().map(|code| LogicalOps {
                        x: code.x.clone(),
                        z: code.z.clone(),
                    })
                })
                .collect(),
            lower_release,
            fixes: Arc::new(ResolverFixes::new(Arc::clone(resolver), Arc::clone(lower))),
            select_policy: select::recoverable,
        })
    }
}

/// Whether `instruction` is lowered as [`Lowering::LogicalPauli`] when no
/// gadget implements it: its action is non-empty and has only unguarded `pauli`
/// steps, it has no flags, and its boundary is in place: at least one input,
/// and outputs identical to the inputs.
fn is_pauli_only(instruction: &Signature) -> bool {
    !instruction.action.is_empty()
        && instruction
            .action
            .iter()
            .all(|s| !s.guarded && matches!(s.action, StepAction::Pauli(_)))
        && instruction.flags == 0
        && !instruction.inputs.is_empty()
        && instruction.inputs == instruction.outputs
}

/// Whether `instruction` is lowered as [`Lowering::Release`] when no gadget
/// implements it: `in = [t]` with `t` not variadic, `out = []`, empty action,
/// no flags.
fn is_release(instruction: &Signature) -> bool {
    matches!(instruction.inputs.as_slice(), [op] if !op.variadic)
        && instruction.outputs.is_empty()
        && instruction.action.is_empty()
        && instruction.flags == 0
}

/// Checks that `lower` can carry Pauli corrections: one instruction must
/// apply `X`, and one `Z`, on logical index 0 of every lower type. A `Y` falls
/// back to them (see [`Resolver::resolve`]).
///
/// Only an encoder that issues Pauli corrections needs this: one with a
/// [`Lowering::LogicalPauli`] instruction, or that accepts [`Correction`]s. The
/// check catches a lower ISA without Pauli instructions at plan time. It
/// doesn't cover the other logical indices of a multi-qubit lower type; a
/// correction the resolver can't bind there fails when it is issued, with
/// `NoImplementation`.
pub fn check_pauli_corrections(resolver: &Resolver, lower: &Isa) -> Result<(), PlanError> {
    for (t, name) in lower.types.names.iter().enumerate() {
        for op in [PauliOp::X, PauliOp::Z] {
            if !resolver.binds_directly(op, t, &lower.types) {
                return Err(PlanError(format!(
                    "the lower ISA has no `{op:?}` Pauli for block type `{name}`"
                )));
            }
        }
    }
    Ok(())
}

/// How a [`GadgetEncoder`] lowers one upper instruction.
#[derive(Clone, Debug)]
pub enum Lowering {
    /// Runs this gadget.
    Gadget(Arc<GadgetProgram>),
    /// A Pauli-only instruction without a gadget: applied as the code's logical
    /// operator, through corrections. The Paulis are the action's `pauli`
    /// steps, on the instruction's flat indices; their product is applied.
    LogicalPauli(Vec<ParsedPauli>),
    /// Releases the block.
    Release,
    /// Fails with `NoImplementation`.
    Unsupported,
}

/// The logical operators of a code: one X and one Z per logical qubit, on the
/// code's qubits.
#[derive(Clone, Debug, Default)]
pub struct LogicalOps {
    /// Logical X operators.
    pub x: Vec<Pauli>,
    /// Logical Z operators.
    pub z: Vec<Pauli>,
}

/// How a [`GadgetEncoder`] runs one gadget.
///
/// Every label of the gadget's circuit is a slot. An invocation places each
/// slot on a lower block id. For example, with slots for labels `0`, `1` and
/// `anc`, the body call `CX: [0, anc]` has slot operands `[0, 2]`. If an
/// invocation places the slots on lower blocks `[5, 6, 2]`, it emits `CX` on
/// lower blocks 5 and 2.
#[derive(Clone, Debug)]
pub struct GadgetProgram {
    /// The gadget's id in its layer.
    pub id: GadgetId,
    /// The gadget's name, for messages.
    pub name: String,
    /// The length of the gadget's record: the outcomes and flags of its calls.
    pub record_len: usize,
    /// Lower type per slot.
    pub slot_types: Vec<usize>,
    /// Per `in` operand: the slots of its encoding, in code-qubit order.
    pub inputs: Vec<Vec<usize>>,
    /// Per `out` operand: its encoding.
    pub outputs: Vec<OutputEncoding>,
    /// Per slot: whether an output encoding holds it.
    pub retained: Vec<bool>,
    /// Per slot: whether the body itself consumes it: its last call takes it as
    /// an input but does not output it, as a destructive measurement does. It
    /// is then already gone below, so the encoder must not release it again.
    pub consumed: Vec<bool>,
    /// The body's calls, in order.
    pub calls: Vec<GadgetCall>,
}

impl GadgetProgram {
    /// The program of `gadget`, of `layer`. Its slots are the gadget's label
    /// ids.
    pub fn plan(gadget: &GadgetModel, layer: &LayerModel) -> Result<Self, String> {
        let lower = &layer.lower;
        let n = gadget.labels.len();
        let mut retained = vec![false; n];
        for &label in gadget.outputs.iter().flat_map(|e| &e.labels) {
            retained[label] = true;
        }
        // Label -> (an input of its last call, an output of its last call).
        let mut last_use = vec![None; n];
        let mut calls = Vec::with_capacity(gadget.body.len());
        for call in &gadget.body {
            let instruction = &lower.instructions[call.instruction];
            let (ins, outs) =
                instruction.bind(&lower.types, &call.operands, &FxHashMap::default())?;
            for &label in &call.operands {
                last_use[label] = Some((
                    ins.iter().any(|b| b.label == label),
                    outs.iter().any(|b| b.label == label),
                ));
            }
            calls.push(GadgetCall {
                instruction: call.instruction,
                operands: call.operands.clone(),
                args: call.args.clone(),
                outcomes: instruction.outcomes,
                flags: instruction.flags,
            });
        }
        Ok(Self {
            id: gadget.id,
            name: gadget.name.clone(),
            record_len: gadget.record_len,
            slot_types: gadget.label_types.clone(),
            inputs: gadget.inputs.iter().map(|e| e.labels.clone()).collect(),
            outputs: gadget
                .outputs
                .iter()
                .map(|e| OutputEncoding {
                    block_type: e.block_type,
                    slots: e.labels.clone(),
                    qubits: e
                        .labels
                        .iter()
                        .flat_map(|&label| {
                            let cap = lower.types.caps[gadget.label_types[label]];
                            (0..cap).map(move |i| (label, i))
                        })
                        .collect(),
                })
                .collect(),
            retained,
            consumed: last_use
                .iter()
                .map(|u| matches!(u, Some((true, false))))
                .collect(),
            calls,
        })
    }
}

/// Where an output block of a gadget lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputEncoding {
    /// The upper block type of the output.
    pub block_type: usize,
    /// The slots of its encoding, in code-qubit order.
    pub slots: Vec<usize>,
    /// Its code qubits, in code order: (slot, logical index in that slot's
    /// lower block). For example, two slots `[3, 4]` of a lower type that
    /// encodes 2 qubits give `[(3, 0), (3, 1), (4, 0), (4, 1)]`.
    pub qubits: Vec<(usize, usize)>,
}

/// A call of a gadget body, ready to emit.
#[derive(Clone, Debug, PartialEq)]
pub struct GadgetCall {
    /// The lower instruction.
    pub instruction: usize,
    /// The operands, as slots.
    pub operands: Vec<usize>,
    /// The arguments, in the lower instruction's parameter order. `None` if
    /// they can never be valid, so running the call fails with
    /// `InvalidInstruction`.
    pub args: Option<Vec<Arg>>,
    /// The call's observe outcomes.
    pub outcomes: usize,
    /// The call's flags.
    pub flags: usize,
}

/// How an encoder applies a correction with calls of its lower ISA.
///
/// It is part of an encoder's plan, which every shot shares, on any thread:
/// hence `Send + Sync`.
pub trait Fixes: Send + Sync {
    /// The lower calls that apply `step`, on the step's own qubits:
    /// qubit `i` of `step` is `qubits[i]`, a (lower block id, logical index in
    /// that block) pair, and `lower_type` gives a live lower block's type.
    /// Each call comes with the `select` hint its flags are checked against.
    fn resolve(
        &self,
        step: &Step,
        qubits: &[(u64, usize)],
        lower_type: &dyn Fn(u64) -> usize,
    ) -> Result<Vec<(Call, CompiledSelect)>, Fault>;
}

/// A [`Fix`] an encoder can be planned for: the lower ISA must be able to
/// apply this kind of correction.
pub trait PlannedFix: Fix {
    /// Checks that `lower`, with `resolver`, can apply this kind of
    /// correction.
    fn check(resolver: &Resolver, lower: &Isa) -> Result<(), PlanError>;
}

impl PlannedFix for Never {
    fn check(_resolver: &Resolver, _lower: &Isa) -> Result<(), PlanError> {
        Ok(())
    }
}

/// Decoders usually correct with Paulis, so the lower ISA must have `X` and
/// `Z` (see [`check_pauli_corrections`]). Other steps are checked when they are
/// applied.
impl PlannedFix for Correction {
    fn check(resolver: &Resolver, lower: &Isa) -> Result<(), PlanError> {
        check_pauli_corrections(resolver, lower)
    }
}

/// The blueprint of a [`GadgetEncoder`] for one layer, which accepts
/// corrections of type `C`.
pub struct GadgetEncoderPlan<C = Never> {
    program: Arc<EncoderProgram>,
    fix: PhantomData<fn(C)>,
}

impl<C> GadgetEncoderPlan<C> {
    /// The blueprint of encoders that run `program`.
    #[must_use]
    pub fn new(program: Arc<EncoderProgram>) -> Self {
        Self {
            program,
            fix: PhantomData,
        }
    }

    /// The program the encoders run.
    #[must_use]
    pub fn program(&self) -> &Arc<EncoderProgram> {
        &self.program
    }

    /// Applies corrections with `fixes` instead of the lower ISA's resolver.
    #[must_use]
    pub fn with_fixes(mut self, fixes: Arc<dyn Fixes>) -> Self {
        Arc::make_mut(&mut self.program).fixes = fixes;
        self
    }

    /// Reads corrections' `select` hints with `policy` instead.
    #[must_use]
    pub fn with_select_policy(mut self, policy: SelectPolicy) -> Self {
        Arc::make_mut(&mut self.program).select_policy = policy;
        self
    }
}

impl<C: PlannedFix> GadgetEncoderPlan<C> {
    /// Plans the encoders of `layer`. Fails if the lower ISA cannot apply the
    /// corrections they take.
    pub fn plan(layer: &LayerModel) -> Result<Self, PlanError> {
        let resolver = Arc::new(Resolver::new(&layer.lower));
        let program = EncoderProgram::plan(layer, &resolver)?;
        C::check(&resolver, &layer.lower)?;
        Ok(Self::new(Arc::new(program)))
    }
}

impl<C: Fix> Blueprint for GadgetEncoderPlan<C> {
    type Instance = GadgetEncoder<C>;

    fn instantiate(&self) -> GadgetEncoder<C> {
        GadgetEncoder::new(Arc::clone(&self.program))
    }
}

/// Lowers each upper call with the layer's gadget for it, or as a logical Pauli
/// or a release when the layer has no gadget (see [`Lowering`]).
///
/// ```text
///   │ Vec<Lower<C>>  upper calls and corrections  ▲ Result<Vec<Record>, Fault>
///  ┌▼─────────────────────────────────────────────┴───────────────────────────┐
///  │ GadgetEncoder<C>                                                         │
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
pub struct GadgetEncoder<C = Never> {
    program: Arc<EncoderProgram>,

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
    /// Its upper block type.
    pub block_type: usize,
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

impl<C> GadgetEncoder<C> {
    /// A fresh encoder for one shot, which runs `program`.
    #[must_use]
    pub fn new(program: Arc<EncoderProgram>) -> Self {
        Self {
            program,
            blocks: FxHashMap::default(),
            free: BinaryHeap::new(),
            type_of: Vec::new(),
            generation: FxHashMap::default(),
            next_invocation: 0,
            absorbed: Status::default(),
            no_results: false,
            fix: PhantomData,
        }
    }

    /// The program it runs.
    #[must_use]
    pub fn program(&self) -> &Arc<EncoderProgram> {
        &self.program
    }

    /// The live upper block with this label, if any.
    #[must_use]
    pub fn live_block(&self, label: u64) -> Option<&LiveBlock> {
        self.blocks.get(&label)
    }
}

impl<C: Fix> Layer for GadgetEncoder<C> {
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

impl<C: Fix> Encoder<C> for GadgetEncoder<C> {
    fn expect_no_results(&mut self) {
        self.no_results = true;
    }
}

impl<C: Fix> GadgetEncoder<C> {
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
                    let selection = accepts(&select, &readout.flags);
                    self.absorbed |= readout.status | (self.program.select_policy)(selection);
                }
            }
        }
        Ok(())
    }

    // ── Parsing an upper call (read-only) ───────────────────────────────────

    fn parse(&self, call: &Call) -> Result<ParsedCall, Fault> {
        self.program
            .upper
            .parse_call(call.instruction_id, &call.parameters)
            .map_err(Fault::invalid)
    }

    fn input_types(&self, operands: &[u64]) -> FxHashMap<u64, usize> {
        operands
            .iter()
            .filter_map(|l| self.blocks.get(l).map(|b| (*l, b.block_type)))
            .collect()
    }

    // ── Lowering one upper call ─────────────────────────────────────────────

    fn lower_call(&mut self, call: &Call, b: &mut Batch<'_, '_>) -> Result<(), Fault> {
        let parsed = self.parse(call)?;
        let program = Arc::clone(&self.program);
        match &program.instructions[parsed.index] {
            Lowering::Gadget(gp) => self.start_gadget(gp, parsed, b),
            Lowering::LogicalPauli(paulis) => {
                self.apply_logical_pauli(&parsed, paulis, b)?;
                b.open_record(parsed.index, 0);
                Ok(())
            }
            Lowering::Release => {
                let [label] = parsed.operands.as_slice() else {
                    return Err(Fault::invalid("a release takes one block"));
                };
                let released = self.release(*label, b);
                let record = b.open_record(parsed.index, 0);
                b.records[record].released.extend(released);
                Ok(())
            }
            Lowering::Unsupported => Err(Fault::no_implementation(format!(
                "no gadget implements `{}`",
                program.upper.instructions[parsed.index].mnemonic
            ))),
        }
    }

    /// Binds a gadget's operands and places its inputs, without changing any
    /// state, then runs it.
    fn start_gadget(
        &mut self,
        gp: &GadgetProgram,
        parsed: ParsedCall,
        b: &mut Batch<'_, '_>,
    ) -> Result<(), Fault> {
        // Live types constrain only the inputs (see `Signature::bind`), so an
        // instruction may change a block's type.
        let input_types = self.input_types(&parsed.operands);
        let upper = &self.program.upper;
        let (in_ops, out_ops) = upper.instructions[parsed.index]
            .bind(&upper.types, &parsed.operands, &input_types)
            .map_err(Fault::invalid)?;
        if in_ops.len() != gp.inputs.len() || out_ops.len() != gp.outputs.len() {
            return Err(Fault::invalid(
                "operands do not match the gadget's encodings",
            ));
        }

        let mut placement = vec![UNPLACED; gp.slot_types.len()];
        for (label, slots) in parsed.operands.iter().zip(&gp.inputs) {
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
                    || self.type_of[ix(lb)] != Some(gp.slot_types[slot])
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
        gp: &GadgetProgram,
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
        let program = Arc::clone(&self.program);
        let output_refs: Vec<BlockRef> = out_ops
            .iter()
            .map(|o| self.fresh_ref(o.label, &program.upper.types.names[o.block_type]))
            .collect();

        for (slot, lb) in placement.iter_mut().enumerate() {
            if *lb == UNPLACED {
                *lb = self.allocate_lower(gp.slot_types[slot]);
            }
        }

        let id = self.next_invocation;
        self.next_invocation += 1;
        let record = b.open_record(parsed.index, gp.record_len);
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
        gp: &GadgetProgram,
        record: usize,
        placement: &[u64],
        b: &mut Batch<'_, '_>,
    ) -> Result<(), Fault> {
        let mut offset = 0;
        for call in &gp.calls {
            let args = call.args.as_ref().ok_or_else(|| {
                Fault::invalid(format!(
                    "gadget `{}` passes invalid arguments to `{}`",
                    gp.name, self.program.lower.instructions[call.instruction].mnemonic
                ))
            })?;
            let mut params: SmallVec<[OwnedParam; 2]> =
                SmallVec::with_capacity(call.operands.len() + args.len());
            params.extend(
                call.operands
                    .iter()
                    .map(|&slot| OwnedParam::Block(placement[slot])),
            );
            for arg in args {
                params.push(match arg {
                    Arg::Value(value) => value.clone(),
                    Arg::Forwarded { name, kind } => {
                        let inv = b.records[record]
                            .invocation
                            .as_ref()
                            .expect("a gadget record has an invocation");
                        inv.args
                            .get(name)
                            .filter(|v| kind.matches(v))
                            .ok_or_else(|| {
                                Fault::invalid(format!("argument `{name}` does not fit its use"))
                            })?
                            .clone()
                    }
                    Arg::Readout(j) => {
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
                instruction_id: call.instruction,
                parameters: params,
            });
            let count = call.outcomes + call.flags;
            if count > 0 && !self.no_results {
                b.expected.push(Expected::Bits {
                    record,
                    offset,
                    outcomes: call.outcomes,
                    flags: call.flags,
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
        gp: &GadgetProgram,
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
        for (block_ref, output) in inv.output_refs.iter().zip(&gp.outputs) {
            let support: Vec<u64> = output.slots.iter().map(|&s| placement[s]).collect();
            let qubits = output
                .qubits
                .iter()
                .map(|&(slot, i)| (placement[slot], i))
                .collect();
            self.blocks.insert(
                block_ref.label,
                LiveBlock {
                    block_ref: block_ref.clone(),
                    block_type: output.block_type,
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
            .filter(|&(slot, _)| !gp.retained[slot])
            .map(|(slot, &lb)| (lb, slot))
            .collect();
        leftovers.sort_unstable();
        for (lb, slot) in leftovers {
            if gp.consumed[slot] {
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
        paulis: &[ParsedPauli],
        b: &mut Batch<'_, '_>,
    ) -> Result<(), Fault> {
        let program = Arc::clone(&self.program);
        let instruction = &program.upper.instructions[call.index];
        // Signs are dropped: the spec defines Pauli steps only up to global
        // phase (https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#actions).
        let mut product = Pauli::default();
        for pauli in paulis {
            let p = pauli
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|e| e.resolve(&call.args))
                .map_err(Fault::invalid)?;
            product = product.mul_unsigned(&p);
        }
        let input_types = self.input_types(&call.operands);
        let (ins, _) = instruction
            .bind(&program.upper.types, &call.operands, &input_types)
            .map_err(Fault::invalid)?;
        for bound in ins {
            let block_ref = self
                .blocks
                .get(&bound.label)
                .ok_or_else(|| Fault::invalid(format!("block {} is not live", bound.label)))?
                .block_ref
                .clone();
            let code = program.codes[bound.block_type].as_ref().ok_or_else(|| {
                Fault::no_implementation(format!(
                    "no code for `{}`",
                    program.upper.types.names[bound.block_type]
                ))
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

    /// Lowers one correction through the program's [`Fixes`].
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

        let program = Arc::clone(&self.program);
        let type_of = &self.type_of;
        let parts = program
            .fixes
            .resolve(&local, &slots, &|lb| type_of[ix(lb)].unwrap_or_default())?;
        for (call, select) in parts {
            let instruction = program
                .lower
                .instructions
                .get(call.instruction_id)
                .ok_or_else(|| Fault::protocol("a correction with an unknown lower instruction"))?;
            let (outcomes, flags) = (instruction.outcomes, instruction.flags);
            b.buffer.push(call);
            if outcomes + flags > 0 && !self.no_results {
                b.expected.push(Expected::Absorb {
                    outcomes,
                    flags,
                    select,
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
        if let Some(index) = self.program.lower_release[t] {
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
