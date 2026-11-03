// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crate::conformance;
use crate::tests::declared;
use crate::tests::test_double::{Model, TestDoubleExecutor};
use crate::{
    BuildError, BuildFaults, CallInput, Capabilities, Close, ComposedJob, Describe, Driver,
    Executor, ImmediateReply, InstructionBatch, InstructionBatchRef, InstructionCall,
    InstructionMismatch, InstructionSet, Need, Needs, Opcode, Operand, OperandKind, Outcomes,
    Program, Rejected, ShotContext, ShotId, ShotOutcome, ShotSet, UnmetNeed, Value,
    check_unique_tags, compose, executor_fn, wait,
};

use OperandKind::{Block, Tag};

/// The chain's set: a toy of one bit per qubit, `reset` first.
fn chain_set() -> InstructionSet {
    declared(
        InstructionSet::new("toy")
            .with("reset", [Block])
            .with("x", [Block])
            .with("measure", [Block, Tag]),
    )
}

/// Makes toy shots, which run calls numbered by the chain's set.
struct Toy {
    set: Arc<InstructionSet>,
    /// Where the last batch a shot ran was held.
    received: Rc<Cell<usize>>,
}

impl Toy {
    /// A toy that runs calls in the chain's numbering.
    fn new() -> Self {
        Self {
            set: Arc::new(chain_set()),
            received: Rc::default(),
        }
    }
}

/// A toy shot: two bits, run by the mnemonics of its set.
struct ToyState {
    shot: ShotId,
    bits: [bool; 2],
    set: Arc<InstructionSet>,
    received: Rc<Cell<usize>>,
}

impl Describe for Toy {
    fn capabilities(&self) -> Capabilities {
        Capabilities::default().with_instruction_set(Some(Arc::clone(&self.set)))
    }
}

impl Model for Toy {
    type Instruction = InstructionCall;
    type State = ToyState;

    fn state(&self, context: &ShotContext) -> Result<ToyState, String> {
        Ok(ToyState {
            shot: context.shot(),
            bits: [false; 2],
            set: Arc::clone(&self.set),
            received: Rc::clone(&self.received),
        })
    }
}

impl Executor<InstructionBatch<InstructionCall>> for ToyState {
    type Reply = ImmediateReply<ShotOutcome>;

    fn call(
        &mut self,
        batch: InstructionBatchRef<'_, InstructionCall>,
    ) -> Result<Self::Reply, Rejected> {
        batch.shots.check_only(self.shot)?;
        // Its set first, then its tags, as the crate's executors check.
        self.set.check_batch(batch.instructions)?;
        check_unique_tags(
            (batch.instructions.iter().enumerate()).filter_map(|(at, call)| {
                match call.operands.get(1) {
                    Some(&Operand::Tag(tag)) => Some((at, tag)),
                    _ => None,
                }
            }),
        )?;
        self.received.set(batch.instructions.as_ptr().addr());
        let mut outcomes = Vec::new();
        for call in batch.instructions {
            let operand = |i: usize| match call.operands[i] {
                Operand::Block(n) | Operand::Tag(n) => n,
                _ => unreachable!("the toy takes blocks and tags"),
            };
            let bit = &mut self.bits[operand(0) as usize];
            match self.set.mnemonic(call.opcode) {
                Some("reset") => *bit = false,
                Some("x") => *bit = !*bit,
                _ => outcomes.push(ShotOutcome::new(self.shot, operand(1), (*bit).into())),
            }
        }
        Ok(ImmediateReply::done(outcomes))
    }
}

impl Close for ToyState {
    type Report = ();

    fn close(&mut self) {}
}

/// A program of calls by opcode, built with the set it needs: batches,
/// each run on every shot of its job, in turn ([`EachInTurn`]), giving the
/// values of each batch.
struct Batches {
    needs: Needs,
    batches: Vec<Vec<InstructionCall>>,
}

