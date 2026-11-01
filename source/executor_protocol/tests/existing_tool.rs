// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! [Guide: An existing backend and a
//! profiler](../docs/guides.md#an-existing-backend-and-a-profiler):
//! two components a tool built before the protocol already has, written
//! on it as executors.
//!
//! - An existing backend stays as it is: it names its instructions by
//!   string and returns its results in the order of the batch
//!   (`existing_tool/backend.rs`). A bridge runs it on the protocol,
//!   taking the toy's instructions as calls (`existing_tool/bridge.rs`):
//!   it names each instruction once, when it is built, gives the `k`-th
//!   result the `k`-th tag, reads a lost qubit as a value, and fails the
//!   reply, and itself, when the backend errs. It is written with
//!   `executor_fn`, so it holds only that logic: the facility keeps the
//!   protocol's rules.
//! - A profiler is an executor that counts the instructions its shots run,
//!   and draws each shot's random bits from the shot's seed
//!   (`existing_tool/profiler.rs`). It is written with `executor_fn`, a
//!   state per shot: each shot's state keeps its counts and reports them
//!   when the profiler closes, and the facility folds them into the
//!   profiler's report.
//!
//! Read `existing_tool/backend.rs`, `existing_tool/bridge.rs` and
//! `existing_tool/profiler.rs`, then the tests in their order in the
//! file.

#[path = "existing_tool/backend.rs"]
mod backend;
#[path = "existing_tool/bridge.rs"]
mod bridge;
#[path = "existing_tool/profiler.rs"]
mod profiler;
#[allow(dead_code, reason = "this guide runs no toy shot")]
#[path = "toy/simulator.rs"]
mod toy;

use std::collections::BTreeMap;
use std::sync::Arc;

use executor_protocol::{
    Close, Describe, Drive, Driver, ErrorKind, Executor, ExecutorReport, Instruction,
    InstructionBatch, InstructionBatchRef, InstructionCall, InstructionSet, Needs, Operand,
    OperandKind, Rejected, ShotOutcome, ShotSet, Value, compose, conformance, wait,
};

use backend::{Bits, ExistingInterface, FixedReply, NAMES, Reading};
use bridge::backend_bridge;
use profiler::{Counts, toy_profiler};
use toy::{Op, action};

/// The toy's instructions the backend names: all but `coin`.
fn bits_set() -> InstructionSet {
    use OperandKind::{Block, Tag};
    InstructionSet::new("bits")
        .with_action("x", [Block], action("x"))
        .with_action("cx", [Block, Block], action("cx"))
        .with_action("measure", [Block, Tag], action("measure"))
        .with_action("reset", [Block], action("reset"))
}

/// `op` as a call of `set`.
fn call(set: &InstructionSet, op: Op) -> InstructionCall {
    use Operand::{Block, Tag};
    let (mnemonic, operands) = match op {
        Op::Coin(q) => ("coin", vec![Block(q)]),
        Op::X(q) => ("x", vec![Block(q)]),
        Op::Cx(c, t) => ("cx", vec![Block(c), Block(t)]),
        Op::Measure { qubit, tag } => ("measure", vec![Block(qubit), Tag(tag)]),
        Op::Reset(q) => ("reset", vec![Block(q)]),
    };
    set.call(mnemonic, operands)
        .expect("the tests call only instructions `set` describes")
}

/// A message of `instructions` on shot 0.
fn on_shot_0<I>(instructions: &[I]) -> InstructionBatchRef<'_, I> {
    const SHOT_0: &ShotSet = &ShotSet::one(0);
    InstructionBatchRef {
        shots: SHOT_0,
        instructions,
    }
}

/// `backend` through the bridge, running the calls of [`bits_set`].
fn bridged(
    backend: impl ExistingInterface,
) -> (
    impl Executor<InstructionBatch<InstructionCall>> + Describe + Close<Report = ExecutorReport<()>>,
    Arc<InstructionSet>,
) {
    let set = Arc::new(bits_set());
    let bridge = backend_bridge(backend, set.clone(), &NAMES)
        .expect("the backend names each instruction of `bits_set`");
    (bridge, set)
}

