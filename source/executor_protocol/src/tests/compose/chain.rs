// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::rc::Rc;
use std::sync::Arc;

use crate::tests::test_double::{TestDoubleExecutor, TestDoubleModel};
use crate::tests::{
    Bits, ByHand, Count, Relabel, bits_set, block_on, flip_and_toss, reversed_set, shots, values,
};
use crate::{
    Chain, DefaultInterpreter, Executor, InstructionBatch, InstructionBatchRef, InstructionCall,
    InstructionSet, Interpret, InterpreterDriver, JobFaults, Need, Needs, Opcode, Operand,
    Rejected, ShotId, ShotSet, StaticProgram, Value, interpreter, wait_async,
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

/// A chain of the crate's interpreter on the test double, which holds at
/// most `max_shots` shots at once when stated.
fn double_chain(
    max_shots: Option<u64>,
) -> Chain<DefaultInterpreter, bool, TestDoubleExecutor<TestDoubleModel>> {
    let model = TestDoubleModel {
        max_shots,
        ..TestDoubleModel::default()
    };
    (interpreter() >> TestDoubleExecutor::with_model(model, 7))
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
#[expect(
    clippy::needless_borrows_for_generic_args,
    reason = "the borrowed array is a form under test"
)]
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
#[expect(
    clippy::needless_borrows_for_generic_args,
    reason = "the borrowed array is the form under test"
)]
fn borrowed_shots_give_what_owned_shots_give() {
    let chain = || {
        (interpreter() >> TestDoubleExecutor::with_model(Bits::default(), 7))
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
    let chain = (interpreter() >> TestDoubleExecutor::with_model(Bits::default(), 7))
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
    let chain = (interpreter() >> TestDoubleExecutor::with_model(Bits::default(), 7))
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

/// A job whose program of calls by opcode states no set, given to a chain
/// on the bits: the refusal names the program as the side that may state
/// none, the program being what the job gives.
#[test]
fn a_jobs_program_of_calls_by_opcode_stating_no_set_is_named_in_its_refusal() {
    let chain = (interpreter() >> TestDoubleExecutor::with_model(Bits::default(), 7))
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
    assert!(text.starts_with("the program or "), "{text}");
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
    let chain = (interpreter() >> TestDoubleExecutor::with_model(bits, 7))
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
        (interpreter()
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
        let chain = (interpreter() >> TestDoubleExecutor::with_model(Bits::default(), 7))
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

/// An interpreter that reads shot 0 whatever the job's shots: a bug.
struct ReadsShotZero;

impl Interpret<()> for ReadsShotZero {
    type Instruction = bool;
    type Output = Result<(), Rejected>;

    async fn interpret<E>(&self, (): (), executor: &mut E, _: &ShotSet) -> Self::Output
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

/// A job calls only its own shots: one of shot 1 whose interpreter calls
/// shot 0 is refused before the executor sees the call, which makes no
/// shot.
#[test]
fn a_job_that_calls_a_shot_not_its_own_is_refused() {
    let model = TestDoubleModel::default();
    let made = Rc::clone(&model.made);
    let chain = (InterpreterDriver::new(ReadsShotZero) >> TestDoubleExecutor::with_model(model, 7))
        .expect("the double takes any batch of `bool`s");
    let (output, _) = chain.run((), [1]).expect("a shot").wait();
    assert_eq!(output, Err(Rejected::ShotNotHeld(0)));
    assert_eq!(made.get(), 0, "the job reached shot 0");
}

/// A job awaited, on the caller's executor of futures, gives what it gives
/// waited for.
#[test]
fn a_job_awaited_gives_what_it_gives_waited_for() {
    let chain = || {
        (interpreter() >> TestDoubleExecutor::with_model(Bits::default(), 7))
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

/// `interpreter() >> adapter >> executor`, then the job: a static program
/// run through an adapter that renumbers its calls on the bits gives the
/// outcomes and the report that the same program written as a driver
/// gives on the same chain.
#[test]
fn a_chain_runs_a_job_as_its_program_written_as_a_driver_runs() {
    let relabel = || Relabel::new(reversed_set(), false);
    let bits = || TestDoubleExecutor::with_model(Bits::default(), 7);
    let program = flip_and_toss_in(bits_set(), None);
    let chain = (interpreter() >> relabel() >> bits()).expect("the chain runs the bits' set");
    let (interpreted, interpreted_report) = chain
        .run(program, [2, 5, 6, 11])
        .expect("shots, needs met")
        .wait();
    let by_hand = ByHand {
        needs: Needs::default().with_instruction_set(Some(Arc::new(bits_set()))),
        batches: flip_and_toss(&bits_set()),
    };
    let (by_hand, by_hand_report) = (interpreter() >> relabel() >> bits())
        .expect("the chain runs the bits' set")
        .run(by_hand, [2, 5, 6, 11])
        .expect("shots, needs met")
        .wait();
    assert_eq!(
        interpreted.expect("the bits neither refuse nor fail"),
        by_hand
    );
    assert_eq!(interpreted_report, by_hand_report);
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
    let chain = (interpreter() >> relabel >> bits).expect("the chain runs the bits' set");
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
