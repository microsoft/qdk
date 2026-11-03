// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A code's adapter, as example code: what a code provides (a [`Code`],
//! plain data, as a Qodec loader would fill it), and the three types that
//! run any such code on the protocol: [`Encoding`] (the adapter, built once
//! against what is downstream of it), [`Encoded`] (the adapter upstream of
//! the physical executor, an executor of logical calls) and [`Decoded`]
//! (each reply, logical outcomes from physical ones).
//!
//! It shows that the protocol's facilities meet the needs of a code's
//! adapter; it is not a product. What it leaves out (ancillas, parameters,
//! gadgets on several blocks, checks of the gadgets, Pauli frames,
//! feed-forward, decoders) is listed in `docs/instruction-sets.md`.

use std::collections::BTreeMap;
use std::fmt;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::task::Waker;

use executor_protocol::{
    Adapter, AdapterBoundary, Apply, Close, Executor, InstructionBatch, InstructionBatchRef,
    InstructionCall, InstructionSet, Opcode, OpcodeMap, OpcodeTable, Operand, Rejected, Reply,
    ReplyStatus, ShotId, ShotOutcome, Value, check_unique_tags,
};

/// A code, as Qodec gives it: the logical instruction set, the
/// physical instructions its gadgets call, and each logical instruction's
/// gadget.
///
/// Logical block `b` is physical qubits `b·block_size` to
/// `b·block_size + block_size − 1`. The gadgets are trusted, as a loader
/// checks them: each logical instruction takes one block, and one tag per
/// logical readout; each gadget calls instructions of `sends`, with the
/// operands they take, on its block's qubits, and its logical readouts read
/// circuit readouts it makes.
pub struct Code {
    /// The logical instruction set.
    pub accepts: Arc<InstructionSet>,
    /// The instructions its gadgets call, as what is downstream declares
    /// them: with their actions, so that the device's set is matched by
    /// action.
    pub sends: InstructionSet,
    /// The physical qubits of a logical block: not 0, as the device's
    /// qubits are shared out by it.
    pub block_size: NonZeroU32,
    /// Each logical instruction's gadget, by its mnemonic.
    pub gadgets: BTreeMap<String, Gadget>,
}

/// What a logical instruction becomes: physical calls, and its logical
/// readouts.
pub struct Gadget {
    /// The physical calls, in order.
    pub calls: Vec<GadgetCall>,
    /// One per tag operand of the logical instruction, in order: the circuit
    /// readouts whose parity it is.
    pub readouts: Vec<Vec<u32>>,
}

/// One physical call of a gadget: an instruction of [`Code::sends`],
/// by its mnemonic, and its operands.
pub struct GadgetCall {
    pub mnemonic: String,
    pub operands: Vec<Slot>,
}

/// An operand of a gadget's call, relative to the logical call.
#[derive(Clone, Copy)]
pub enum Slot {
    /// Qubit `n` of the logical call's block.
    Qubit(u32),
    /// The gadget's `n`-th circuit readout.
    Readout(u32),
}

/// Why a code cannot run on what is downstream of it, apart from what
/// `compose` checks itself (that what the gadgets call fits the set
/// downstream). [`Encoding`] reports every one when it is built, so that
/// whoever writes the code fixes them all at once.
#[derive(Clone, Debug, PartialEq)]
pub enum CodeFault {
    /// The logical instruction has no gadget.
    NoGadget(String),
    /// Downstream has fewer qubits than one block, or does not say how many
    /// it has.
    TooFewQubits { needed: u32, offered: Option<u32> },
}

impl fmt::Display for CodeFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoGadget(instruction) => write!(f, "`{instruction}` has no gadget"),
            Self::TooFewQubits {
                needed,
                offered: Some(offered),
            } => write!(f, "the code needs {needed} qubits, of {offered}"),
            Self::TooFewQubits {
                needed,
                offered: None,
            } => write!(f, "the code needs {needed} qubits, of an unstated number"),
        }
    }
}