/// The events of the reply to `instructions` on shot 0 of `executor`, and
/// how the reply ended.
fn reply<I>(
    executor: &mut impl Executor<InstructionBatch<I>>,
    instructions: &[I],
) -> (Vec<ShotOutcome>, Result<(), String>) {
    let mut outcomes = Vec::new();
    let reply = executor
        .call(InstructionBatchRef {
            shots: &ShotSet::one(0),
            instructions,
        })
        .expect("the tests send only what the executor runs");
    let end = wait(reply, &mut outcomes);
    (outcomes, end)
}

/// `ops` as calls, run on `backend` through the bridge, as `(tag, value)`.
fn run_bridged(backend: impl ExistingInterface, ops: &[Op]) -> Vec<(u32, Value)> {
    let (mut bridge, set) = bridged(backend);
    let calls: Vec<_> = ops.iter().map(|&op| call(&set, op)).collect();
    let (outcomes, end) = reply(&mut bridge, &calls);
    end.expect("the backend runs the batch");
    outcomes.iter().map(|o| (o.tag, o.value)).collect()
}

/// A program: its batches, sent in order to every shot, each once the
/// reply to the one before it ended, and what it needs of the chain.
struct Program<I> {
    batches: Vec<Vec<I>>,
    needs: Needs,
}

impl<I: Instruction> Drive for Program<I> {
    type Instruction = I;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        self.needs.clone()
    }

    async fn drive<E: Executor<InstructionBatch<I>>>(
        self,
        executor: &mut E,
        shots: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let mut outcomes = Vec::new();
        for batch in &self.batches {
            let reply = executor
                .call(InstructionBatchRef {
                    shots,
                    instructions: batch,
                })
                .expect("the tests' programs send only what the executor runs");
            wait(reply, &mut outcomes).expect("the tests' programs fail no batch");
        }
        outcomes
    }
}

/// The counts of `program` and its outcomes, on `shots` of the profiler
/// whose shots draw from `seed`.
fn profile(program: Vec<Vec<Op>>, seed: u64, shots: ShotSet) -> (Counts, Vec<ShotOutcome>) {
    let program = Program {
        batches: program,
        needs: Needs::default(),
    };
    let run = compose(Driver::new(program, shots), toy_profiler(seed))
        .expect("the profiler counts any of the toy's instructions");
    let (output, report) = run.wait();
    (report.states, output)
}

/// The backend loses qubit 1: its readout is a loss, and the readout of
/// qubit 0 after it in the batch still runs.
#[test]
fn a_lost_qubit_is_read_as_a_loss_and_the_batch_goes_on() {
    let ops = [
        Op::X(0),
        Op::Measure { qubit: 1, tag: 0 },
        Op::Measure { qubit: 0, tag: 1 },
    ];
    assert_eq!(
        run_bridged(Bits::new(2, [1]), &ops),
        [(0, Value::Error(ErrorKind::Loss)), (1, Value::One)]
    );
}

/// A batch flips qubit 0 and reads qubit 1 under tag 5, then qubit 0 under
/// tag 3: the backend returns the zero and the one in that order, without
/// tags, and the bridge gives each the tag of its readout.
#[test]
fn the_bridge_gives_each_result_the_tag_of_its_readout() {
    let ops = [
        Op::X(0),
        Op::Measure { qubit: 1, tag: 5 },
        Op::Measure { qubit: 0, tag: 3 },
    ];
    assert_eq!(
        run_bridged(Bits::new(2, []), &ops),
        [(5, Value::Zero), (3, Value::One)]
    );
}

/// The bridge refuses a call at an opcode beyond its set, or with an
/// operand too many, at its index: the conformance kit's check of a
/// described set.
#[test]
fn the_bridge_refuses_calls_outside_its_set() {
    let (mut bridge, set) = bridged(Bits::new(2, []));
    let shots = ShotSet::one(0);
    conformance::refuses_calls_outside_its_set(&mut bridge, &shots, &set, &call(&set, Op::X(0)));
}

