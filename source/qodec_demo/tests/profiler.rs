//! A profiler in the Qodec chains: below a Qodec layer it counts the
//! physical instructions a program costs, as the chain
//! `static_driver() >> qodec >> profiler() >> backend` that the Qodec
//! runtime's users plan; above it, the logical ones; with a profiler at
//! each place in one chain, each counts its own place's. Each count is
//! derived from the program, from the code's gadgets, or from what the
//! Qodec runtime's own stack sends its bottom backends, and the profiler
//! changes no outcome. Counting each shot's calls, its totals
//! do not depend on how the calls below Qodec are grouped. Awaited, a run
//! over the profiler yields while its calls below are pending, over a
//! backend answered from another thread once the run yields.

mod common;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use executor_protocol::{
    Driver, DriverExt, Executor, ExecutorReport, InstructionBatch, InstructionBatchRef,
    InstructionCall, InstructionSet, Needs, Opcode, Operand, ShotId, ShotOutcome, ShotSet,
    StaticProgram, static_driver, wait,
};
use qodec_demo::answers_on_yield::{AnswersOnYield, YieldGate};
use qodec_demo::call_counter::{Calls, call_counter};
use qodec_demo::coin::coin_device;
use qodec_demo::noisy_bits::{CX, M, R, ROTATE_Z, simulator};
use qodec_demo::profiler::{Profile, profiler};
use qodec_demo::program::Program;
use qodec_demo::qodec_layer::QodecLayer;
use qodec_demo::recorder::{Sent, recorder};
use qodec_demo::stack::on_the_stack;

use common::two_reads_over_coins_refusing;
use common::waiting::bounded;
use common::{
    C4C6_BOTTOM_BATCHES, CODE_SHOTS, FLIP, SEED, SHOTS, bottom, c4c6, c4c6_one_batch_program,
    c4c6_program, c422, c422_program, repetition3, repetition3_program, shot_set, sorted, stim_rz,
};

/// Repetition3's program, as many shots as [`SHOTS`].
const SHOT_COUNT: u64 = SHOTS.len() as u64;

/// `counts` of each opcode, `batches` batches, over all shots.
fn profile(batches: u64, counts: impl IntoIterator<Item = (Opcode, u64)>) -> Profile {
    Profile {
        batches,
        counts: BTreeMap::from_iter(counts),
    }
}

/// Repetition3 on the chain with no profiler: its outcomes.
fn repetition3_unprofiled() -> Vec<ShotOutcome> {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let chain = (static_driver() >> layer.adapter() >> simulator(stim_rz().set, SEED, FLIP))
        .expect("every boundary matches");
    let program = repetition3_program(&qodec).to_static(Needs::default());
    let (outcomes, _) = chain
        .run(program, shot_set(&SHOTS))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds")
}

/// Repetition3 with the profiler below the layer: its outcomes and the
/// profile.
fn repetition3_profiled_below() -> (Vec<ShotOutcome>, Profile) {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let simulator = simulator(stim_rz().set, SEED, FLIP);
    let chain = (static_driver() >> layer.adapter() >> profiler() >> simulator)
        .expect("every boundary matches");
    let program = repetition3_program(&qodec).to_static(Needs::default());
    let (outcomes, report): (_, (Profile, ExecutorReport<()>)) = chain
        .run(program, shot_set(&SHOTS))
        .expect("the job fits the chain")
        .wait();
    (outcomes.expect("the run succeeds"), report.0)
}

/// Repetition3 with the profiler above the layer: its outcomes, the
/// profile, and the logical set's opcodes of the program's mnemonics.
fn repetition3_profiled_above() -> (Vec<ShotOutcome>, Profile, [Opcode; 4]) {
    let qodec = repetition3();
    let layer = QodecLayer::new(&qodec, 0).expect("repetition3 plans");
    let logical = layer.accepted().set.clone();
    let opcode = |m| logical.opcode(m).expect("a logical instruction");
    let opcodes = ["prepare_z", "idle", "rotate_z", "measure_z"].map(opcode);
    let simulator = simulator(stim_rz().set, SEED, FLIP);
    let chain = (static_driver() >> profiler() >> layer.adapter() >> simulator)
        .expect("every boundary matches");
    let program = repetition3_program(&qodec).to_static(Needs::default());
    let (outcomes, report): (_, (Profile, ExecutorReport<()>)) = chain
        .run(program, shot_set(&SHOTS))
        .expect("the job fits the chain")
        .wait();
    (outcomes.expect("the run succeeds"), report.0, opcodes)
}