impl Batches {
    /// `batches`, built with `set`.
    fn new(set: &InstructionSet, batches: Vec<Vec<InstructionCall>>) -> Self {
        Self {
            needs: needing(set),
            batches,
        }
    }
}

impl Program for Batches {
    type Instruction = InstructionCall;
    type Output = Vec<Vec<Value>>;

    fn needs(&self) -> Needs {
        self.needs.clone()
    }
}

/// A driver of [`Batches`]: it sends each batch to every shot of its job,
/// in turn, waiting for each.
struct EachInTurn;

impl Driver<Batches> for EachInTurn {
    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        &self,
        program: Batches,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<Vec<Value>> {
        (program.batches.iter())
            .map(|batch| {
                let reply = executor
                    .call(InstructionBatchRef {
                        shots,
                        instructions: batch,
                    })
                    .expect("the batch is valid");
                let mut out: Vec<ShotOutcome> = Vec::new();
                wait(reply, &mut out).expect("the toy's batches end well");
                out.iter().map(|outcome| outcome.value).collect()
            })
            .collect()
    }
}

/// What `program` gives on the toy's shots `shots`.
fn run(program: Batches, toy: Toy, shots: ShotSet) -> Vec<Vec<Value>> {
    let (output, _) = compose(
        ComposedJob::driven(EachInTurn, program, shots),
        TestDoubleExecutor::with_model(toy, 7),
    )
    .expect("the toy accepts the program's set")
    .wait();
    output
}

/// The faults of composing a program needing `needs` with the toy.
fn refused(needs: Needs) -> BuildFaults {
    let program = Batches {
        needs,
        batches: Vec::new(),
    };
    let composed = compose(
        ComposedJob::driven(EachInTurn, program, ShotSet::one(1)),
        TestDoubleExecutor::with_model(Toy::new(), 7),
    );
    let Err(faults) = composed else {
        panic!("the toy lacks what the program needs");
    };
    faults
}

/// The unmet needs among `faults`.
fn unmet(faults: &BuildFaults) -> Vec<&UnmetNeed> {
    faults
        .0
        .iter()
        .filter_map(|fault| match &fault.error {
            BuildError::Unmet(unmet) => Some(unmet),
            _ => None,
        })
        .collect()
}

/// The needs of a program that builds its calls with `set`.
fn needing(set: &InstructionSet) -> Needs {
    Needs::default().with_instruction_set(Some(Arc::new(set.clone())))
}

/// A program's set that declares the toy's instructions in another order:
/// its `x` is at the toy's `reset`'s opcode.
fn program_set() -> InstructionSet {
    declared(
        InstructionSet::new("program")
            .with("x", [Block])
            .with("measure", [Block, Tag])
            .with("reset", [Block]),
    )
}

/// `x` on block 0, then its measurement, built with `set`.
fn flip_and_measure(set: &InstructionSet) -> [InstructionCall; 2] {
    [
        set.call("x", [Operand::Block(0)])
            .expect("`x` takes a block"),
        set.call("measure", [Operand::Block(0), Operand::Tag(0)])
            .expect("`measure` takes a block and a tag"),
    ]
}

/// A program whose set declares the executor's instructions in another
/// order builds its calls with its own set, and they run as it means them, on
/// an executor that runs calls in its own numbering only: its `x` flips the
/// bit, where the executor's instruction at that opcode would reset it.
#[test]
fn a_program_whose_set_is_in_another_order_has_its_own_instructions_run() {
    let set = program_set();
    let program = Batches::new(&set, vec![flip_and_measure(&set).to_vec()]);
    let shots = ShotSet::range(1..=3).expect("a range of shots");
    let values = run(program, Toy::new(), shots);
    assert_eq!(values, [[Value::One; 3]]);
}