/// The toy has a `coin`, which the backend does not name: a bridge for the
/// toy's whole set is refused when it is built, naming it.
#[test]
fn the_bridge_is_refused_at_build_with_each_instruction_the_backend_does_not_name() {
    let refused = backend_bridge(Bits::new(2, []), Arc::new(toy::toy_set()), &NAMES).err();
    assert_eq!(refused, Some(vec!["coin".to_owned()]));
}

/// A backend of two qubits reads qubit 0, then cannot flip qubit 5: its
/// `Err` does not say whether any of the batch ran, so the reply fails with
/// the backend's reason, and reports not even the readout before it.
#[test]
fn the_bridge_fails_the_reply_with_no_events_when_the_backend_errs() {
    let (mut bridge, set) = bridged(Bits::new(2, []));
    let batch = [
        call(&set, Op::Measure { qubit: 0, tag: 0 }),
        call(&set, Op::X(5)),
    ];
    assert_eq!(
        reply(&mut bridge, &batch),
        (Vec::new(), Err("out of qubits".to_owned()))
    );
}

/// A faulty backend returns one result for a batch of two readouts, or
/// three: the bridge cannot tell which result is whose, so the reply fails
/// with no outcome.
#[test]
fn the_bridge_fails_the_reply_with_no_events_on_a_wrong_number_of_results() {
    use Reading::{One, Zero};
    for results in [vec![One], vec![One, Zero, One]] {
        let (mut bridge, set) = bridged(FixedReply(Ok(results)));
        let batch = [
            call(&set, Op::Measure { qubit: 0, tag: 0 }),
            call(&set, Op::Measure { qubit: 1, tag: 1 }),
        ];
        let (outcomes, end) = reply(&mut bridge, &batch);
        assert_eq!(outcomes, Vec::new());
        assert!(end.is_err(), "{end:?}");
    }
}

/// Batches the bridge refuses for their own faults: one with a call outside
/// its set (an `x` with an operand too many), one that reads two qubits
/// under one tag.
fn faulty_batches(set: &InstructionSet) -> [[InstructionCall; 2]; 2] {
    let x = call(set, Op::X(0));
    let outside = InstructionCall::new(x.opcode, [Operand::Block(0), Operand::Block(1)]);
    let read = |qubit| call(set, Op::Measure { qubit, tag: 0 });
    [[x, outside], [read(0), read(1)]]
}

/// A batch with a call outside the bridge's set, or that reads two qubits
/// under one tag, is refused, and the bridge then runs the corrected batch:
/// the conformance kit's check that a refusal ran nothing, so the bridge
/// does not fail.
#[test]
fn the_bridge_accepts_calls_after_a_refusal() {
    let (_, set) = bridged(Bits::new(2, []));
    let accepted = [call(&set, Op::X(0))];
    for refused in faulty_batches(&set) {
        let (mut bridge, _) = bridged(Bits::new(2, []));
        conformance::accepts_calls_after_a_refusal::<InstructionBatch<InstructionCall>, _>(
            &mut bridge,
            on_shot_0(&refused),
            on_shot_0(&accepted),
        );
    }
}

/// A driver of one shot, whose calls are numbered by the bridge's set,
/// flips qubit 0 and reads it: the bridge describes that set, so the chain
/// composes, and the run reads the backend's one under the readout's tag.
#[test]
fn a_driver_runs_its_calls_on_the_bridge() {
    let (bridge, set) = bridged(Bits::new(2, []));
    let program = Program {
        batches: vec![vec![
            call(&set, Op::X(0)),
            call(&set, Op::Measure { qubit: 0, tag: 4 }),
        ]],
        needs: Needs::default().with_instructions(Some(set)),
    };
    let run = compose(Driver::new(program, ShotSet::one(0)), bridge)
        .expect("the bridge describes the set the driver's calls are of");
    let (outcomes, _) = run.wait();
    assert_eq!(outcomes, [ShotOutcome::new(0, 4, Value::One)]);
}

