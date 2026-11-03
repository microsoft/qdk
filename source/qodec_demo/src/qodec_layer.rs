//! One Qodec layer (the Qodec runtime's `LayerRuntime`, unchanged) as an
//! adapter of the protocol, made by `owning_adapter_fn` with a state per
//! shot: each shot's state is the layer's runtime, made over the way below
//! that the state owns ([`Lower`]).
//!
//! The runtime calls its backend and waits for the results inside its own
//! call (mode (c) of QD1; the README lists the open questions): so does
//! [`Lower`], through `BelowHandle::run`, which parks the thread until the
//! reply below ends. A call on several shots is served shot by shot, and
//! goes below by the adapter facility's rule for a state per shot
//! (`Below::send`, `Below::run`): a batch that reads goes below on its
//! shot alone, after the batches the shot sent before it in the call, each
//! alone too; a batch sent after the shot's last read in the call merges
//! with the other shots' equal sends into one call. So an upper call whose
//! lowering reads nothing goes below on its shots at once
//! (`tests/calls_below.rs`, `tests/answers_later.rs`). Every bottom batch
//! of c4c6 reads (69 a shot), so below c4c6 every one goes alone, whatever
//! the executor below can take. No program of the demo (repetition3,
//! c422, c4c6) sends a batch before or after a read in one upper call, so
//! the demo does not exercise those two branches: the protocol's tests pin them
//! (`a_shots_run_leaves_the_other_shots_sends_merged`,
//! `a_shots_sends_after_its_run_merge_with_the_others`).
//! Merging a shot's batches that read with the other shots' needs a
//! lockstep per-shot facility whose shots' logic can stop at a read: async,
//! resumable, or a thread per shot (open, Q67).
//!
//! The layer's plans (`Rc`) and each shot's runtime are not `Send`, so the
//! layer's adapter is not, and a chain with a Qodec layer cannot move to
//! another thread: it is built on the thread that runs it
//! (`a_qodec_chain_built_and_run_on_a_thread_of_its_own_gives_the_same_run`).
//!
//! ```compile_fail,E0277
//! fn send<T: Send>(_: T) {}
//! fn adapter_is_send(layer: qodec_demo::qodec_layer::QodecLayer) {
//!     send(layer.adapter());
//! }
//! ```
//!
//! Markers, for the facility candidates of the Qodec prototype's findings,
//! named as there, but for these differences:
//! - the prototype's `[PLUMBING:errors]` marked its mapping of the
//!   runtime's errors to a refusal or a failure, which a logic cannot make
//!   (it can only fail a call: a `FRICTION` below); here it marks
//!   [`LowerError`];
//! - its `[PLUMBING:down]`, the batch as calls in the numbering below, is
//!   `[GLUE]` here: the facility renumbers (`[FACILITY:numbering]`), so
//!   what is left is the runtime's types to the protocol's;
//! - its check of one shot a call is gone: a state per shot serves one
//!   shot by construction, and a state serving two fails the call
//!   (`a_layer_of_one_state_fails_a_call_on_two_shots`).
//!
//! The markers:
//! - `[QODEC]`: the Qodec runtime's own logic (lowering, decoding, its
//!   state);
//! - `[GLUE]`: the Qodec runtime's types to the protocol's, also its own
//!   (see `boundary.rs`);
//! - `[PLUMBING:<name>]`: what any adapter would need, and the protocol
//!   does not give;
//! - `[FACILITY:<name>]`: what the prototype wrote as `[PLUMBING:<name>]`
//!   and the protocol's adapter facility now gives; the code there is this
//!   layer's use of it.
//!
//! The rest is unmarked: this layer's policy on the blocks below
//! ([`QodecLayer::within`], QD4), and the assembly of the adapter from its
//! parts.

use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use executor_protocol::{
    AdapterError, AdapterFn, BelowHandle, CallInput, Fold, InstructionCall, OnePerShot,
    OnePerShotAdapter, Operand, Outcome, Outcomes, Owning, OwningAdapterLogic, Rejected,
    ShotContext, ShotId, ShotOutcome, check_unique_tags, owning_adapter_fn,
};
use qodec::Qodec;
use qodec_runtime::{
    Backend, BasicReadout, BatchResult, Bit, Decoder, DecoderPlan, FrameDecoder, LayerPlan,
    LayerRuntime, OwnedParam, Readout, Resources, Status,
};
use smallvec::SmallVec;