/// A program whose set numbers the instructions as the executor's does (its
/// first ones, in the same order) needs no renumbering: each batch reaches
/// the shots as the program holds it.
#[test]
fn a_batch_in_the_executors_numbering_reaches_it_as_it_is() {
    let set = declared(
        InstructionSet::new("first two")
            .with("reset", [Block])
            .with("x", [Block]),
    );
    let toy = Toy::new();
    let received = Rc::clone(&toy.received);
    let batch = vec![
        set.call("x", [Operand::Block(0)])
            .expect("`x` takes a block"),
    ];
    let held = batch.as_ptr().addr();
    run(Batches::new(&set, vec![batch]), toy, ShotSet::one(1));
    assert_eq!(received.get(), held);
}

/// A program's set the executor does not accept is refused when its job is
/// given to the chain, with each of its instructions that does not fit.
#[test]
fn a_set_the_executor_lacks_is_refused_with_every_instruction_that_does_not_fit() {
    let set = program_set().with("h", [Block]).with("cx", [Block, Block]);
    let faults = refused(needing(&set));
    let unmet = unmet(&faults);
    assert_eq!(unmet.len(), 1, "{faults}");
    assert_eq!(
        (unmet[0].need, unmet[0].mismatches.clone()),
        (
            Need::InstructionSet,
            vec![
                InstructionMismatch::Missing {
                    mnemonic: "h".into()
                },
                InstructionMismatch::Missing {
                    mnemonic: "cx".into()
                },
            ]
        )
    );
}

/// Each call runs its own batch, though a longer one came before: none of
/// the earlier batch's calls is run again.
#[test]
fn each_call_runs_only_its_own_batch() {
    let set = program_set();
    let [x, measure] = flip_and_measure(&set);
    let measure_again = set
        .call("measure", [Operand::Block(0), Operand::Tag(1)])
        .expect("`measure` takes a block and a tag");
    let batches = vec![vec![x, measure.clone(), measure_again], vec![measure]];
    let values = run(Batches::new(&set, batches), Toy::new(), ShotSet::one(1));
    assert_eq!(values[1], [Value::One]);
}

/// A program of calls by opcode in `set`, built with it, whose calls its
/// driver writes.
struct OfSet {
    set: InstructionSet,
}

impl Program for OfSet {
    type Instruction = InstructionCall;
    type Output = ();

    fn needs(&self) -> Needs {
        needing(&self.set)
    }
}

/// A driver that checks that the executor it drives refuses calls outside
/// its program's set ([`conformance::refuses_calls_outside_its_set`]).
struct CallsOutside;

impl Driver<OfSet> for CallsOutside {
    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        &self,
        program: OfSet,
        executor: &mut E,
        _: &ShotSet,
    ) {
        let x = (program.set)
            .call("x", [Operand::Block(0)])
            .expect("`x` takes a block");
        conformance::refuses_calls_outside_its_set(executor, &ShotSet::one(1), &program.set, &x);
    }
}

/// When each batch is renumbered, a call at an opcode the program's set
/// lacks is refused at its index, though the executor's set has an
/// instruction there that takes the same operands (its `x`, at the opcode
/// after the program's only one).
#[test]
fn a_call_outside_the_programs_set_is_refused() {
    let set = declared(InstructionSet::new("x alone").with("x", [Block]));
    compose(
        ComposedJob::driven(CallsOutside, OfSet { set }, ShotSet::one(1)),
        TestDoubleExecutor::with_model(Toy::new(), 7),
    )
    .expect("the toy accepts the program's set")
    .wait();
}

/// A program of calls by opcode, built with `set`: batches, each sent to
/// see whether it is refused ([`SendsEach`]), giving how each was refused,
/// if it was.
struct Probes {
    set: InstructionSet,
    batches: Vec<Vec<InstructionCall>>,
}

impl Program for Probes {
    type Instruction = InstructionCall;
    type Output = Vec<Option<Rejected>>;

    fn needs(&self) -> Needs {
        needing(&self.set)
    }
}