/// The backend cannot flip qubit 5: its state is then unknown, so the
/// bridge refuses every later batch, though the backend could run it.
#[test]
fn the_bridge_refuses_calls_after_the_backend_errs() {
    let (mut bridge, set) = bridged(Bits::new(2, []));
    let later = [call(&set, Op::X(0))];
    conformance::refuses_calls_after_a_failure::<InstructionBatch<InstructionCall>, _>(
        &mut bridge,
        on_shot_0(&[call(&set, Op::X(5))]),
        &[on_shot_0(&later), on_shot_0(&later)],
    );
}

/// The backend cannot flip qubit 5, and a batch follows before the caller
/// waits for the failing batch's reply: that batch is refused, or ends
/// failed, naming the backend's error, rather than run on a backend whose
/// state is unknown: the conformance kit's check.
#[test]
fn a_batch_in_flight_when_the_backend_errs_ends_with_its_error() {
    let (mut bridge, set) = bridged(Bits::new(2, []));
    let later = [call(&set, Op::X(0))];
    conformance::ends_calls_in_flight_at_an_earlier_failure::<InstructionBatch<InstructionCall>, _>(
        &mut bridge,
        on_shot_0(&[call(&set, Op::X(5))]),
        on_shot_0(&later),
    );
}

/// A faulty backend returns one result for a batch of two readouts: which
/// readouts ran is then unknown, so the bridge refuses every later batch,
/// even one of a single readout, for which the backend's one result would
/// fit.
#[test]
fn the_bridge_refuses_calls_after_a_wrong_number_of_results() {
    let (mut bridge, set) = bridged(FixedReply(Ok(vec![Reading::One])));
    let read = |qubit, tag| call(&set, Op::Measure { qubit, tag });
    let later = [read(0, 0)];
    conformance::refuses_calls_after_a_failure::<InstructionBatch<InstructionCall>, _>(
        &mut bridge,
        on_shot_0(&[read(0, 0), read(1, 1)]),
        &[on_shot_0(&later), on_shot_0(&later)],
    );
}

/// A caller closes the bridge, then sends a batch with a call outside its
/// set, or one that repeats a tag: the bridge says that it is closed, not
/// what is wrong with the batch, since the caller may not call it at all.
#[test]
fn a_closed_bridge_says_closed_whatever_the_batch() {
    let (mut bridge, set) = bridged(Bits::new(2, []));
    bridge.close();
    for faulty in faulty_batches(&set) {
        assert_eq!(
            bridge.call(on_shot_0(&faulty)).err(),
            Some(Rejected::Closed)
        );
    }
}

/// The backend cannot flip qubit 5; the caller then sends a batch with a
/// call outside the bridge's set, or one that repeats a tag: the bridge
/// names the backend's failure, not what is wrong with the batch, since the
/// run is lost whatever the caller corrects.
#[test]
fn a_failed_bridge_names_its_failure_whatever_the_batch() {
    let (mut bridge, set) = bridged(Bits::new(2, []));
    let (_, end) = reply(&mut bridge, &[call(&set, Op::X(5))]);
    end.expect_err("a backend of two qubits cannot flip qubit 5");
    for faulty in faulty_batches(&set) {
        assert_eq!(
            bridge.call(on_shot_0(&faulty)).err(),
            Some(Rejected::EarlierFailure("out of qubits".into()))
        );
    }
}

/// A bridge that ran a batch reports once: the conformance kit's check.
#[test]
fn the_bridge_reports_once() {
    let (mut bridge, set) = bridged(Bits::new(2, []));
    let (_, end) = reply(&mut bridge, &[call(&set, Op::X(0))]);
    end.expect("the backend runs the batch");
    conformance::reports_once(&mut bridge);
}

/// A second call reads the flip of the first on the backend's qubit 0:
/// the conformance kit's check of `feedback`. The checks that a shot alone
/// gives what it gives among others, and that a call changes no shot it
/// does not name, do not apply: the bridge runs one shot.
#[test]
fn the_bridge_continues_the_backends_shot_from_one_call_to_the_next() {
    let set = bits_set();
    conformance::continues_a_shot_from_one_call_to_the_next(
        || bridged(Bits::new(2, [])).0,
        &ShotSet::one(0),
        &[call(&set, Op::X(0))],
        &[call(&set, Op::Measure { qubit: 0, tag: 0 })],
    );
}