impl std::error::Error for CodeFault {}

/// A code as an adapter: it accepts the logical set and sends what its
/// gadgets call, and gives the [`Encoded`] executor upstream of the
/// physical one. Built once ([`Adapter::build`]), it checks the code against
/// what is downstream, and resolves each gadget into the downstream
/// opcodes, so that a call goes down with no name looked up.
pub struct Encoding {
    code: Arc<Code>,
    /// The set its calls are numbered by: what the gadgets call, until it
    /// is built, then the set downstream, whose numbering it adopts.
    sends: Arc<InstructionSet>,
    /// The code resolved downstream, once built without a fault.
    resolved: Option<Arc<ResolvedCode>>,
}

/// What an [`Encoding`] runs: the code, its gadgets resolved in the
/// downstream numbering.
struct ResolvedCode {
    accepts: Arc<InstructionSet>,
    templates: OpcodeTable<Template>,
    block_size: u32,
    blocks: u32,
}

/// A gadget, each call by its downstream opcode.
struct Template {
    calls: Vec<(Opcode, Vec<Slot>)>,
    readouts: Vec<Vec<u32>>,
    /// How many circuit readouts the gadget makes.
    circuit_readouts: u32,
}

impl Encoding {
    /// `code`, to be composed upstream of an executor. It offers a logical
    /// qubit per `block_size` of the executor's qubits.
    pub fn new(code: Code) -> Self {
        Self {
            sends: Arc::new(code.sends.clone()),
            code: Arc::new(code),
            resolved: None,
        }
    }
}

impl Adapter for Encoding {
    type Accepts = InstructionCall;
    type Sends = InstructionCall;

    fn sends(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.sends))
    }

    fn accepts(&self) -> Option<Arc<InstructionSet>> {
        Some(Arc::clone(&self.code.accepts))
    }

    /// Checks the code against what is downstream, and reports every
    /// [`CodeFault`], in this order: each logical instruction without a
    /// gadget, in the order `accepts` declares them; too few qubits. It
    /// offers a logical qubit per block; without a fault, it resolves the
    /// gadgets in the downstream numbering, which it adopts.
    fn build(mut self, boundary: &mut AdapterBoundary<'_>) -> Self {
        let code = &*self.code;
        let mut faults: Vec<CodeFault> = (code.accepts.mnemonics())
            .filter(|instruction| !code.gadgets.contains_key(*instruction))
            .map(|instruction| CodeFault::NoGadget(instruction.to_owned()))
            .collect();
        let needed = code.block_size.get();
        let blocks = match boundary.downstream().qubits {
            Some(offered) if offered >= needed => Some(offered / needed),
            offered => {
                faults.push(CodeFault::TooFewQubits { needed, offered });
                None
            }
        };
        boundary.offered_upstream().qubits = blocks;
        let map = if faults.is_empty() {
            boundary.adopt_downstream_numbering()
        } else {
            None
        };
        for fault in faults {
            boundary.fault(fault);
        }
        let (Some(blocks), Some(map)) = (blocks, map) else {
            return self;
        };
        let templates = code
            .accepts
            .table(|instruction| Some(template(code, map, &code.gadgets[instruction])))
            .expect("each logical instruction has a gadget");
        self.resolved = Some(Arc::new(ResolvedCode {
            accepts: Arc::clone(&code.accepts),
            templates,
            block_size: needed,
            blocks,
        }));
        let downstream = boundary.downstream().instruction_set.clone();
        self.sends = downstream.expect("a set the gadgets were matched in");
        self
    }
}

impl<E: Executor<InstructionBatch<InstructionCall>> + Close> Apply<E> for Encoding {
    type Output = Encoded<E>;