use crate::boundary::{Boundary, Ins, bit, opcode, value};

/// Layer `k` of a qodec, ready to be made an adapter
/// ([`QodecLayer::adapter`]).
pub struct QodecLayer {
    // [QODEC] built once: the layer's plan and its decoder's.
    plan: Rc<LayerPlan>,
    decoder: Rc<dyn DecoderPlan<BasicReadout>>,
    // [GLUE] its upper and lower ISAs, as the protocol's sets.
    accepts: Arc<Boundary>,
    sends: Arc<Boundary>,
    /// The upper blocks it can run, when built within a bound of lower
    /// qubits ([`QodecLayer::within`]).
    blocks: Option<u32>,
}

impl QodecLayer {
    /// Layer `k` of `qodec`, with no bound on the blocks it is called on.
    ///
    /// # Errors
    ///
    /// When `k` is the bottom layer or beyond, which has nothing below it
    /// (`the_bottom_layer_is_refused_since_nothing_is_below_it`,
    /// `a_layer_beyond_the_bottom_is_refused`, up to `usize::MAX`:
    /// `the_last_index_is_refused_as_beyond_the_bottom`). Otherwise, with
    /// every fault the build finds, in this order, joined by `; `:
    /// - the Qodec runtime's error when it cannot plan the layer, prefixed
    ///   `layer {k}:` (`a_layer_the_runtime_cannot_plan_is_refused_naming_it`),
    ///   or, once it can, the decoder's error when the decoder cannot be
    ///   planned, prefixed so too
    ///   (`a_layer_whose_decoder_cannot_be_planned_is_refused_naming_it`);
    /// - each fault [`Boundary::new`] finds where the protocol's operands
    ///   cannot express the layer's own set, prefixed `layer {k}, its set:`
    ///   (`a_layer_whose_set_the_protocol_cannot_express_is_refused_with_each_fault`);
    /// - each it finds in the set below, prefixed `layer {k}, the set below:`
    ///   (`a_layer_over_a_set_the_protocol_cannot_express_is_refused_with_each_fault`).
    ///
    /// The faults of both sets come back together
    /// (`a_layer_with_faults_in_its_set_and_the_set_below_is_refused_with_each_naming_its_set`),
    /// after the runtime's error
    /// (`a_layer_the_runtime_cannot_plan_is_refused_with_its_error_then_each_sets_faults`)
    /// or the decoder's
    /// (`a_layer_whose_decoder_cannot_be_planned_is_refused_with_its_error_then_each_sets_faults`).
    pub fn new(qodec: &Qodec, k: usize) -> Result<Self, String> {
        let layers = qodec.layers();
        let bottom = layers.len().saturating_sub(1);
        if k >= bottom {
            return Err(format!(
                "layer {k} has nothing below it: the bottom is layer {bottom}"
            ));
        }
        // [QODEC] this layer's plan alone, over the next layer's ISA, and
        // its decoder's.
        let deferrable = Decoder::<BasicReadout>::deferrable(&FrameDecoder);
        let plan = LayerPlan::new(&layers[k], &layers[k + 1].instruction_set, deferrable);
        let planned = plan.map_err(|e| e.to_string()).and_then(|plan| {
            let plan = Rc::new(plan);
            let decoder = Decoder::<BasicReadout>::plan(&FrameDecoder, &plan).map_err(|e| e.0)?;
            Ok((plan, Rc::<dyn DecoderPlan<BasicReadout>>::from(decoder)))
        });
        // [GLUE]
        let accepts = Boundary::new(&layers[k].instruction_set);
        let sends = Boundary::new(&layers[k + 1].instruction_set);
        // Every check ran: the refusal reports each fault, naming its layer
        // and, for a set's, the set.
        let mut faults = Vec::new();
        if let Err(e) = &planned {
            faults.push(format!("layer {k}: {e}"));
        }
        for (which, set) in [("its set", &accepts), ("the set below", &sends)] {
            let found = set.as_ref().err().into_iter().flatten();
            faults.extend(found.map(|f| format!("layer {k}, {which}: {f}")));
        }
        match (planned, accepts, sends) {
            (Ok((plan, decoder)), Ok(accepts), Ok(sends)) => Ok(Self {
                plan,
                decoder,
                accepts: Arc::new(accepts),
                sends: Arc::new(sends),
                blocks: None,
            }),
            _ => Err(faults.join("; ")),
        }
    }