/// Below the layer, the profiler counts the physical instructions of
/// repetition3's gadgets, shot by shot: `prepare_z` is `R 0 1 2`; each of
/// the three `idle`s is `R 3 4`, four `CX`s and `M 3 4`; `rotate_z` is one
/// `rotate_z`; `measure_z` is `M 0 1 2`.
#[test]
fn below_the_layer_the_profiler_counts_the_gadgets_instructions() {
    let (_, profile) = repetition3_profiled_below();

    let idles = 3;
    let per_shot = [
        (R, 3 + 2 * idles),
        (M, 2 * idles + 3),
        (CX, 4 * idles),
        (ROTATE_Z, 1),
    ];
    let expected = per_shot.map(|(opcode, n)| (opcode, n * SHOT_COUNT));
    assert_eq!(profile.counts, BTreeMap::from(expected));
}

/// Above the layer, the profiler counts the program's logical
/// instructions, in the program's batches: one `prepare_z`, three `idle`s,
/// one `rotate_z` and one `measure_z` a shot, in three batches.
#[test]
fn above_the_layer_the_profiler_counts_the_programs_instructions() {
    let (_, profile, [prepare_z, idle, rotate_z, measure_z]) = repetition3_profiled_above();

    let per_shot = [(prepare_z, 1), (idle, 3), (rotate_z, 1), (measure_z, 1)];
    let expected = profile_of(3, per_shot);
    assert_eq!(profile, expected);
}

/// `per_shot` counts and `batches` batches of one shot, over [`SHOTS`].
fn profile_of(batches: u64, per_shot: [(Opcode, u64); 4]) -> Profile {
    profile(
        batches * SHOT_COUNT,
        per_shot.map(|(opcode, n)| (opcode, n * SHOT_COUNT)),
    )
}

/// c422 below its layer, with `$profiler` (a profiler or nothing) between
/// the layer and the device: the calls the device gets, counted whole,
/// and each shot's batches, as they reach it.
macro_rules! c422_below {
    ($($profiler:expr)?) => {{
        let qodec = c422();
        let layer = QodecLayer::new(&qodec, 0).expect("c422 plans");
        let coins = coin_device(bottom(&qodec).set.clone(), SEED);
        let chain = (static_driver() >> layer.adapter() $(>> $profiler)? >> call_counter()
            >> recorder() >> coins)
            .expect("every boundary matches");
        let program = c422_program(&qodec).to_static(Needs::default());
        let shots: Vec<_> = CODE_SHOTS.collect();
        let (outcomes, report) = chain
            .run(program, shot_set(&shots))
            .expect("the job fits the chain")
            .wait();
        outcomes.expect("the run succeeds");
        report
    }};
}

/// The profiler sends each call down as it is: below c422's layer, the
/// device gets the same calls, and each shot the same batches, with the
/// profiler between them as without it.
#[test]
fn the_profiler_sends_each_call_below_as_it_is() {
    let (_, (calls, (sent, _))): (Profile, (Calls, (Sent, _))) = c422_below!(profiler());
    let (without_calls, (without_sent, _)): (Calls, (Sent, _)) = c422_below!();

    assert!(without_calls.0 > 0, "the device was called");
    assert_eq!((calls, sent), (without_calls, without_sent));
}

/// Once a shot has a state, the profiler counts a call the executor below
/// refuses: over coins that refuse the shot's second call, two reads are
/// counted, though the coins ran one.
#[test]
fn once_a_shot_has_a_state_the_profiler_counts_a_call_refused_below() {
    let (_, counted, ran): (_, Profile, _) = two_reads_over_coins_refusing!(profiler(), 1);
    assert_eq!(ran, profile(1, [(M, 1)]), "precondition: the coins ran one");

    assert_eq!(counted, profile(2, [(M, 2)]));
}

/// A shot's first call, refused below, is not counted: the state made for
/// it goes with the refusal, as the adapter facility keeps no state made
/// for a refused call. Over coins that refuse the shot's first call, only
/// the second read is counted.
#[test]
fn the_profiler_counts_no_shots_first_call_refused_below() {
    let (_, counted, ran): (_, Profile, _) = two_reads_over_coins_refusing!(profiler(), 0);
    assert_eq!(ran, profile(1, [(M, 1)]), "precondition: the coins ran one");

    assert_eq!(counted, profile(1, [(M, 1)]));
}