    /// The code upstream of the physical executor: it draws nothing, so
    /// each shot's draws are the physical shot's.
    fn apply(self, downstream: E) -> Encoded<E> {
        let code = self.resolved;
        Encoded {
            code: code.expect("built without a fault: `compose` applies no other"),
            inner: downstream,
        }
    }
}

/// `gadget` resolved in the downstream set, where `map` matches
/// `code.sends`.
fn template(code: &Code, map: &OpcodeMap, gadget: &Gadget) -> Template {
    let opcode = |mnemonic: &str| {
        let sent = code.sends.opcode(mnemonic).expect("a gadget calls `sends`");
        map.get(sent).expect("`map` matches `sends`")
    };
    let slots = gadget.calls.iter().flat_map(|c| &c.operands);
    let ends = slots.filter_map(|slot| match *slot {
        Slot::Readout(n) => Some(n + 1),
        Slot::Qubit(_) => None,
    });
    Template {
        calls: (gadget.calls.iter())
            .map(|call| (opcode(&call.mnemonic), call.operands.clone()))
            .collect(),
        readouts: gadget.readouts.clone(),
        circuit_readouts: ends.max().unwrap_or(0),
    }
}

/// A code upstream of the physical executor: an executor of logical
/// calls, each batch's gadgets sent down on the batch's shots.
pub struct Encoded<E> {
    code: Arc<ResolvedCode>,
    inner: E,
}

impl<E: Executor<InstructionBatch<InstructionCall>>> Executor<InstructionBatch<InstructionCall>>
    for Encoded<E>
{
    type Reply = Decoded<E::Reply>;

    /// Sends the gadgets of `batch` down as one physical batch, with fresh
    /// tags for their circuit readouts, and returns its reply, decoded.
    ///
    /// # Errors
    ///
    /// [`Rejected::Instruction`] at a call outside the logical set, on a
    /// block the code lacks, or repeating the tag of a readout before it
    /// ([`check_unique_tags`]), before anything goes down; the downstream
    /// rejection otherwise, at the logical call whose gadget holds the
    /// physical call refused.
    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        let (code, calls) = (&*self.code, batch.instructions);
        code.accepts.check_batch(calls)?;
        if let Some((at, b)) =
            (calls.iter().map(block).enumerate()).find(|&(_, b)| b >= code.blocks)
        {
            let reason = format!("block {b}, of {} blocks", code.blocks);
            return Err(Rejected::Instruction { at, reason });
        }
        check_unique_tags(
            calls
                .iter()
                .enumerate()
                .flat_map(|(at, call)| tags(call).map(move |tag| (at, tag))),
        )?;
        let (mut physical, mut origin) = (Vec::new(), Vec::new());
        let mut expected = Expected::default();
        for (at, call) in calls.iter().enumerate() {
            let template = &code.templates[call.opcode];
            let first_qubit = block(call) * code.block_size;
            let first = u32::try_from(expected.feeds.len()).expect("fewer than 2^32 readouts");
            let operand = |slot: &Slot| match *slot {
                Slot::Qubit(n) => Operand::Block(first_qubit + n),
                Slot::Readout(n) => Operand::Tag(first + n),
            };
            let calls = template.calls.iter();
            physical.extend(
                calls.map(|(op, slots)| InstructionCall::new(*op, slots.iter().map(operand))),
            );
            origin.resize(physical.len(), at);
            expected.add(first, template, call);
        }
        let downstream = self.inner.call(InstructionBatchRef {
            shots: batch.shots,
            instructions: &physical,
        });
        let logical = &expected.logical;
        Ok(Decoded {
            inner: downstream.map_err(|rejected| rejected.at_origin(|at| origin[at]))?,
            circuit: Vec::new(),
            logical: (batch.shots.iter())
                .map(|&j| (j, logical.clone()))
                .collect(),
            feeds: expected.feeds,
        })
    }
}

/// The block `call` names.
fn block(call: &InstructionCall) -> u32 {
    let block = call.operands.iter().find_map(|operand| match *operand {
        Operand::Block(b) => Some(b),
        _ => None,
    });
    block.expect("each logical instruction takes a block")
}