    /// This layer, refusing a call on a block the lower qubits `qubits`
    /// cannot hold, by the runtime's own count of what `n` upper blocks
    /// need below (`LayerPlan::required_resources`): the most blocks `n`
    /// whose need fits, numbered from 0.
    ///
    /// QD4: the layer refuses here, in its check at each
    /// call, since the protocol's adapter facility cannot offer upstream
    /// the qubits of its set whose blocks fit below, for the job's check to
    /// refuse a job that states more
    /// (`a_job_needing_more_blocks_than_fit_below_is_accepted`).
    ///
    /// Known issue (blocks neither): it goes by the blocks' numbers, so it
    /// is sound (no call it lets through needs more qubits below) but not
    /// complete: the runtime takes the qubits below as its blocks need
    /// them, so block 1 alone fits where one block does
    /// (`unbounded_a_lone_block_runs_on_the_lowest_qubits_below`), and is
    /// refused
    /// (`within_the_qubits_below_a_call_on_a_block_beyond_them_is_refused`).
    /// What a program needs below is how many blocks it holds at once,
    /// which only the runtime tracks.
    ///
    /// # Panics
    ///
    /// When the layer's upper ISA has more than one block type, or its
    /// lower ISA's blocks hold more than one qubit, so that its need below
    /// is not a count of qubits.
    #[must_use]
    pub fn within(mut self, qubits: u32) -> Self {
        let need = |n: u32| {
            let upper = Resources::qubits(n as usize);
            let lower = (self.plan.required_resources(&upper)).expect("one upper block type");
            assert!(lower.blocks.is_empty(), "lower blocks hold one qubit each");
            lower.qubits
        };
        let mut blocks = 0;
        while need(blocks + 1) <= qubits as usize {
            blocks += 1;
        }
        self.blocks = Some(blocks);
        self
    }

    /// The set it accepts, with its conversions.
    #[must_use]
    pub fn accepted(&self) -> &Arc<Boundary> {
        &self.accepts
    }

    /// The set it sends, with its conversions.
    #[must_use]
    pub fn sent(&self) -> &Arc<Boundary> {
        &self.sends
    }

    /// The layer as an adapter: a state per shot, the layer's runtime over
    /// the way below the state owns, which accepts the upper set and sends
    /// the lower one. It refuses a call its set does not take, such as one
    /// on operands its instruction does not take, one that repeats a tag,
    /// and one on a block beyond [`QodecLayer::within`], before anything
    /// runs. It reports each upper result's status
    /// ([`Statuses`]), beside the report below.
    ///
    /// It fails the call and the adapter on any error of the runtime: one
    /// that poisons it, and one that does not, which ran nothing, since an
    /// adapter's logic can fail a call but not refuse it (a finding).
    #[expect(
        clippy::type_complexity,
        reason = "the facility's type, as `>>` composes it"
    )]
    pub fn adapter(
        self,
    ) -> OnePerShotAdapter<
        InstructionCall,
        InstructionCall,
        QodecShot,
        Owning<
            impl FnMut(ShotContext, BelowHandle<InstructionCall>) -> Result<QodecShot, Rejected>,
        >,
        Owning<impl OwningAdapterLogic<InstructionCall, QodecShot>>,
        impl FnMut(&CallInput<'_, InstructionCall>) -> Result<(), Rejected>,
        impl FnMut(&QodecShot) -> Statuses,
    > {
        self.one_state().shots(OnePerShot)
    }

    /// The layer as an adapter of one state, before [`QodecLayer::adapter`]
    /// gives it a state per shot: a misuse, since the runtime keys its
    /// readouts by position in a shot, so that [`Lower`] fails the call at
    /// the second shot's readouts, as a readout that came twice
    /// (`a_layer_of_one_state_fails_a_call_on_two_shots`).
    #[expect(
        clippy::type_complexity,
        reason = "the facility's type, as `>>` composes it"
    )]
    fn one_state(
        self,
    ) -> AdapterFn<
        InstructionCall,
        InstructionCall,
        QodecShot,
        Owning<
            impl FnMut(ShotContext, BelowHandle<InstructionCall>) -> Result<QodecShot, Rejected>,
        >,
        Owning<impl OwningAdapterLogic<InstructionCall, QodecShot>>,
        impl FnMut(&CallInput<'_, InstructionCall>) -> Result<(), Rejected>,
        impl FnMut(&QodecShot) -> Statuses,
    > {
        let Self {
            plan,
            decoder,
            accepts,
            sends,
            blocks,
        } = self;
        let (accepted, sent) = (Arc::clone(&accepts.set), Arc::clone(&sends.set));
        let checked = Arc::clone(&accepts);
        // [FACILITY:per-shot state] made at the shot's first call, over
        // the way below it owns (`owning_adapter_fn`, `.shots(OnePerShot)`).
        let make = move |shot: ShotContext, below| {
            let lower = Lower {
                below,
                sends: Arc::clone(&sends),
                calls: Vec::new(),
                slots: Vec::new(),
            };
            Ok(QodecShot {
                shot: shot.shot(),
                accepts: Arc::clone(&accepts),
                // [QODEC] the layer's state for one shot.
                runtime: LayerRuntime::new(Rc::clone(&plan), &*decoder, lower),
                statuses: Vec::new(),
            })
        };
        // [FACILITY:lifecycle] admitting a call, failing every later call
        // once the runtime failed, closing: wholly the facility's.
        owning_adapter_fn(make, serve)
            .accepts(accepted)
            .sends(sent)
            .check(move |input| check(&checked, blocks, input.instructions()))
            .state_report(|shot: &QodecShot| {
                Statuses(BTreeMap::from([(shot.shot, shot.statuses.clone())]))
            })
    }
}