/// A bridge built after another flipped the backend's qubit 0 reads it as
/// made: the conformance kit's check that a new executor's shots start as
/// made, whatever an earlier executor left.
#[test]
fn a_new_bridge_starts_the_backends_shot_as_made() {
    let set = bits_set();
    conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || bridged(Bits::new(2, [])).0,
        &ShotSet::one(0),
        &[call(&set, Op::X(0))],
        &[call(&set, Op::Measure { qubit: 0, tag: 0 })],
    );
}

/// A profile of sixteen shots, each reading once, reads the same bits when
/// run again with its seed, and others with another seed.
#[test]
fn a_profiles_draws_replay_from_the_runs_seed() {
    let program = || vec![vec![Op::Measure { qubit: 0, tag: 0 }]];
    let shots = || ShotSet::range(0..=15).expect("0..=15 is not empty");
    let (_, first) = profile(program(), 7, shots());
    let (_, again) = profile(program(), 7, shots());
    let (_, other) = profile(program(), 8, shots());
    assert_eq!(first, again);
    assert_ne!(first, other);
}

/// The report of a run of three shots of the profiler counts each
/// instruction three times as often as one shot runs it.
#[test]
fn a_profile_of_many_shots_is_the_sum_of_their_counts() {
    let program = || {
        vec![
            vec![Op::X(0), Op::Cx(0, 1)],
            vec![Op::X(0), Op::Measure { qubit: 1, tag: 0 }],
        ]
    };
    let (one, _) = profile(program(), 0, ShotSet::one(0));
    let (three, _) = profile(
        program(),
        0,
        ShotSet::range(0..=2).expect("0..=2 is not empty"),
    );
    assert_eq!(one, BTreeMap::from([("cx", 1), ("measure", 1), ("x", 2)]));
    assert_eq!(three, BTreeMap::from([("cx", 3), ("measure", 3), ("x", 6)]));
}

/// A profiler that ran a batch reports its counts when it closes, and
/// nothing when it closes again.
#[test]
fn a_profile_is_reported_once() {
    let mut profiler = toy_profiler(0);
    let (_, end) = reply(&mut profiler, &[Op::X(0)]);
    end.expect("the profiler fails no batch");
    conformance::reports_once(&mut profiler);
}

