// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Seeing inside a running qodec without changing it.
//!
//! [`Traced`] wraps any [`Design`] and gives every layer it assembles a tap:
//! a layer around the layer that writes each batch it is given, and each
//! answer it returns, in a [`Journal`]. A traced design is a design, so the
//! runtime runs it as it runs any other, and knows nothing of the taps.
//!
//! ```text
//!   │ Vec<Call>          ▲ Result<Vec<Reply>, Fault>
//!  ┌▼────────────────────┴┐
//!  │ Tap   open ... close ├──▶ Journal
//!  │ ┌──────────────────┐ │
//!  │ │ the layer        │ │
//!  │ └──────────────────┘ │
//!  └┬────────────────────▲┘
//!   ▼ the next tap, ...  │
//!   ▼ the Bench          ├──▶ Journal (the bottom batches, with their bits)
//! ```
//!
//! Layers answer by calling the layer below, so the batches nest: each batch
//! a layer sends below opens while the batch that caused it is still open. The
//! journal keeps them as a tree, and a batch's depth in it is its boundary:
//! 0 for the top ISA, and one more per layer down to the bottom ISA.

use qodec_runtime_recipes::{
    Backend, BasicReadout, Blueprint, BoxedLayer, Call, Design, Fault, Isa, Layer, LayerBlueprint,
    LayerModel, PlanError, Reply, Status,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
};

/// Where a bottom bit comes from: the `k`-th bit the bottom backend made up
/// while running the line with stable id `line`. Lines keep their ids when
/// other lines are added, removed or moved, so a bit keeps its identity, and
/// its pin, across edits elsewhere in the program.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BitId {
    /// The stable id of the program line.
    pub line: u64,
    /// The bit's position among the line's bottom bits.
    pub k: u32,
}

/// How the bottom backend makes up bits nobody pinned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Every bit is 0: the noiseless outcome of most syndrome measurements.
    Zero,
    /// Every bit is 1.
    One,
    /// Pseudo-random bits, a function of the seed and the bit's [`BitId`].
    Random(u64),
}

impl std::fmt::Display for Policy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Policy::Zero => write!(f, "zero"),
            Policy::One => write!(f, "one"),
            Policy::Random(seed) => write!(f, "random {seed}"),
        }
    }
}

/// The bits the bottom backend returns: a [`Policy`], overridden by pins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcomes {
    /// How unpinned bits are made up.
    pub policy: Policy,
    /// Bits set by hand.
    pub pins: BTreeMap<BitId, bool>,
}

impl Default for Outcomes {
    fn default() -> Self {
        Self {
            policy: Policy::Zero,
            pins: BTreeMap::new(),
        }
    }
}

impl Outcomes {
    /// The value of bit `id`, and whether it is pinned.
    #[must_use]
    pub fn bit(&self, id: BitId) -> (bool, bool) {
        if let Some(&pinned) = self.pins.get(&id) {
            return (pinned, true);
        }
        let value = match self.policy {
            Policy::Zero => false,
            Policy::One => true,
            Policy::Random(seed) => {
                splitmix(seed ^ splitmix(id.line) ^ splitmix(u64::from(id.k) << 32)) & 1 == 1
            }
        };
        (value, false)
    }
}