/// A driver of [`Probes`]: it sends each batch to every shot of its job,
/// and keeps how each was refused.
struct SendsEach;

impl Driver<Probes> for SendsEach {
    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        &self,
        program: Probes,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<Option<Rejected>> {
        (program.batches.iter())
            .map(|batch| {
                let answer = executor.call(InstructionBatchRef {
                    shots,
                    instructions: batch,
                });
                answer.err()
            })
            .collect()
    }
}

/// When each batch is renumbered, a batch with a call on operands outside
/// the set and a later call at an opcode the program's set lacks is refused
/// at the first, as the executor alone would refuse it, whether it is the
/// batch's first call or the one just before, and at the first of two such
/// calls; and the other way round too: the numbering does not change which
/// call outside the set is reported.
/// A batch whose only call outside is at an opcode lacked is refused there.
#[test]
fn a_renumbered_batch_is_refused_at_its_first_call_outside() {
    let set = declared(InstructionSet::new("x alone").with("x", [Block]));
    let x = set
        .call("x", [Operand::Block(0)])
        .expect("`x` takes a block");
    let on_a_real = InstructionCall::new(x.opcode, [Operand::Real(0.5)]);
    let lacked = InstructionCall::new(Opcode::new(1), [Operand::Block(0)]);
    let batches = vec![
        vec![on_a_real.clone(), x.clone(), lacked.clone()],
        vec![lacked.clone(), x.clone(), on_a_real.clone()],
        vec![x.clone(), on_a_real.clone(), lacked.clone()],
        vec![x.clone(), on_a_real.clone(), on_a_real, lacked.clone()],
        vec![x.clone(), x, lacked],
    ];
    let (refusals, _) = compose(
        ComposedJob::driven(SendsEach, Probes { set, batches }, ShotSet::one(1)),
        TestDoubleExecutor::with_model(Toy::new(), 7),
    )
    .expect("the toy accepts the program's set")
    .wait();
    let refused_at = |at, reason: InstructionMismatch| {
        Some(Rejected::Instruction {
            at,
            reason: reason.to_string(),
        })
    };
    let on_operands = || InstructionMismatch::DifferentOperands {
        mnemonic: "x".into(),
    };
    let at_opcode = || InstructionMismatch::UnknownOpcode {
        opcode: Opcode::new(1),
    };
    assert_eq!(
        refusals,
        [
            refused_at(0, on_operands()),
            refused_at(0, at_opcode()),
            refused_at(1, on_operands()),
            refused_at(1, on_operands()),
            refused_at(2, at_opcode()),
        ]
    );
}

/// A batch that reads two qubits under one tag, then calls outside the
/// set: the toy, which checks its set before its tags, as the crate's
/// executors do, refuses it at that last call. A program in another
/// numbering, whose set lacks `reset`, sends the same readouts and a call
/// at an opcode its set lacks, and is refused at the same call: for such
/// an executor, the numbering does not change which call outside the set
/// is reported, even when an earlier call would be refused by its tag
/// check.
#[test]
fn a_renumbered_batch_is_refused_where_a_batch_in_the_chains_numbering_is() {
    let measure = |set: &InstructionSet, qubit| {
        set.call("measure", [Operand::Block(qubit), Operand::Tag(0)])
            .expect("`measure` takes a block and a tag")
    };
    let refused_at = |set: InstructionSet, outside: Opcode| {
        let batch = vec![
            measure(&set, 0),
            measure(&set, 1),
            InstructionCall::new(outside, [Operand::Block(0)]),
        ];
        let (refusals, _) = compose(
            ComposedJob::driven(
                SendsEach,
                Probes {
                    set,
                    batches: vec![batch],
                },
                ShotSet::one(1),
            ),
            TestDoubleExecutor::with_model(Toy::new(), 7),
        )
        .expect("the toy accepts the program's set")
        .wait();
        match refusals[..] {
            [Some(Rejected::Instruction { at, .. })] => at,
            ref other => panic!("the batch must be refused at a call, not {other:?}"),
        }
    };
    let other = declared(
        InstructionSet::new("no reset")
            .with("measure", [Block, Tag])
            .with("x", [Block]),
    );
    assert_eq!(
        refused_at(chain_set(), Opcode::new(3)),
        2,
        "in the chain's numbering"
    );
    assert_eq!(refused_at(other, Opcode::new(2)), 2, "in another numbering");
}