/// Profiles of three shots that ran different instructions fold to the
/// same total in any order, and folding in an empty profile changes
/// nothing.
#[test]
fn profiles_fold_in_any_order() {
    let counts = |pairs: &[(&'static str, u64)]| pairs.iter().copied().collect::<Counts>();
    let (a, b, c) = (
        counts(&[("x", 2)]),
        counts(&[("x", 1), ("cx", 1)]),
        counts(&[("measure", 3)]),
    );
    conformance::default_is_empty(&b);
    conformance::folds_in_any_order(&a, &b, &c);
}

/// A batch that reads two qubits under one tag is refused, and the
/// profiler then runs the corrected batch: the conformance kit's check
/// that a refusal leaves the executor usable.
#[test]
fn the_profiler_accepts_calls_after_a_refusal() {
    let read = |qubit| Op::Measure { qubit, tag: 0 };
    conformance::accepts_calls_after_a_refusal::<InstructionBatch<Op>, _>(
        &mut toy_profiler(0),
        on_shot_0(&[read(0), read(1)]),
        on_shot_0(&[read(0)]),
    );
}

/// Sixteen readouts of qubit 0, under tags `first_tag` to `first_tag + 15`.
fn sixteen_readouts(first_tag: u32) -> Vec<Op> {
    (first_tag..first_tag + 16)
        .map(|tag| Op::Measure { qubit: 0, tag })
        .collect()
}

/// Each of shots 0 to 3 reads alone, and with the shots split into calls in
/// another order, the sixteen random bits it reads among the others: the
/// conformance kit's check of `replayable`.
#[test]
fn the_profiler_gives_each_shot_alone_what_it_gives_among_others() {
    conformance::gives_each_shot_alone_what_it_gives_among_others(
        || toy_profiler(7),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        &[&sixteen_readouts(0)],
    );
}

/// A second call of sixteen readouts reads the bits that follow those of
/// the first, as one batch of both reads them: the conformance kit's
/// check of `feedback`, on each shot's draws.
#[test]
fn the_profiler_continues_a_shots_draws_from_one_call_to_the_next() {
    conformance::continues_a_shot_from_one_call_to_the_next(
        || toy_profiler(7),
        &ShotSet::range(0..=1).expect("0..=1 is not empty"),
        &sixteen_readouts(0),
        &sixteen_readouts(16),
    );
}

/// Sixteen readouts on shots 1 and 3 leave the draws of shots 0 and 2,
/// which drew before, where they were: the conformance kit's check that a
/// call changes no shot it does not name.
#[test]
fn the_profiler_changes_no_shot_a_call_does_not_name() {
    conformance::changes_no_shot_a_call_does_not_name(
        || toy_profiler(7),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        &sixteen_readouts(0),
        &sixteen_readouts(16),
    );
}

/// After a profiler drew sixteen bits on each of shots 0 to 3 and was
/// closed, or dropped unclosed, the next one draws each shot's bits from
/// its start: the conformance kit's check that a new executor starts its
/// shots as made.
#[test]
fn the_profiler_starts_its_shots_as_made_whatever_an_earlier_one_left() {
    conformance::starts_its_shots_as_made_whatever_an_earlier_executor_left(
        || toy_profiler(7),
        &ShotSet::range(0..=3).expect("0..=3 is not empty"),
        &sixteen_readouts(0),
        &sixteen_readouts(16),
    );
}

/// The profiler and the bridge each refuse a batch that repeats a tag, at
/// the instruction that repeats it, after one that reads nothing too, and
/// take one that reuses a tag of a batch before; here on shot 3, since the
/// bridge holds one shot, not always the first.
#[test]
fn each_executor_refuses_a_tag_repeated_within_a_batch() {
    let (a, b, a_again) = (
        Op::Measure { qubit: 0, tag: 0 },
        Op::Measure { qubit: 1, tag: 1 },
        Op::Measure { qubit: 1, tag: 0 },
    );
    let shots = ShotSet::one(3);
    let mut profiler = toy_profiler(0);
    conformance::refuses_a_tag_repeated_within_a_batch(&mut profiler, &shots, &a, &b, &a_again);
    let x = Op::X(1);
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut profiler,
        &shots,
        &a,
        &x,
        &a_again,
    );
    let (mut bridge, set) = bridged(Bits::new(2, []));
    let [a, b, x, a_again] = [a, b, x, a_again].map(|op| call(&set, op));
    conformance::refuses_a_tag_repeated_within_a_batch(&mut bridge, &shots, &a, &b, &a_again);
    conformance::refuses_a_repeated_tag_at_the_index_of_its_instruction(
        &mut bridge,
        &shots,
        &a,
        &x,
        &a_again,
    );
}

/// The profiler and the bridge, each once closed, refuse calls.
#[test]
fn each_executor_rejects_calls_once_closed() {
    let shots = ShotSet::one(0);
    let ops = [Op::X(0)];
    let mut profiler = toy_profiler(0);
    let batch = InstructionBatchRef {
        shots: &shots,
        instructions: &ops,
    };
    conformance::rejects_calls_once_closed::<InstructionBatch<Op>, _>(&mut profiler, batch);
    let (mut bridge, set) = bridged(Bits::new(2, []));
    let calls = ops.map(|op| call(&set, op));
    let batch = InstructionBatchRef {
        shots: &shots,
        instructions: &calls,
    };
    conformance::rejects_calls_once_closed::<InstructionBatch<InstructionCall>, _>(
        &mut bridge,
        batch,
    );
}
