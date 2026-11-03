// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::rc::Rc;
use std::sync::Arc;

use crate::OperandKind::{Block, Tag};
use crate::tests::test_double::{TestDoubleExecutor, TestDoubleModel};
use crate::tests::{
    Batches, Bits, ByHand, Count, EVERY_BIT, Relabel, bits_set, block_on, declaring, flip_and_toss,
    resetting_bits_set, reversed_set, shots, values,
};
use crate::{
    Action, CallInput, Capabilities, Chain, ChainPrefix, Driver, DriverExt, Executor, ExecutorFn,
    ExecutorLogic, Instruction, InstructionBatch, InstructionBatchRef, InstructionCall,
    InstructionMismatch, InstructionSet, JobFaults, MakeState, Need, Needs, NoAdapter, Opcode,
    Operand, Outcomes, Pauli, Program, Rejected, ShotId, ShotSet, StaticDriver, StaticProgram,
    Value, executor_fn, static_driver, wait_async,
};

/// [`flip_and_toss`] as a static program in `set`'s numbering, needing
/// that set, and `qubits` when stated.
fn flip_and_toss_in(set: InstructionSet, qubits: Option<u32>) -> StaticProgram<InstructionCall> {
    let batches = flip_and_toss(&set);
    let needs = Needs {
        qubits,
        ..Needs::default().with_instruction_set(Some(Arc::new(set)))
    };
    StaticProgram::new(needs, batches)
}

/// A static program of one read, for the test double, which reads a bit
/// of each shot's seed.
fn one_read() -> StaticProgram<bool> {
    StaticProgram::new(Needs::default(), vec![vec![true]])
}

/// The unmet need of a rejection, if it is one.
fn unmet(rejected: &Rejected) -> Option<(Need, u64, u64)> {
    match rejected {
        Rejected::Unmet(unmet) => Some((unmet.need, unmet.needed, unmet.offered)),
        _ => None,
    }
}