/// Makes toy shots, of which shot 2's holds another shot, so that a call
/// that names shot 2 fails the executor.
struct ToyFailingAtShot2(Toy);

impl Describe for ToyFailingAtShot2 {
    fn capabilities(&self) -> Capabilities {
        self.0.capabilities()
    }
}

impl Model for ToyFailingAtShot2 {
    type Instruction = InstructionCall;
    type State = ToyState;

    fn state(&self, context: &ShotContext) -> Result<ToyState, String> {
        let mut state = self.0.state(context)?;
        if state.shot == 2 {
            state.shot = 99;
        }
        Ok(state)
    }
}

/// A program in another numbering than the toy's runs `[x]` on shots 1 and
/// 2, which is accepted and fails the toy at shot 2. Its next batches are
/// refused where they call outside the set, at an opcode its set lacks or
/// at an earlier call on operands outside, as before the failure: the
/// renumbering refuses them before the toy is asked. A batch within the
/// set is then refused for the earlier failure.
#[test]
fn a_renumbered_batch_is_refused_at_its_call_before_an_earlier_failure() {
    let set = declared(InstructionSet::new("x alone").with("x", [Block]));
    let x = set
        .call("x", [Operand::Block(0)])
        .expect("`x` takes a block");
    let on_a_real = InstructionCall::new(x.opcode, [Operand::Real(0.5)]);
    let lacked = InstructionCall::new(Opcode::new(1), [Operand::Block(0)]);
    let batches = vec![
        vec![x.clone()],
        vec![x.clone(), lacked.clone()],
        vec![on_a_real, lacked],
        vec![x],
    ];
    let shots = ShotSet::range(1..=2).expect("shots 1 and 2");
    let (refusals, _) = compose(
        ComposedJob::driven(SendsEach, Probes { set, batches }, shots),
        TestDoubleExecutor::with_model(ToyFailingAtShot2(Toy::new()), 7),
    )
    .expect("the toy accepts the program's set")
    .wait();
    let at = |refusal: &Option<Rejected>| match refusal {
        Some(Rejected::Instruction { at, .. }) => Some(*at),
        _ => None,
    };
    assert_eq!(at(&refusals[1]), Some(1), "at the opcode lacked");
    assert_eq!(at(&refusals[2]), Some(0), "at the call on operands outside");
    assert!(
        matches!(refusals[3], Some(Rejected::EarlierFailure(_))),
        "a batch within the set meets the failure: {:?}",
        refusals[3]
    );
}

/// A toy closed before `compose` serves a program in another numbering. Its
/// batch `[x, lacked]` is refused at the opcode lacked, by the
/// renumbering, which does not ask the toy; a batch within the set reaches
/// the toy, which refuses it as closed.
#[test]
fn a_renumbered_batch_is_refused_at_its_call_before_a_closed_executor_is() {
    let set = declared(InstructionSet::new("x alone").with("x", [Block]));
    let x = set
        .call("x", [Operand::Block(0)])
        .expect("`x` takes a block");
    let lacked = InstructionCall::new(Opcode::new(1), [Operand::Block(0)]);
    let mut toy = TestDoubleExecutor::with_model(Toy::new(), 7);
    let _ = toy.close();
    let batches = vec![vec![x.clone(), lacked], vec![x]];
    let (refusals, _) = compose(
        ComposedJob::driven(SendsEach, Probes { set, batches }, ShotSet::one(1)),
        toy,
    )
    .expect("the toy accepts the program's set")
    .wait();
    assert!(
        matches!(refusals[1], Some(Rejected::Closed)),
        "a batch within the set must reach the closed toy: {:?}",
        refusals[1]
    );
    assert!(
        matches!(refusals[0], Some(Rejected::Instruction { at: 1, .. })),
        "the batch must be refused at the opcode lacked: {:?}",
        refusals[0]
    );
}