/// A well-mixed hash of `x` (`SplitMix64`'s finalizer).
fn splitmix(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// One batch at one boundary.
#[derive(Clone, Debug)]
pub struct Node {
    /// The boundary: 0 for the top ISA, the number of layers for the bottom.
    pub boundary: usize,
    /// The index of the program line that caused it.
    pub line: usize,
    /// The batch whose handling sent this one, one boundary up.
    pub parent: Option<usize>,
    /// The batches sent below while handling this one, in order.
    pub children: Vec<usize>,
    /// The calls, with their results once the batch is answered.
    pub calls: Vec<TracedCall>,
    /// Why the batch failed, if it did. The shot is then lost.
    pub fault: Option<String>,
}

/// A call of a batch, and what it returned.
#[derive(Clone, Debug)]
pub struct TracedCall {
    /// The call.
    pub call: Call,
    /// Its result, if its instruction reports one and the batch was answered.
    pub readout: Option<BasicReadout>,
    /// For a bottom call, where each bit of the result came from, in result
    /// order (outcomes, then flags), and whether it was pinned.
    pub bits: Vec<(BitId, bool)>,
}

/// The tree of batches of a shot, as the taps and the bench write it.
#[derive(Debug, Default)]
pub struct Journal {
    nodes: Vec<Node>,
    /// The batches being answered, outermost first.
    open: Vec<usize>,
    /// The line being run: its index and stable id.
    line: (usize, u64),
    /// The next bit's position in the line.
    next_bit: u32,
}

impl Journal {
    /// Forgets everything, for a new shot.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Starts recording the batches of line `index`, whose stable id is `id`.
    pub fn begin_line(&mut self, index: usize, id: u64) {
        self.open.clear();
        self.line = (index, id);
        self.next_bit = 0;
    }

    /// The batches recorded so far.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Takes the batches recorded so far.
    pub fn take(&mut self) -> Vec<Node> {
        std::mem::take(&mut self.nodes)
    }

    /// Records a batch arriving at the next boundary down, and returns its id.
    fn open(&mut self, calls: &[Call]) -> usize {
        let id = self.nodes.len();
        let parent = self.open.last().copied();
        if let Some(parent) = parent {
            self.nodes[parent].children.push(id);
        }
        self.nodes.push(Node {
            boundary: self.open.len(),
            line: self.line.0,
            parent,
            children: Vec::new(),
            calls: calls
                .iter()
                .map(|call| TracedCall {
                    call: call.clone(),
                    readout: None,
                    bits: Vec::new(),
                })
                .collect(),
            fault: None,
        });
        self.open.push(id);
        id
    }

    /// Records the answer of batch `id`: one result per call that has one, as
    /// `isa` shapes them, or why it failed.
    fn close(&mut self, id: usize, isa: &Isa, answer: Result<Vec<BasicReadout>, String>) {
        if let Some(k) = self.open.iter().rposition(|&o| o == id) {
            self.open.truncate(k);
        }
        let node = &mut self.nodes[id];
        match answer {
            Ok(results) => {
                let mut results = results.into_iter();
                for traced in &mut node.calls {
                    if isa.shape(traced.call.instruction_id).is_some() {
                        traced.readout = results.next();
                    }
                }
            }
            Err(e) => node.fault = Some(e),
        }
    }

    /// The id of the next bit of the line being run.
    fn next_bit(&mut self) -> BitId {
        let id = BitId {
            line: self.line.1,
            k: self.next_bit,
        };
        self.next_bit += 1;
        id
    }
}

/// A journal shared by the taps of a plan and the bench of each shot.
pub type SharedJournal = Arc<Mutex<Journal>>;

/// Locks `journal`. A panic while it was locked leaves it usable: it is only
/// a record.
pub fn lock(journal: &SharedJournal) -> MutexGuard<'_, Journal> {
    journal
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `inner`, with every layer it assembles tapped into `journal`.
pub struct Traced<D> {
    /// The design that assembles the layers.
    pub inner: D,
    /// Where the taps write.
    pub journal: SharedJournal,
}

impl<D: Design> Design for Traced<D> {
    fn layer(&self, layer: &Arc<LayerModel>) -> Result<Box<dyn LayerBlueprint>, PlanError> {
        Ok(Box::new(TapPlan {
            inner: self.inner.layer(layer)?,
            upper: Arc::clone(&layer.upper),
            journal: Arc::clone(&self.journal),
        }))
    }
}

/// The blueprint of a [`Tap`].
struct TapPlan {
    inner: Box<dyn LayerBlueprint>,
    upper: Arc<Isa>,
    journal: SharedJournal,
}

impl Blueprint for TapPlan {
    type Instance = Tap;

    fn instantiate(&self) -> Tap {
        Tap {
            inner: self.inner.instantiate_layer(),
            upper: Arc::clone(&self.upper),
            journal: Arc::clone(&self.journal),
        }
    }
}

/// A layer around a layer, which journals the batches the layer is given and
/// its answers, and changes nothing.
struct Tap {
    inner: BoxedLayer,
    /// The ISA the layer is called in, to match its results with its calls.
    upper: Arc<Isa>,
    journal: SharedJournal,
}

impl Layer for Tap {
    type Input = Vec<Call>;
    type Output = Result<Vec<Reply>, Fault>;
    type LowerInput = Vec<Call>;
    type LowerOutput = Result<Vec<Reply>, Fault>;

    fn execute(
        &mut self,
        calls: Vec<Call>,
        below: &mut dyn Backend<Vec<Call>, Result<Vec<Reply>, Fault>>,
    ) -> Result<Vec<Reply>, Fault> {
        // The journal is not locked while the layer runs: the taps below
        // write in it too.
        let id = lock(&self.journal).open(&calls);
        let result = self.inner.execute(calls, below);
        let answer = match &result {
            Ok(replies) => Ok(replies.iter().cloned().map(Reply::into_ready).collect()),
            Err(fault) => Err(fault.to_string()),
        };
        lock(&self.journal).close(id, &self.upper, answer);
        result
    }
}

/// The bottom backend: it runs nothing, and answers each call that reports
/// bits with bits from [`Outcomes`], journaling the batch and where each bit
/// came from.
pub struct Bench<'a> {
    /// The bottom ISA.
    pub isa: Arc<Isa>,
    /// Where it writes.
    pub journal: SharedJournal,
    /// Where its bits come from.
    pub outcomes: &'a Outcomes,
}

impl Backend<Vec<Call>, Result<Vec<BasicReadout>, String>> for Bench<'_> {
    fn execute(&mut self, calls: Vec<Call>) -> Result<Vec<BasicReadout>, String> {
        let mut journal = lock(&self.journal);
        let id = journal.open(&calls);
        let mut results = Vec::new();
        for (j, call) in calls.iter().enumerate() {
            let Some(instruction) = self.isa.instructions.get(call.instruction_id) else {
                let e = format!("unknown bottom instruction id {}", call.instruction_id);
                journal.close(id, &self.isa, Err(e.clone()));
                return Err(e);
            };
            if !instruction.has_result() {
                continue;
            }
            let mut bits = Vec::with_capacity(instruction.outcomes + instruction.flags);
            let mut values = Vec::with_capacity(bits.capacity());
            for _ in 0..instruction.outcomes + instruction.flags {
                let bit = journal.next_bit();
                let (value, pinned) = self.outcomes.bit(bit);
                bits.push((bit, pinned));
                values.push(Some(value));
            }
            let (outcomes, flags) = values.split_at(instruction.outcomes);
            results.push(BasicReadout::new(outcomes, flags, Status::default()));
            journal.nodes[id].calls[j].bits = bits;
        }
        journal.close(id, &self.isa, Ok(results.clone()));
        Ok(results)
    }
}