/// The profiler changes no outcome, below the layer or above it.
#[test]
fn the_profiler_changes_no_outcome() {
    let unprofiled = sorted(repetition3_unprofiled());
    let (below, _) = repetition3_profiled_below();
    let (above, ..) = repetition3_profiled_above();

    assert_eq!(sorted(below), unprofiled, "below the layer");
    assert_eq!(sorted(above), unprofiled, "above the layer");
}

/// c4c6, both layers above the profiler: per shot, the one top batch
/// `prepare_z_all 0; idle 0; measure_z_all 0` costs the 14 bottom batches
/// the Qodec runtime's own console lists for it.
#[test]
fn below_two_layers_the_profiler_counts_the_runtimes_bottom_batches() {
    let qodec = c4c6();
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> top.adapter() >> middle.adapter() >> profiler() >> coins)
        .expect("every boundary matches");
    let program = c4c6_one_batch_program(&qodec).to_static(Needs::default());
    let shots: Vec<_> = CODE_SHOTS.collect();
    let (outcomes, (profile, _)): (_, (Profile, _)) = chain
        .run(program, shot_set(&shots))
        .expect("the job fits the chain")
        .wait();
    outcomes.expect("the run succeeds");

    assert_eq!(profile.batches, C4C6_BOTTOM_BATCHES * shots.len() as u64);
}

/// `$chain`, the static driver first and the coins of `$qodec`'s bottom set
/// last, run on shots 0 to 3: its report, the run having succeeded.
macro_rules! report_of {
    ($qodec:expr, $program:expr, $($chain:expr)=>+) => {{
        let coins = coin_device(bottom(&$qodec).set.clone(), SEED);
        let chain = (static_driver() $(>> $chain)+ >> coins).expect("every boundary matches");
        let shots: Vec<_> = CODE_SHOTS.collect();
        let (outcomes, report) = chain
            .run($program.to_static(Needs::default()), shot_set(&shots))
            .expect("the job fits the chain")
            .wait();
        outcomes.expect("the run succeeds");
        report
    }};
}

/// c4c6, both layers above the profiler: it counts what the Qodec
/// runtime's own stack sends its bottom backend, instruction by
/// instruction and batch by batch, on the whole program, as counted from
/// the batches the stack's backends kept.
#[test]
fn below_two_layers_the_profiler_counts_the_stacks_bottom_instructions() {
    let qodec = c4c6();
    let program = c4c6_program(&qodec);
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let (profile, _): (Profile, _) =
        report_of!(qodec, program, top.adapter() => middle.adapter() => profiler());

    assert_eq!(profile, stacks_bottom_profile(&qodec, &program));
}

/// What a profiler above the layers counts of `program` on shots 0 to 3:
/// each of its batches on each shot, and each batch's instructions.
fn programs_profile(program: &Program) -> Profile {
    let mut profile = Profile::default();
    for _ in CODE_SHOTS {
        for batch in &program.batches {
            profile.add(batch.iter().map(|call| call.opcode));
        }
    }
    profile
}

/// What the Qodec runtime's own stack sends its bottom backends, running
/// `program` on shots 0 to 3, as counted from the batches its backends
/// kept.
fn stacks_bottom_profile(qodec: &qodec::Qodec, program: &Program) -> Profile {
    let shots: Vec<_> = CODE_SHOTS.collect();
    let stack = on_the_stack(qodec, program, &shot_set(&shots), SEED).expect("the stack runs");
    let sent = stack.backends.iter().flat_map(|(_, coins)| coins.sent());
    let mut expected = Profile::default();
    for batch in sent {
        expected.batches += 1;
        for call in batch {
            *expected.counts.entry(call.opcode).or_default() += 1;
        }
    }
    assert!(expected.batches > 0, "the stack ran");
    expected
}

/// c422's program, its CCZs and teleports, profiled below its one layer:
/// the profiler counts what the Qodec runtime's own stack sends its bottom
/// backends for it, batch by batch and instruction by instruction.
#[test]
fn below_c422s_layer_the_profiler_counts_the_stacks_bottom_instructions() {
    let qodec = c422();
    let program = c422_program(&qodec);
    let layer = QodecLayer::new(&qodec, 0).expect("c422 plans");
    let (profile, _): (Profile, _) = report_of!(qodec, program, layer.adapter() => profiler());

    assert_eq!(profile, stacks_bottom_profile(&qodec, &program));
}