/// Refuses `calls` outside the set, repeating a tag
/// (`[PLUMBING:checks]`), or on a block of `blocks` or beyond (QD4).
fn check(
    accepts: &Boundary,
    blocks: Option<u32>,
    calls: &[InstructionCall],
) -> Result<(), Rejected> {
    accepts.set.check_batch(calls)?;
    check_unique_tags(calls.iter().enumerate().flat_map(|(at, call)| {
        call.operands.iter().filter_map(move |o| match *o {
            Operand::Tag(t) => Some((at, t)),
            _ => None,
        })
    }))?;
    let Some(blocks) = blocks else {
        return Ok(());
    };
    for (at, call) in calls.iter().enumerate() {
        for operand in call.operands.iter() {
            if let Operand::Block(b) = *operand
                && b >= blocks
            {
                return Err(Rejected::Instruction {
                    at,
                    reason: format!("block {b} is beyond the {blocks} the qubits below hold"),
                });
            }
        }
    }
    Ok(())
}

/// The layer on one shot.
pub struct QodecShot {
    shot: ShotId,
    accepts: Arc<Boundary>,
    runtime: LayerRuntime<usize, OwnedParam, BasicReadout, Lower>,
    /// Each result's status, in order, by the tag of its first readout.
    statuses: Vec<(u32, Status)>,
}

/// The logic of a shot: the call's instructions through the runtime, each
/// result's bits written under the tags of the call it answers, and its
/// status logged.
///
/// # Errors
///
/// Any error of the runtime. FRICTION: one that does not poison it ran
/// nothing, and the hand-written layer refused the call
/// (`Rejected::Malformed`); a logic can only fail it, so the adapter fails
/// too. Only `.check` refuses, before the runtime runs.
fn serve(
    shot: &mut QodecShot,
    input: &CallInput<'_, InstructionCall>,
    outcomes: &mut Outcomes<'_>,
) -> Result<(), AdapterError> {
    // [GLUE] the calls as the Qodec runtime's instructions, and their tags.
    let (upper, tags): (Vec<Ins>, Vec<SmallVec<[u32; 4]>>) = (input.instructions().iter())
        .map(|call| shot.accepts.instruction(call))
        .unzip();
    // [QODEC] lowering and decoding; calls `Lower` as often as it needs.
    let result = shot
        .runtime
        .execute_batch(&upper)
        .map_err(|e| e.to_string())?;
    // [PLUMBING:map up] results in order, to outcomes by tag.
    let mut results = result.results.iter();
    for tags in tags.iter().filter(|t| !t.is_empty()) {
        let r = results.next().ok_or("fewer results than calls that read")?;
        let bits = r.outcomes().iter().chain(r.flags()).copied();
        outcomes.extend(
            tags.iter()
                .zip(bits)
                .map(|(&tag, b)| Outcome::new(tag, value(b))),
        );
        shot.statuses.push((tags[0], r.status()));
    }
    Ok(())
}