/// A chain of the static driver on the test double, which holds at
/// most `max_shots` shots at once when stated.
fn double_chain(
    max_shots: Option<u64>,
) -> Chain<StaticDriver, bool, TestDoubleExecutor<TestDoubleModel>> {
    let model = TestDoubleModel {
        max_shots,
        ..TestDoubleModel::default()
    };
    (static_driver() >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s")
}

/// A sampler's job of no shots (an empty range) is refused when given to
/// the chain, naming the shots as its fault: a refused job gives no
/// [`Run`](crate::Run), so nothing of it runs.
#[test]
fn a_job_of_no_shots_is_refused() {
    let Err(rejected) = double_chain(None).run(one_read(), 0..0) else {
        panic!("no shots must be refused");
    };
    assert_eq!(rejected, JobFaults(vec![Rejected::NoShots]));
}

/// A job that names shot 3 twice is refused when given to the chain,
/// naming that shot, whether its shots are written as a `Vec` or an array,
/// owned or borrowed, or as a slice.
#[test]
fn a_job_that_names_a_shot_twice_is_refused() {
    let named_twice = JobFaults(vec![Rejected::RepeatedShot(3)]);
    let ids: [ShotId; 3] = [3, 5, 3];
    let list = ids.to_vec();
    let forms = [
        ("Vec", double_chain(None).run(one_read(), ids.to_vec())),
        ("borrowed Vec", double_chain(None).run(one_read(), &list)),
        ("slice", double_chain(None).run(one_read(), ids.as_slice())),
        ("array", double_chain(None).run(one_read(), ids)),
        ("borrowed array", double_chain(None).run(one_read(), &ids)),
    ];
    for (written, given) in forms {
        let Err(rejected) = given else {
            panic!("a shot named twice, as a {written}, must be refused");
        };
        assert_eq!(rejected, named_twice, "as a {written}");
    }
}

/// A caller that keeps its shots, to compare later runs on them, lends
/// them, in no sorted order: as an array, a `Vec`, a slice or a set,
/// borrowed, they give the outcomes that the same shots give owned, in
/// the caller's order.
#[test]
fn borrowed_shots_give_what_owned_shots_give() {
    let chain = || {
        (static_driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
            .expect("the bits run calls by opcode")
    };
    let job = || flip_and_toss_in(bits_set(), None);
    let ids: [ShotId; 4] = [11, 2, 6, 5];
    let (list, set) = (ids.to_vec(), ShotSet::new(ids.to_vec()).expect("distinct"));
    let owned = chain().run(job(), ids).expect("shots, needs met").wait().0;
    let lent = [
        ("array", chain().run(job(), &ids)),
        ("Vec", chain().run(job(), &list)),
        ("slice", chain().run(job(), &ids[..])),
        ("set", chain().run(job(), &set)),
    ];
    for (form, given) in lent {
        let (outcomes, _) = given.expect("shots, needs met").wait();
        assert_eq!(outcomes, owned, "lent as a {form}");
    }
}

/// A job that names shot 1 twice among four ids, on an executor that holds
/// three shots at once: only the repeat is reported, since shots that are
/// not a set are not counted against `max_shots` (the four ids would be
/// one too many).
#[test]
fn only_shots_that_are_a_set_are_counted_against_max_shots() {
    let Err(rejected) = double_chain(Some(3)).run(one_read(), [1, 1, 2, 3]) else {
        panic!("a repeated shot must be refused");
    };
    assert_eq!(rejected, JobFaults(vec![Rejected::RepeatedShot(1)]));
}

/// A job that names shots 5 and 3 twice each, among six ids: its one shot
/// fault names the least of them, shot 3, as `ShotSet::new` does.
#[test]
fn a_job_that_names_several_shots_twice_reports_the_least() {
    let Err(rejected) = double_chain(Some(3)).run(one_read(), [5, 3, 5, 3, 7, 8]) else {
        panic!("repeated shots must be refused");
    };
    assert_eq!(rejected, JobFaults(vec![Rejected::RepeatedShot(3)]));
}

/// A job of no shots whose program needs more qubits than the bits have
/// and calls an opcode outside its set: every fault is reported at once,
/// the shots' first, then the unmet need, then the call, so that all are
/// fixed in one go.
#[test]
fn a_jobs_shot_faults_are_reported_with_its_programs_needs_and_calls() {
    let chain = (static_driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let mut program = flip_and_toss(&bits_set());
    program[1].push(InstructionCall::new(Opcode::new(3), [Operand::Block(0)]));
    let needs = Needs::default()
        .with_instruction_set(Some(Arc::new(bits_set())))
        .with_qubits(3);
    let Err(JobFaults(faults)) = chain.run(StaticProgram::new(needs, program), 0..0) else {
        panic!("no shots, three qubits on two bits and opcode 3 must be refused");
    };
    let [shots, qubits, call] = faults.as_slice() else {
        panic!("three faults: {faults:?}");
    };
    assert_eq!(shots, &Rejected::NoShots);
    assert_eq!(unmet(qubits), Some((Need::Qubits, 3, 2)), "{faults:?}");
    assert!(
        matches!(call, Rejected::Instruction { at: 2, reason } if reason.starts_with("batch 1:")),
        "{faults:?}"
    );
}

/// The faults of a job of no shots whose program needs more qubits than
/// the bits have read as one message, each as its own text, in their
/// order, joined by "; ", as a chain's build faults do.
#[test]
fn a_jobs_faults_read_as_one_message_in_their_order() {
    let chain = (static_driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let program = flip_and_toss_in(bits_set(), Some(3));
    let Err(faults) = chain.run(program, 0..0) else {
        panic!("no shots and three qubits on two bits must be refused");
    };
    let [shots, qubits] = faults.0.as_slice() else {
        panic!("two faults: {faults:?}");
    };
    assert_eq!(faults.to_string(), format!("{shots}; {qubits}"));
}

/// A job whose program of calls by opcode states no set, run by the
/// static driver, which adds none, on the bits: the refusal names the
/// program and its driver, the two sides that may state the set.
#[test]
fn a_jobs_program_of_calls_by_opcode_stating_no_set_is_named_in_its_refusal() {
    let chain = (static_driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let program = StaticProgram::new(Needs::default(), flip_and_toss(&bits_set()));
    let Err(JobFaults(faults)) = chain.run(program, 0..1) else {
        panic!("a program of calls by opcode that states no set must be refused");
    };
    let [Rejected::Unmet(unmet)] = faults.as_slice() else {
        panic!("one unmet need: {faults:?}");
    };
    assert_eq!(unmet.need, Need::InstructionSet, "{unmet}");
    let text = unmet.to_string();
    assert!(
        text.starts_with("neither the program nor its driver "),
        "{text}"
    );
}

/// A job of four shots whose program of calls by opcode states no set and
/// needs three qubits and the log probability, run by the static driver on
/// two bits that hold three shots at once and give no log probability: the
/// undescribed set is reported in the instruction set's place, in the order
/// of `Capabilities::admit`, after the qubits and the log probability and
/// before the shots, so that all are fixed in one go.
#[test]
fn an_undescribed_set_is_reported_in_the_instruction_sets_place() {
    let bits = Bits {
        max_shots: Some(3),
        ..Bits::default()
    };
    let chain = (static_driver() >> TestDoubleExecutor::with_model(bits, 7))
        .expect("the bits run calls by opcode");
    let needs = Needs::default().with_qubits(3).with_log_probability(true);
    let program = StaticProgram::new(needs, flip_and_toss(&bits_set()));
    let Err(JobFaults(faults)) = chain.run(program, 0..4) else {
        panic!("the qubits, the log probability, no set and four shots must be refused");
    };
    let faults: Vec<_> = (faults.iter())
        .map(|fault| match fault {
            Rejected::Unmet(unmet) => (
                unmet.need,
                unmet.needed,
                unmet.offered,
                unmet.mismatches.len(),
            ),
            other => panic!("the job's faults are its unmet needs: {other:?}"),
        })
        .collect();
    assert_eq!(
        faults,
        [
            (Need::Qubits, 3, 2, 0),
            (Need::LogProbability, 1, 0, 0),
            (Need::InstructionSet, 1, 0, 0),
            (Need::Shots, 4, 3, 0)
        ]
    );
}

/// A job of four shots on an executor that holds three shots at once is
/// refused when given to the chain, naming the shots it holds
/// (`Need::Shots`).
#[test]
fn a_job_of_more_shots_than_the_executor_holds_is_refused() {
    let Err(JobFaults(faults)) = double_chain(Some(3)).run(one_read(), 0..4) else {
        panic!("four shots on three at once must be refused");
    };
    let faults: Vec<_> = faults.iter().map(unmet).collect();
    assert_eq!(faults, [Some((Need::Shots, 4, 3))]);
}

/// A job's run gives the program's outcomes and, with them, the report of
/// the executor it closed: the two calls of the job, on its three shots,
/// each of which tossed one coin.
#[test]
fn a_jobs_run_gives_its_outcomes_with_the_executors_report() {
    let bits = Bits {
        log_probability: true,
        ..Bits::default()
    };
    let chain = (static_driver() >> TestDoubleExecutor::with_model(bits, 7))
        .expect("the bits run calls by opcode");
    let (outcomes, report) = chain
        .run(flip_and_toss_in(bits_set(), None), [2, 5, 11])
        .expect("shots, needs met")
        .wait();
    let outcomes = outcomes.expect("the bits neither refuse nor fail");
    assert_eq!(outcomes.len(), 6, "a readout per shot per batch");
    assert_eq!(report.calls, 2, "{report:?}");
    assert_eq!(report.states, Count(3), "{report:?}");
    let ln_half = 0.5_f64.ln();
    assert_eq!(
        [2, 5, 11].map(|j| report.log_p.get(j)),
        [Some(ln_half); 3],
        "{report:?}"
    );
}

/// The chain built again by the function that built it, on bits that
/// offer replayable shots, runs a shot again: shot 6, run alone, gives the
/// outcomes it gave among other shots, its coin included, as each shot
/// draws from its own seed.
#[test]
fn a_replayable_chain_built_again_runs_a_shot_again_with_the_same_outcomes() {
    let chain = || {
        (static_driver()
            >> Relabel::new(reversed_set(), false)
            >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the chain runs the bits' set")
    };
    let run = |shots: &[u32]| {
        let (outcomes, _) = chain()
            .run(flip_and_toss_in(bits_set(), None), shots.to_vec())
            .expect("shots, needs met")
            .wait();
        values(&outcomes.expect("the bits neither refuse nor fail"))
    };
    let among_others: Vec<_> = (run(&[2, 5, 6]).into_iter())
        .filter(|&(shot, _, _)| shot == 6)
        .collect();
    assert_eq!(among_others.len(), 2, "shot 6 read twice: {among_others:?}");
    assert_eq!(run(&[6]), among_others);
}

/// A job's program states its own numbering: one in the reversed set's
/// numbering runs its own instructions on the bits, its first readout one
/// as flipped, and gives what the job in the bits' numbering gives.
#[test]
fn a_job_is_renumbered_into_the_chains_set_by_its_programs_numbering() {
    let run = |set: InstructionSet| {
        let chain = (static_driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
            .expect("the bits run calls by opcode");
        let (outcomes, _) = chain
            .run(flip_and_toss_in(set, None), [2, 3])
            .expect("the bits have every instruction of either set")
            .wait();
        outcomes.expect("the bits neither refuse nor fail")
    };
    let (reversed, in_bits) = (run(reversed_set()), run(bits_set()));
    assert_eq!(values(&reversed), values(&in_bits));
    assert!(
        values(&reversed[..2])
            .iter()
            .all(|&(_, _, v)| v == Value::One),
        "{reversed:?}"
    );
}

/// A program of one read of every shot of its job, with no content: its
/// driver sends the read. It gives whether the call was taken.
struct OneRead;

impl Program for OneRead {
    type Instruction = bool;
    type Output = Result<(), Rejected>;
}

/// A driver that reads shot 0 whatever the job's shots: a bug.
struct ReadsShotZero;

impl Driver<OneRead> for ReadsShotZero {
    async fn drive<E>(&self, _: OneRead, executor: &mut E, _: &ShotSet) -> Result<(), Rejected>
    where
        E: Executor<InstructionBatch<bool>>,
    {
        let reply = executor.call(InstructionBatchRef {
            shots: &shots(&[0]),
            instructions: &[true],
        })?;
        wait_async(reply).await.1.expect("the double does not fail");
        Ok(())
    }
}

/// A job calls only its own shots: one of shot 1 whose driver calls
/// shot 0 is refused before the executor sees the call, which makes no
/// shot.
#[test]
fn a_job_that_calls_a_shot_not_its_own_is_refused() {
    let model = TestDoubleModel::default();
    let made = Rc::clone(&model.made);
    let chain = (ReadsShotZero.into_chain() >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let (output, _) = chain.run(OneRead, [1]).expect("a shot").wait();
    assert_eq!(output, Err(Rejected::ShotNotHeld(0)));
    assert_eq!(made.get(), 0, "the job reached shot 0");
}

/// A job awaited, on the caller's executor of futures, gives what it gives
/// waited for.
#[test]
fn a_job_awaited_gives_what_it_gives_waited_for() {
    let chain = || {
        (static_driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
            .expect("the bits run calls by opcode")
    };
    let job = || flip_and_toss_in(bits_set(), None);
    let (awaited, _) = block_on(async { chain().run(job(), [2, 5]).expect("shots").await });
    let (waited, _) = chain().run(job(), [2, 5]).expect("shots").wait();
    assert_eq!(
        values(&awaited.expect("the bits neither refuse nor fail")),
        values(&waited.expect("the bits neither refuse nor fail"))
    );
}

/// `static_driver() >> adapter >> executor`, then the job: a static program
/// run through an adapter that renumbers its calls on the bits gives the
/// outcomes and the report that the same batches give on the same chain
/// headed by a driver written by hand, which sends them in turn.
#[test]
fn a_chain_runs_a_static_program_as_a_driver_written_by_hand_runs_its_batches() {
    let relabel = || Relabel::new(reversed_set(), false);
    let bits = || TestDoubleExecutor::with_model(Bits::default(), 7);
    let program = flip_and_toss_in(bits_set(), None);
    let chain = (static_driver() >> relabel() >> bits()).expect("the chain runs the bits' set");
    let (from_static, from_static_report) = chain
        .run(program, [2, 5, 6, 11])
        .expect("shots, needs met")
        .wait();
    let batches = Batches {
        needs: Needs::default().with_instruction_set(Some(Arc::new(bits_set()))),
        batches: flip_and_toss(&bits_set()),
    };
    let (by_hand, by_hand_report) = (ByHand.into_chain() >> relabel() >> bits())
        .expect("the chain runs the bits' set")
        .run(batches, [2, 5, 6, 11])
        .expect("shots, needs met")
        .wait();
    assert_eq!(
        from_static.expect("the bits neither refuse nor fail"),
        by_hand
    );
    assert_eq!(from_static_report, by_hand_report);
}

/// A job whose program needs three qubits and the log probability, through an
/// adapter onto the two bits, which give no log probability: each unmet need
/// is reported, in the order `Capabilities::admit` checks them, naming the
/// chain's executor, adapters included.
#[test]
fn a_jobs_unmet_needs_are_each_reported_in_order_naming_the_chains_executor() {
    let relabel = Relabel::new(reversed_set(), false);
    let bits = TestDoubleExecutor::with_model(Bits::default(), 7);
    let needs = Needs::default()
        .with_instruction_set(Some(Arc::new(bits_set())))
        .with_qubits(3)
        .with_log_probability(true);
    let program = StaticProgram::new(needs, flip_and_toss(&bits_set()));
    let chain = (static_driver() >> relabel >> bits).expect("the chain runs the bits' set");
    let Err(JobFaults(faults)) = chain.run(program, [2, 5]) else {
        panic!("three qubits and the log probability on two bits must be refused");
    };
    let unmet: Vec<_> = (faults.iter())
        .map(|fault| match fault {
            Rejected::Unmet(unmet) => (unmet.need, unmet.component.as_str()),
            other => panic!("the job's faults are its unmet needs: {other:?}"),
        })
        .collect();
    let needs: Vec<Need> = unmet.iter().map(|&(need, _)| need).collect();
    assert_eq!(needs, [Need::Qubits, Need::LogProbability]);
    for (_, component) in unmet {
        assert!(
            component.contains("Relabel") && component.contains("TestDoubleExecutor"),
            "{component}"
        );
    }
}

/// A runtime that runs a static program as the static driver does, but
/// whose way of sending needs what it holds, whatever the program states.
struct AddsNeeds(Needs);

impl<I: Instruction> Driver<StaticProgram<I>> for AddsNeeds {
    fn needs(&self, _: &StaticProgram<I>) -> Needs {
        self.0.clone()
    }

    fn check(&self, program: &StaticProgram<I>) -> Vec<Rejected> {
        StaticDriver.check(program)
    }

    async fn drive<E>(
        &self,
        program: StaticProgram<I>,
        executor: &mut E,
        shots: &ShotSet,
    ) -> <StaticProgram<I> as Program>::Output
    where
        E: Executor<InstructionBatch<I>>,
    {
        StaticDriver.drive(program, executor, shots).await
    }
}

/// `AddsNeeds(needs)` as a chain's prefix, driving static programs of `I`s.
fn adding<I: Instruction>(needs: Needs) -> ChainPrefix<AddsNeeds, NoAdapter<I>> {
    DriverExt::<StaticProgram<I>>::into_chain(AddsNeeds(needs))
}

/// A sampler of `bool`s that offers none of the needs a flag states.
fn offering_nothing() -> ExecutorFn<bool, (), impl MakeState<()>, impl ExecutorLogic<bool, ()>> {
    executor_fn(
        Capabilities::default(),
        |_| Ok(()),
        |(): &mut (), _: &CallInput<'_, bool>, _: &mut Outcomes<'_>| Ok(()),
    )
}

/// Each need a flag states, alone, with the need it is reported as.
fn each_flag() -> [(Needs, Need); 4] {
    let none = Needs::default();
    [
        (
            none.clone().with_resumable_shots(true),
            Need::ResumableShots,
        ),
        (none.clone().with_resets(true), Need::Resets),
        (none.clone().with_replayable(true), Need::Replayable),
        (none.with_log_probability(true), Need::LogProbability),
    ]
}

/// A runtime that adds a need to a program of one read stating none, on a
/// sampler that offers none: the job is refused for that need, as if the
/// program stated it, whichever flag it is.
#[test]
fn each_need_a_driver_adds_is_checked_with_its_programs() {
    for (added, need) in each_flag() {
        let chain =
            (adding(added) >> offering_nothing()).expect("the sampler takes any batch of `bool`s");
        let Err(JobFaults(faults)) = chain.run(one_read(), [4]) else {
            panic!("{need:?}, added by the driver, must be refused");
        };
        let faults: Vec<_> = faults.iter().map(unmet).collect();
        assert_eq!(faults, [Some((need, 1, 0))], "{need:?}");
    }
}

/// A program of one read that states a need, driven by a runtime that adds
/// none, on a sampler that offers none: the job is refused for the
/// program's need, since a driver cannot drop one, whichever flag it is.
#[test]
fn each_need_a_program_states_is_checked_whatever_its_driver_adds() {
    for (stated, need) in each_flag() {
        let chain = (adding(Needs::default()) >> offering_nothing())
            .expect("the sampler takes any batch of `bool`s");
        let program = StaticProgram::new(stated, vec![vec![true]]);
        let Err(JobFaults(faults)) = chain.run(program, [4]) else {
            panic!("{need:?}, stated by the program, must be refused");
        };
        let faults: Vec<_> = faults.iter().map(unmet).collect();
        assert_eq!(faults, [Some((need, 1, 0))], "{need:?}");
    }
}

/// A program of one read that states a need, driven by a runtime whose way
/// of sending needs it too, on a sampler that offers none: the job is
/// refused for that need, stated on both sides, whichever flag it is.
#[test]
fn each_need_both_the_program_and_its_driver_state_is_checked() {
    for (stated, need) in each_flag() {
        let chain = (adding(stated.clone()) >> offering_nothing())
            .expect("the sampler takes any batch of `bool`s");
        let program = StaticProgram::new(stated, vec![vec![true]]);
        let Err(JobFaults(faults)) = chain.run(program, [4]) else {
            panic!("{need:?}, stated by the program and its driver, must be refused");
        };
        let faults: Vec<_> = faults.iter().map(unmet).collect();
        assert_eq!(faults, [Some((need, 1, 0))], "{need:?}");
    }
}

/// A job on the two bits whose program and driver state qubits, one side
/// three, the other one or none, in either order: it is refused for the
/// three, the larger, wherever it is stated.
#[test]
fn a_job_needs_the_larger_qubits_its_program_and_driver_state() {
    let cases = [
        (Some(3), Some(1)),
        (Some(1), Some(3)),
        (Some(3), None),
        (None, Some(3)),
    ];
    for (stated, added) in cases {
        let chain = (adding(Needs {
            qubits: added,
            ..Needs::default()
        }) >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
        let Err(JobFaults(faults)) = chain.run(flip_and_toss_in(bits_set(), stated), [2]) else {
            panic!("three qubits on two bits must be refused: {stated:?}, {added:?}");
        };
        let faults: Vec<_> = faults.iter().map(unmet).collect();
        assert_eq!(
            faults,
            [Some((Need::Qubits, 3, 2))],
            "program {stated:?}, driver {added:?}"
        );
    }
}

/// A job on the two bits whose program and driver state the same qubits, as
/// a runtime that restates its program's count does: two are met, and three
/// are refused for the three, so a count both sides state is the job's.
#[test]
fn a_job_needs_the_qubits_its_program_and_driver_both_state() {
    let run = |qubits: u32| {
        let chain = (adding(Needs::default().with_qubits(qubits))
            >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
        chain.run(flip_and_toss_in(bits_set(), Some(qubits)), [2])
    };
    run(2)
        .expect("two qubits, stated by both, on two bits")
        .wait()
        .0
        .expect("the bits neither refuse nor fail");
    let Err(JobFaults(faults)) = run(3) else {
        panic!("three qubits, stated by both, on two bits must be refused");
    };
    let faults: Vec<_> = faults.iter().map(unmet).collect();
    assert_eq!(faults, [Some((Need::Qubits, 3, 2))]);
}

/// A job whose calls are in the reversed set's numbering, that set stated
/// by its program, by its driver, or by both: it runs on the bits, and
/// gives what the job gives in the bits' numbering, so a set stated on one
/// side, or the same on both, is the job's.
#[test]
fn a_set_stated_by_one_side_or_the_same_by_both_numbers_the_jobs_calls() {
    let reversed = || Some(Arc::new(reversed_set()));
    let run = |stated: Option<Arc<InstructionSet>>, added: Option<Arc<InstructionSet>>| {
        let chain = (adding(Needs::default().with_instruction_set(added))
            >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
        let program = StaticProgram::new(
            Needs::default().with_instruction_set(stated),
            flip_and_toss(&reversed_set()),
        );
        let (outcomes, _) = chain
            .run(program, [2, 3])
            .expect("the bits have every instruction of the reversed set")
            .wait();
        values(&outcomes.expect("the bits neither refuse nor fail"))
    };
    let in_bits = {
        let chain = (static_driver() >> TestDoubleExecutor::with_model(Bits::default(), 7))
            .expect("the bits run calls by opcode");
        let (outcomes, _) = (chain.run(flip_and_toss_in(bits_set(), None), [2, 3]))
            .expect("the bits have every instruction of their set")
            .wait();
        values(&outcomes.expect("the bits neither refuse nor fail"))
    };
    let sides = [
        ("program", reversed(), None),
        ("driver", None, reversed()),
        ("both", reversed(), reversed()),
    ];
    for (stated_by, stated, added) in sides {
        assert_eq!(run(stated, added), in_bits, "stated by the {stated_by}");
    }
}

/// A job of no shots, three qubits on the two bits, whose program states
/// the bits' set and whose driver states the reversed set: the different
/// sets are one fault of the job, naming the driver, reported after the
/// shots' fault and before the unmet needs, so that all are fixed in one
/// go.
#[test]
fn a_job_whose_program_and_driver_state_different_sets_is_refused() {
    let added = Needs::default().with_instruction_set(Some(Arc::new(reversed_set())));
    let chain = (adding(added) >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(flip_and_toss_in(bits_set(), Some(3)), 0..0) else {
        panic!("no shots, different sets and three qubits must be refused");
    };
    let [shots, Rejected::Unmet(sets), qubits] = faults.as_slice() else {
        panic!("the shots, the sets, then the qubits: {faults:?}");
    };
    assert_eq!(shots, &Rejected::NoShots);
    assert_eq!(
        (sets.need, sets.needed, sets.offered, sets.mismatches.len()),
        (Need::InstructionSet, 2, 1, 0),
        "{sets}"
    );
    assert!(sets.component.contains("AddsNeeds"), "{sets}");
    assert!(
        sets.to_string()
            .contains("state different instruction sets"),
        "{sets}"
    );
    assert_eq!(unmet(qubits), Some((Need::Qubits, 3, 2)), "{faults:?}");
}

/// A job whose program states the bits' set and also calls opcode 3,
/// outside it, driven by a runtime that states the reversed set: the
/// different sets and the call its driver refuses are both reported, the
/// sets first, so that a conflict of sets hides none of the program's
/// faults.
#[test]
fn different_sets_do_not_hide_the_faults_the_driver_finds_in_the_program() {
    let added = Needs::default().with_instruction_set(Some(Arc::new(reversed_set())));
    let chain = (adding(added) >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let mut batches = flip_and_toss(&bits_set());
    batches[1].push(InstructionCall::new(Opcode::new(3), [Operand::Block(0)]));
    let needs = Needs::default().with_instruction_set(Some(Arc::new(bits_set())));
    let Err(JobFaults(faults)) = chain.run(StaticProgram::new(needs, batches), [2]) else {
        panic!("different sets and opcode 3 must be refused");
    };
    let [Rejected::Unmet(sets), call] = faults.as_slice() else {
        panic!("the sets, then the call: {faults:?}");
    };
    assert_eq!(
        (sets.need, sets.needed, sets.offered),
        (Need::InstructionSet, 2, 1),
        "{sets}"
    );
    assert!(
        matches!(call, Rejected::Instruction { at: 2, reason } if reason.starts_with("batch 1:")),
        "{faults:?}"
    );
}

/// A job whose program states the bits' set, driven by a runtime that
/// states a set also named "bits", of the same instructions in another
/// order: the two number the calls otherwise, so they are different sets,
/// refused as such, naming the driver, whatever their name.
#[test]
fn sets_of_one_name_in_another_order_are_different_sets() {
    let reordered = InstructionSet::new("bits")
        .with("measure", [Block, Tag])
        .with("x", [Block])
        .with("coin", [Block]);
    let reordered = declaring(reordered, EVERY_BIT);
    let added = Needs::default().with_instruction_set(Some(Arc::new(reordered)));
    let chain = (adding(added) >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(flip_and_toss_in(bits_set(), None), [2]) else {
        panic!("the bits' set in two orders must be refused");
    };
    let [Rejected::Unmet(sets)] = faults.as_slice() else {
        panic!("one fault, the sets: {faults:?}");
    };
    assert_eq!(
        (sets.need, sets.needed, sets.offered, sets.mismatches.len()),
        (Need::InstructionSet, 2, 1, 0),
        "{sets}"
    );
    assert!(sets.component.contains("AddsNeeds"), "{sets}");
}

/// A runtime that numbers its calls by the reversed set, and also needs
/// three qubits and resets, driving a program in the bits' set on the two
/// bits, which reset nothing: the different sets, the qubits and the resets
/// are all reported, so that a conflict of sets hides none of the needs the
/// driver adds.
#[test]
fn different_sets_do_not_hide_the_needs_the_driver_adds() {
    let added = Needs::default()
        .with_instruction_set(Some(Arc::new(reversed_set())))
        .with_qubits(3)
        .with_resets(true);
    let chain = (adding(added) >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let Err(JobFaults(faults)) = chain.run(flip_and_toss_in(bits_set(), None), [2]) else {
        panic!("different sets, three qubits and resets must be refused");
    };
    let faults: Vec<_> = faults.iter().map(unmet).collect();
    assert_eq!(
        faults,
        [
            Some((Need::InstructionSet, 2, 1)),
            Some((Need::Qubits, 3, 2)),
            Some((Need::Resets, 1, 0)),
        ]
    );
}

/// A program in the bits' set, or one stating an empty set and making no
/// call, driven by a runtime that states the bits' set with a reset
/// appended: the program's set numbers its calls as the driver's would,
/// yet the two are different sets, refused as such, naming the driver.
#[test]
fn a_programs_set_that_the_drivers_extends_is_a_different_set() {
    let extended = InstructionSet::new("bits")
        .with("coin", [Block])
        .with("x", [Block])
        .with("measure", [Block, Tag])
        .with("reset", [Block]);
    let extended = declaring(extended, &["coin", "x", "measure", "reset"]);
    let empty = Needs::default().with_instruction_set(Some(Arc::new(InstructionSet::new("bits"))));
    let programs = [
        ("the bits' set", flip_and_toss_in(bits_set(), None)),
        ("an empty set", StaticProgram::new(empty, vec![vec![]])),
    ];
    for (stated, program) in programs {
        let added = Needs::default().with_instruction_set(Some(Arc::new(extended.clone())));
        let chain = (adding(added) >> TestDoubleExecutor::with_model(Bits::default(), 7))
            .expect("the bits run calls by opcode");
        let Err(JobFaults(faults)) = chain.run(program, [2]) else {
            panic!("{stated}, extended by the driver's, must be refused");
        };
        let [Rejected::Unmet(sets)] = faults.as_slice() else {
            panic!("{stated}: one fault, the sets: {faults:?}");
        };
        assert_eq!(
            (sets.need, sets.needed, sets.offered),
            (Need::InstructionSet, 2, 1),
            "{stated}: {sets}"
        );
        assert!(sets.component.contains("AddsNeeds"), "{stated}: {sets}");
    }
}

/// A program in the bits' set, driven by a runtime whose set numbers every
/// call alike but differs otherwise: under another name, with the read
/// taking a block alone, or leaving the flip's action undeclared. Each is
/// a different set, refused as such, since a set is the same only when it
/// is the same in every part, not only in its numbering.
#[test]
fn sets_that_number_calls_alike_but_differ_otherwise_are_different_sets() {
    let renamed = InstructionSet::new("bits, renamed")
        .with("coin", [Block])
        .with("x", [Block])
        .with("measure", [Block, Tag]);
    let read_without_tag = Action::new([1], []).observe([Pauli::z(0)]);
    let other_operands = InstructionSet::new("bits")
        .with("coin", [Block])
        .with("x", [Block])
        .with_action("measure", vec![Block], read_without_tag);
    let undeclared_flip = InstructionSet::new("bits")
        .with("coin", [Block])
        .with("x", [Block])
        .with("measure", [Block, Tag]);
    let sets = [
        ("another name", declaring(renamed, EVERY_BIT)),
        (
            "another operand kind",
            declaring(other_operands, &["coin", "x"]),
        ),
        (
            "another action",
            declaring(undeclared_flip, &["coin", "measure"]),
        ),
    ];
    for (differing, set) in sets {
        let added = Needs::default().with_instruction_set(Some(Arc::new(set)));
        let chain = (adding(added) >> TestDoubleExecutor::with_model(Bits::default(), 7))
            .expect("the bits run calls by opcode");
        let Err(JobFaults(faults)) = chain.run(flip_and_toss_in(bits_set(), None), [2]) else {
            panic!("a set of {differing} must be refused");
        };
        let [Rejected::Unmet(sets)] = faults.as_slice() else {
            panic!("{differing}: one fault, the sets: {faults:?}");
        };
        assert_eq!(
            (sets.need, sets.needed, sets.offered),
            (Need::InstructionSet, 2, 1),
            "{differing}: {sets}"
        );
    }
}

/// A job whose program states a set with a reset, which the bits lack, and
/// whose driver states the bits' set: besides the different sets, the
/// program's set is checked against the chain, which reports the reset it
/// lacks, since the job's calls are numbered by the program's set.
#[test]
fn where_the_sets_differ_the_programs_is_checked_against_the_chain() {
    let added = Needs::default().with_instruction_set(Some(Arc::new(bits_set())));
    let chain = (adding(added) >> TestDoubleExecutor::with_model(Bits::default(), 7))
        .expect("the bits run calls by opcode");
    let program = flip_and_toss_in(resetting_bits_set(), None);
    let Err(JobFaults(faults)) = chain.run(program, [2]) else {
        panic!("different sets and a reset the bits lack must be refused");
    };
    let [Rejected::Unmet(sets), Rejected::Unmet(reset)] = faults.as_slice() else {
        panic!("the sets, then the program's set: {faults:?}");
    };
    assert_eq!(
        (sets.need, sets.needed),
        (Need::InstructionSet, 2),
        "{sets}"
    );
    assert_eq!(reset.need, Need::InstructionSet, "{reset}");
    assert_eq!(
        reset.mismatches,
        [InstructionMismatch::Missing {
            mnemonic: "reset".to_owned()
        }],
        "{reset}"
    );
}