/// The tags `call` names, in order: one per logical readout.
fn tags(call: &InstructionCall) -> impl Iterator<Item = u32> + '_ {
    call.operands.iter().filter_map(|operand| match *operand {
        Operand::Tag(t) => Some(t),
        _ => None,
    })
}

impl<E: Close> Close for Encoded<E> {
    type Report = E::Report;

    fn close(&mut self) -> E::Report {
        self.inner.close()
    }
}

/// The reply of an [`Encoded`]: each shot's logical outcome when the last
/// circuit readout of its parity on that shot arrives. A circuit readout
/// that no logical readout reads goes nowhere.
///
/// Known issue, example quality: a circuit readout under a tag the batch
/// did not assign, or on a shot not in the batch, is ignored, and one that
/// comes up twice before its logical readout is complete counts twice
/// (after, it is ignored), so a faulty device downstream goes unnoticed; a
/// product would fail the reply.
pub struct Decoded<R> {
    inner: R,
    /// The circuit readouts that came up, before they are decoded.
    circuit: Vec<ShotOutcome>,
    /// By physical tag, the logical readouts it is a bit of.
    feeds: Vec<Vec<usize>>,
    /// Each shot's logical readouts, as they fill.
    logical: BTreeMap<ShotId, Vec<Parity>>,
}

/// The logical readouts of a batch, before any comes up.
#[derive(Default)]
struct Expected {
    /// By physical tag, the logical readouts it is a bit of.
    feeds: Vec<Vec<usize>>,
    logical: Vec<Parity>,
}

/// A logical readout on its way up.
#[derive(Clone)]
struct Parity {
    tag: u32,
    value: Value,
    missing: usize,
}

impl Expected {
    /// Expects the readouts of `call`'s `template`: its circuit readouts at
    /// physical tags from `first`, and its logical readouts under `call`'s
    /// tags.
    fn add(&mut self, first: u32, template: &Template, call: &InstructionCall) {
        let end = first as usize + template.circuit_readouts as usize;
        self.feeds.resize(end, Vec::new());
        for (bits, tag) in template.readouts.iter().zip(tags(call)) {
            for &bit in bits {
                self.feeds[(first + bit) as usize].push(self.logical.len());
            }
            self.logical.push(Parity {
                tag,
                value: Value::Zero,
                missing: bits.len(),
            });
        }
    }
}

impl<R: Reply<Item = ShotOutcome>> Reply for Decoded<R> {
    type Item = ShotOutcome;

    fn poll_events(&mut self, out: &mut Vec<ShotOutcome>, waker: &Waker) -> ReplyStatus {
        let status = self.inner.poll_events(&mut self.circuit, waker);
        let (feeds, logical) = (&self.feeds, &mut self.logical);
        for ShotOutcome {
            shot, tag, value, ..
        } in self.circuit.drain(..)
        {
            let Some(parities) = logical.get_mut(&shot) else {
                continue;
            };
            for &readout in feeds.get(tag as usize).into_iter().flatten() {
                let parity = &mut parities[readout];
                if parity.missing == 0 {
                    continue;
                }
                parity.value = xor(parity.value, value);
                parity.missing -= 1;
                if parity.missing == 0 {
                    out.push(ShotOutcome::new(shot, parity.tag, parity.value));
                }
            }
        }
        match status {
            ReplyStatus::Done if logical.values().flatten().any(|parity| parity.missing > 0) => {
                ReplyStatus::Failed("a logical readout is missing circuit readouts".into())
            }
            status => status,
        }
    }
}

/// The parity of two readouts; when either has no value, the parity has
/// none.
fn xor(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Missing(failure), _) | (_, Value::Missing(failure)) => Value::Missing(failure),
        (a, b) => ((a == Value::One) != (b == Value::One)).into(),
    }
}