/// A toy closed before `compose` serves a program in another numbering. Its
/// batch calls `x` on a real, an opcode both sets know on an operand the
/// toy's set refuses: the renumbering refuses only a call at an opcode the
/// program's set lacks, so this batch reaches the toy, which refuses it as
/// closed, as it would in the chain's numbering.
#[test]
fn a_renumbered_batch_outside_the_set_only_by_its_operands_reaches_the_executor() {
    let set = declared(InstructionSet::new("x alone").with("x", [Block]));
    let x = set
        .call("x", [Operand::Block(0)])
        .expect("`x` takes a block");
    let on_a_real = InstructionCall::new(x.opcode, [Operand::Real(0.5)]);
    let mut toy = TestDoubleExecutor::with_model(Toy::new(), 7);
    let _ = toy.close();
    let (refusals, _) = compose(
        ComposedJob::driven(
            SendsEach,
            Probes {
                set,
                batches: vec![vec![on_a_real]],
            },
            ShotSet::one(1),
        ),
        toy,
    )
    .expect("the toy accepts the program's set")
    .wait();
    assert!(
        matches!(refusals[0], Some(Rejected::Closed)),
        "the batch must reach the closed toy: {:?}",
        refusals[0]
    );
}

/// An executor of one state has served shot 5 before `compose`, so it
/// holds no other shot. A driver runs a program in another numbering on
/// it, on shot 1: its batch `[x, lacked]` is refused at the opcode lacked,
/// by the renumbering, which does not ask the executor; a batch within the
/// set reaches the executor, which refuses shot 1 as not held.
#[test]
fn a_renumbered_batch_is_refused_at_its_call_before_the_executors_shot_not_held() {
    fn run_nothing(
        (): &mut (),
        _: &CallInput<'_, InstructionCall>,
        _: &mut Outcomes<'_>,
    ) -> Result<(), String> {
        Ok(())
    }
    let chain = Arc::new(chain_set());
    let mut executor = executor_fn(
        Capabilities::default().with_instruction_set(Some(Arc::clone(&chain))),
        |_: ShotContext| Ok::<(), Rejected>(()),
        run_nothing,
    );
    let x_in_chain = chain
        .call("x", [Operand::Block(0)])
        .expect("the chain's `x` takes a block");
    executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(5),
            instructions: &[x_in_chain],
        })
        .expect("the executor takes shot 5 as its one state");
    let set = declared(InstructionSet::new("x alone").with("x", [Block]));
    let x = set
        .call("x", [Operand::Block(0)])
        .expect("`x` takes a block");
    let lacked = InstructionCall::new(Opcode::new(1), [Operand::Block(0)]);
    let batches = vec![vec![x.clone(), lacked], vec![x]];
    let (refusals, _) = compose(
        ComposedJob::driven(SendsEach, Probes { set, batches }, ShotSet::one(1)),
        executor,
    )
    .expect("the executor accepts the program's set")
    .wait();
    assert!(
        matches!(refusals[1], Some(Rejected::ShotNotHeld(1))),
        "a batch within the set must reach the executor: {:?}",
        refusals[1]
    );
    assert!(
        matches!(refusals[0], Some(Rejected::Instruction { at: 1, .. })),
        "the batch must be refused at the opcode lacked: {:?}",
        refusals[0]
    );
}