/// c422's program profiled above its layer: the profiler counts the
/// program's logical instructions, in the program's batches, on each shot.
#[test]
fn above_c422s_layer_the_profiler_counts_the_programs_instructions() {
    let qodec = c422();
    let program = c422_program(&qodec);
    let layer = QodecLayer::new(&qodec, 0).expect("c422 plans");
    let (profile, _): (Profile, _) = report_of!(qodec, program, profiler() => layer.adapter());

    assert_eq!(profile, programs_profile(&program));
}

/// c4c6's program, ten idles, profiled above both its layers: the profiler
/// counts the program's logical instructions, in the program's batches,
/// on each shot.
#[test]
fn above_two_layers_the_profiler_counts_the_programs_instructions() {
    let qodec = c4c6();
    let program = c4c6_program(&qodec);
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let (profile, _): (Profile, _) =
        report_of!(qodec, program, profiler() => top.adapter() => middle.adapter());

    assert_eq!(profile, programs_profile(&program));
}

/// c4c6 profiled above both layers and below them in the same chain, as a
/// user comparing a program's logical and physical cost does: each
/// profiler counts its own place's instructions, the one above the
/// program's, the one below the stack's bottom ones.
#[test]
fn two_profilers_in_one_chain_each_count_their_own_places_instructions() {
    let qodec = c4c6();
    let program = c4c6_program(&qodec);
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let (above, (below, _)): (Profile, (Profile, _)) = report_of!(
        qodec, program, profiler() => top.adapter() => middle.adapter() => profiler()
    );

    assert_eq!(above, programs_profile(&program), "above the layers");
    assert_eq!(
        below,
        stacks_bottom_profile(&qodec, &program),
        "below the layers"
    );
}

/// Bottom batches replayed as calls, each on its shots, in order: a
/// program that groups the same batches as it chooses.
struct Replayed {
    set: Arc<InstructionSet>,
    calls: Vec<(Vec<ShotId>, Vec<InstructionCall>)>,
}

impl executor_protocol::Program for Replayed {
    type Instruction = InstructionCall;
    type Output = Vec<ShotOutcome>;

    fn needs(&self) -> Needs {
        Needs::default().with_instruction_set(Some(Arc::clone(&self.set)))
    }
}

/// The driver of a [`Replayed`]: makes its calls in order, each on its
/// shots.
struct ReplayedDriver;

impl Driver<Replayed> for ReplayedDriver {
    async fn drive<E: Executor<InstructionBatch<InstructionCall>>>(
        &self,
        program: Replayed,
        executor: &mut E,
        _: &ShotSet,
    ) -> Vec<ShotOutcome> {
        let mut outcomes = Vec::new();
        for (shots, instructions) in &program.calls {
            let shots = shot_set(shots);
            let batch = InstructionBatchRef {
                shots: &shots,
                instructions,
            };
            let reply = executor.call(batch).expect("the profiler takes the batch");
            wait(reply, &mut outcomes).expect("the coins run the batch");
        }
        outcomes
    }
}

/// Each shot's `batches`, as calls: the `i`th batches of all shots before
/// the `(i+1)`th, each on the shots it is equal on (`together`) or each on
/// its shot alone.
fn calls_of(
    batches: &[(ShotId, &[Vec<InstructionCall>])],
    together: bool,
) -> Vec<(Vec<ShotId>, Vec<InstructionCall>)> {
    let most = batches.iter().map(|(_, b)| b.len()).max().unwrap_or(0);
    let mut calls: Vec<(Vec<ShotId>, Vec<InstructionCall>)> = Vec::new();
    for i in 0..most {
        let first = calls.len();
        for (shot, batches) in batches {
            let Some(batch) = batches.get(i) else {
                continue;
            };
            let equal =
                (together.then(|| calls[first..].iter_mut().find(|(_, b)| b == batch))).flatten();
            match equal {
                Some((shots, _)) => shots.push(*shot),
                None => calls.push((vec![*shot], batch.clone())),
            }
        }
    }
    calls
}