/// Each upper result's status, by shot: a shot's in the order of its
/// results, each with the tag of its first readout. A tag a later call
/// reuses keeps each result's status
/// (`reused_readout_tags_keep_every_results_status`).
///
/// FRICTION (QD5): a `ShotOutcome` has no room for the Qodec runtime's
/// `Status`, so it goes in the layer's report, read at close; and a lower
/// layer's status does not reach the layer above as it does in the Qodec
/// runtime's stack, which merges it into the upper results.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Statuses(pub BTreeMap<ShotId, Vec<(u32, Status)>>);

impl Fold for Statuses {
    /// Each shot's statuses are its state's alone, so the shots of two
    /// reports differ.
    fn fold(&mut self, other: Self) {
        self.0.extend(other.0);
    }
}

/// The backend below a layer: the way below one shot's state owns.
pub struct Lower {
    below: BelowHandle<InstructionCall>,
    sends: Arc<Boundary>,
    calls: Vec<InstructionCall>,
    /// The bit under each tag of the batch in flight; `None`: not come up.
    slots: Vec<Option<Bit>>,
}

/// `[PLUMBING:errors]` Why the executor below did not give a batch's
/// results.
#[derive(Debug)]
pub enum LowerError {
    /// The way below refused or failed, as it says.
    Below(AdapterError),
    /// A readout asked for did not come up.
    Missing(u32),
    /// An outcome came up that was not asked for, or came twice.
    Stray(ShotOutcome),
}

impl fmt::Display for LowerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Below(reason) => write!(f, "below: {reason}"),
            Self::Missing(tag) => write!(f, "no outcome under tag {tag}"),
            Self::Stray(o) => write!(f, "an outcome not asked for, or that came twice: {o:?}"),
        }
    }
}

impl Backend<usize, OwnedParam, BasicReadout> for Lower {
    type Error = LowerError;

    /// A batch that reads runs below, on this shot alone, and waits. One
    /// that does not is sent: by the facility's rule (`Below::send`), it
    /// goes below alone before the shot's next batch that reads, or, if
    /// the shot reads nothing more in the call, with the other shots'
    /// equal sends once every shot was served. In the demo's programs a
    /// batch sent is the whole lowering of an upper call, so only the
    /// second branch is exercised (`tests/calls_below.rs`).
    fn execute_batch(
        &mut self,
        instructions: &[Ins],
    ) -> Result<BatchResult<BasicReadout>, LowerError> {
        // [GLUE] the batch as calls of the lower set, with fresh tags, in
        // the set's own numbering ([FACILITY:numbering]: the adapter
        // renumbers).
        self.calls.clear();
        let mut next = 0;
        for ins in instructions {
            let opcode = opcode(ins.instruction_id);
            self.calls.push(self.sends.call(ins, opcode, &mut next));
        }
        // [FACILITY:call and wait] a batch that reads nothing is sent; one
        // that reads runs, and `BelowHandle::run` parks this thread until
        // the reply ends (QD1, mode (c)).
        if next == 0 {
            self.below.send(&self.calls).map_err(LowerError::Below)?;
            return Ok(BatchResult {
                results: SmallVec::new(),
            });
        }
        let read = self.below.run(&self.calls).map_err(LowerError::Below)?;
        // [PLUMBING:map up] outcomes by tag, to results in order.
        self.slots.clear();
        self.slots.resize(next as usize, None);
        for o in &read {
            match self.slots.get_mut(o.tag as usize) {
                Some(slot @ None) => *slot = Some(bit(o.value)),
                _ => return Err(LowerError::Stray(*o)),
            }
        }
        let mut at = 0;
        let mut results = SmallVec::new();
        for ins in instructions {
            let layout = &self.sends.layouts[ins.instruction_id];
            if layout.readouts == 0 {
                continue;
            }
            let mut bits = SmallVec::<[Bit; 8]>::new();
            for (k, slot) in self.slots[at..at + layout.readouts].iter().enumerate() {
                let tag = u32::try_from(at + k).expect("fewer than 2^32 readouts");
                bits.push(slot.ok_or(LowerError::Missing(tag))?);
            }
            // FRICTION (QD5): the status below is lost; see `Statuses`.
            let (outs, flags) = bits.split_at(layout.observes);
            results.push(BasicReadout::new(outs, flags, Status::default()));
            at += layout.readouts;
        }
        Ok(BatchResult { results })
    }
}

#[cfg(test)]
#[path = "tests/qodec_layer.rs"]
mod tests;