/// A program of calls by opcode, built with `set`: batches, each on shots
/// of its own, which may be outside the job's ([`SendsEachOnItsShots`]),
/// giving what each call refused.
struct ProbesOnShots {
    set: InstructionSet,
    batches: Vec<(ShotSet, Vec<InstructionCall>)>,
}

impl Program for ProbesOnShots {
    type Instruction = InstructionCall;
    type Output = Vec<Option<Rejected>>;

    fn needs(&self) -> Needs {
        needing(&self.set)
    }
}

/// A driver of [`ProbesOnShots`]: it sends each batch to its own shots,
/// whatever the job's, and keeps how each was refused.
struct SendsEachOnItsShots;

impl Driver<ProbesOnShots> for SendsEachOnItsShots {
    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        &self,
        program: ProbesOnShots,
        executor: &mut E,
        _: &ShotSet,
    ) -> Vec<Option<Rejected>> {
        (program.batches.iter())
            .map(|(shots, batch)| {
                let answer = executor.call(InstructionBatchRef {
                    shots,
                    instructions: batch,
                });
                answer.err()
            })
            .collect()
    }
}

/// A job of shot 1, in another numbering than the executor's, calls
/// `[x, lacked]` on shot 2, which it does not hold: the run refuses the
/// shot before the renumbering sees the batch, so the refusal is
/// `ShotNotHeld`, not the renumbering's `Instruction`, which the same batch
/// gets on shot 1.
#[test]
fn a_batch_on_a_shot_the_job_does_not_hold_is_refused_before_the_renumbering() {
    let set = declared(InstructionSet::new("x alone").with("x", [Block]));
    let x = set
        .call("x", [Operand::Block(0)])
        .expect("`x` takes a block");
    let lacked = InstructionCall::new(Opcode::new(1), [Operand::Block(0)]);
    let batch = vec![x, lacked];
    let batches = vec![(ShotSet::one(1), batch.clone()), (ShotSet::one(2), batch)];
    let (refusals, _) = compose(
        ComposedJob::driven(
            SendsEachOnItsShots,
            ProbesOnShots { set, batches },
            ShotSet::one(1),
        ),
        TestDoubleExecutor::with_model(Toy::new(), 7),
    )
    .expect("the toy accepts the program's set")
    .wait();
    assert!(
        matches!(refusals[0], Some(Rejected::Instruction { at: 1, .. })),
        "on a shot the job holds, the renumbering must refuse the opcode \
         lacked: {:?}",
        refusals[0]
    );
    assert_eq!(refusals[1], Some(Rejected::ShotNotHeld(2)));
}

/// A program that needs the log probability, which the toy does not compute, is
/// refused when its job is given to the chain, whether it states no set, a
/// set in the executor's numbering, or one in another numbering: its needs
/// other than its instructions are checked on every path.
#[test]
fn a_program_needing_what_the_executor_lacks_is_refused_whatever_its_set() {
    let first = declared(InstructionSet::new("first").with("reset", [Block]));
    for needs in [Needs::default(), needing(&first), needing(&program_set())] {
        let faults = refused(needs.with_log_probability(true));
        assert!(
            unmet(&faults)
                .iter()
                .any(|unmet| unmet.need == Need::LogProbability),
            "{faults}"
        );
    }
}

/// A program whose `x` declares nothing, on a toy, a set of another name,
/// that declares what each instruction does: what the `x` does cannot be
/// checked, so the chain refuses the job, as the program's need of an
/// instruction set unmet.
#[test]
fn an_instruction_whose_action_the_programs_set_leaves_undeclared_is_refused() {
    let set = program_set().with("x", [Block]);
    let faults = refused(needing(&set));
    let unmet = unmet(&faults);
    assert_eq!(unmet.len(), 1, "{faults}");
    assert_eq!(unmet[0].need, Need::InstructionSet);
    let undeclared = InstructionMismatch::UndeclaredAction {
        mnemonic: "x".to_owned(),
    };
    assert_eq!(unmet[0].mismatches, [undeclared]);
}