/// The profile of `ReplayedDriver.into_chain() >> profiler() >> coins`
/// running `calls`.
fn profile_of_calls(
    qodec: &qodec::Qodec,
    shots: &[ShotId],
    calls: Vec<(Vec<ShotId>, Vec<InstructionCall>)>,
) -> Profile {
    let set = bottom(qodec).set.clone();
    let coins = coin_device(Arc::clone(&set), SEED);
    let chain = (ReplayedDriver.into_chain() >> profiler() >> coins).expect("the profiler fits");
    let (_, (profile, _)): (_, (Profile, _)) = chain
        .run(Replayed { set, calls }, shot_set(shots))
        .expect("the job fits the chain")
        .wait();
    profile
}

/// c4c6 on 4 shots: the bottom batches the Qodec runtime's own stack
/// sends, given to the profiler a shot to a call (276 calls), or each to
/// all the shots it is equal on (204 calls), or by the layers themselves:
/// the profiler counts each shot's calls, so its totals are the same. The
/// layers send each of c4c6's bottom batches on its shot alone
/// (`below_c4c6_every_bottom_batch_goes_below_on_its_shot_alone`), so
/// their grouping is the first again: two distinct groupings are shown.
#[test]
fn the_profilers_totals_do_not_depend_on_how_the_calls_below_qodec_are_grouped() {
    let qodec = c4c6();
    let program = c4c6_program(&qodec);
    let shots: Vec<_> = CODE_SHOTS.collect();
    let stack = on_the_stack(&qodec, &program, &shot_set(&shots), SEED).expect("the stack runs");
    let batches: Vec<_> = (stack.backends.iter())
        .map(|(shot, coins)| (*shot, coins.sent()))
        .collect();
    let alone = calls_of(&batches, false);
    let together = calls_of(&batches, true);
    assert_eq!(alone.len(), 276, "69 batches a shot");
    assert_eq!(together.len(), 204, "equal batches of several shots");
    let top = QodecLayer::new(&qodec, 0).expect("c4c6 plans");
    let middle = QodecLayer::new(&qodec, 1).expect("c4c6 plans");
    let coins = coin_device(bottom(&qodec).set.clone(), SEED);
    let chain = (static_driver() >> top.adapter() >> middle.adapter() >> profiler() >> coins)
        .expect("every boundary matches");
    let (_, (by_the_layers, _)): (_, (Profile, _)) = chain
        .run(program.to_static(Needs::default()), shot_set(&shots))
        .expect("the job fits the chain")
        .wait();

    let one_shot_a_call = profile_of_calls(&qodec, &shots, alone);
    let grouped = profile_of_calls(&qodec, &shots, together);
    assert_eq!(grouped, one_shot_a_call, "grouped");
    assert_eq!(by_the_layers, one_shot_a_call, "by the layers");
}

/// How long a reply below the profiler waits for the run to yield before
/// it is answered anyway: long, since the run yields for each of them, so
/// that only a reply held on the polling thread could reach it, never one
/// whose run was merely not scheduled for a while.
const YIELD_TIMEOUT: Duration = Duration::from_secs(5);

/// Over the profiler alone, the run, awaited, yields while its calls below
/// are pending, so a remote device answers them with the thread free: the
/// profiler keeps the chain's non-synchronous path.
#[test]
fn awaited_over_the_profiler_no_reply_below_waits_on_the_thread_that_polls_the_run() {
    let gate = bounded(|| {
        let set = stim_rz().set;
        let call =
            |m, operands: &[Operand]| set.call(m, operands.iter().copied()).expect("stim+rz");
        let read = |tag| {
            vec![
                call("R", &[Operand::Block(0)]),
                call("M", &[Operand::Block(0), Operand::Tag(tag)]),
            ]
        };
        let needs = Needs::default().with_instruction_set(Some(Arc::clone(&set)));
        let program = StaticProgram::new(needs, vec![read(0), read(1)]);
        let gate = YieldGate::new(YIELD_TIMEOUT);
        let coins = AnswersOnYield::new(coin_device(Arc::clone(&set), SEED), Arc::clone(&gate));
        let chain = (static_driver() >> profiler() >> coins).expect("the profiler fits");
        let run = chain
            .run(program, shot_set(&SHOTS))
            .expect("the job fits the chain");
        let (outcomes, _) = gate.await_run(run);
        outcomes.expect("the run succeeds");
        gate
    });
    assert!(
        gate.replies() > 0,
        "precondition: the profiler called below"
    );
    assert_eq!(gate.timed_out(), 0);
}
