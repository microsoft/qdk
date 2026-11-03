//! What the tests share: the qodecs they run, their programs and the
//! reference values every chain is held to; the devices that misbehave on
//! cue ([`devices`]); a scene of two reads through an adapter over coins
//! wrapped to refuse one ([`two_reads_over_coins_refusing`]); and how the
//! tests wait ([`waiting`]).

// Each test binary uses only part of this module.
#![allow(dead_code)]

pub mod devices;
pub mod waiting;

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use executor_protocol::{ShotId, ShotOutcome, ShotSet, Value};
use qodec::Qodec;
use qodec_demo::boundary::{Boundary, status};
use qodec_demo::program::Program;
use qodec_runtime::Status;

/// The seed of every run, from which each shot's seed is drawn.
pub const SEED: u64 = 0x5EED;

/// The shots of the reference values: not a range, so that a shot that
/// takes another's place, or its seed, shows.
pub const SHOTS: [ShotId; 4] = [2, 5, 6, 11];

/// The chance that a reset or a CX flips each bit it touches: with
/// [`SEED`], low enough that shot 2 draws no flip and reads Zero, while
/// shots 5, 6 and 11 draw one each, which the frame decoder detects and
/// does not correct, so they read an error.
pub const FLIP: f64 = 0.01;

/// A flip chance at which, on repetition3's whole program, every shot draws
/// several flips: their counts, and so their log probabilities, differ,
/// and their statuses are not all alike, so a shot that drew from another
/// shot's seed shows. (On one syndrome round, two shots may draw as many
/// flips.)
pub const DRAWS_FLIP: f64 = 0.3;

/// The shots of the c422 and c4c6 runs, as the Qodec prototype ran them.
pub const CODE_SHOTS: std::ops::RangeInclusive<ShotId> = 0..=3;

/// The repetition3 bundle as the qodec library's repository ships it,
/// copied unchanged into `tests/data` (from `microsoft/qdk-ec` at commit
/// `50f3bff`, `qodec/examples/repetition3`). Its gadgets are written in
/// Stim, and declare the checks the decoder conditions outcomes on, which
/// the lowerer's copy (`source/qodec_lowerer/examples`) does not; the
/// crate's Stim parser is registered first.
pub fn repetition3() -> Qodec {
    qodec_demo::stim::register().expect("the parser registers");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/repetition3.qodec.yaml");
    Qodec::load(path).expect("the repetition3 bundle of tests/data loads")
}

/// The c422 example of the lowerer: one layer, `c832` and `c422` blocks
/// over physical qubits.
pub fn c422() -> Qodec {
    lowerer_example("c422-c832-arch")
}

/// The c4c6 example of the lowerer: two layers.
pub fn c4c6() -> Qodec {
    lowerer_example("c4c6")
}

fn lowerer_example(name: &str) -> Qodec {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../qodec_lowerer/examples")
        .join(name)
        .join("qodec.yaml");
    Qodec::load(path).expect("the lowerer's example loads")
}

/// Repetition3's physical set, `stim+rz`, as the qodec library declares it.
pub fn qodec_stim_rz() -> Arc<qodec::InstructionSet> {
    Arc::clone(&repetition3().layers()[1].instruction_set)
}

/// Repetition3's physical set, `stim+rz`, as the protocol's set and its
/// conversions.
pub fn stim_rz() -> Boundary {
    Boundary::new(&qodec_stim_rz()).expect("the operands express stim+rz")
}

/// The bottom set of `qodec`, as the protocol's set and its conversions.
pub fn bottom(qodec: &Qodec) -> Arc<Boundary> {
    let isa = &qodec.layers().last().expect("a layer").instruction_set;
    Arc::new(Boundary::new(isa).expect("the operands express the bottom set"))
}

/// Repetition3's program: prepare, three syndrome rounds around a rotation,
/// measure.
pub fn repetition3_program(qodec: &Qodec) -> Program {
    Program::parse(
        qodec,
        &[
            "prepare_z 0",
            "idle 0; idle 0; rotate_z 0 theta=0.7; idle 0",
            "measure_z 0",
        ],
    )
}

/// The c422 program: prepare, five rounds of a CCZ and two teleports
/// around an H, measure.
pub fn c422_program(qodec: &Qodec) -> Program {
    let round = "ccz_c832 0; teleport_c832_to_c422 0 1; h_all_c422 1; teleport_c422_to_c832 1 0";
    let rounds = [round; 5].join("; ");
    Program::parse(
        qodec,
        &["prepare_z_all_c832 0", &rounds, "measure_z_all_c832 0"],
    )
}

/// The c4c6 program: prepare, ten idles, measure.
pub fn c4c6_program(qodec: &Qodec) -> Program {
    let idles = ["idle 0"; 10].join("; ");
    Program::parse(qodec, &["prepare_z_all 0", &idles, "measure_z_all 0"])
}

/// The top batch of the Qodec prototype's run of c4c6 in the Qodec
/// runtime's own console: prepare, one idle, measure.
pub fn c4c6_one_batch_program(qodec: &Qodec) -> Program {
    Program::parse(qodec, &["prepare_z_all 0; idle 0; measure_z_all 0"])
}

/// The bottom batches of each shot of [`c4c6_one_batch_program`], as the
/// Qodec runtime's own console lists them ("lower batch 1 of 14"): its
/// layers flush below at the end of each batch, at a readout given as an
/// argument, at a decode that cannot wait, and at a correction whose
/// result is needed.
pub const C4C6_BOTTOM_BATCHES: u64 = 14;

/// `shots` as a set of shots.
pub fn shot_set(shots: &[ShotId]) -> ShotSet {
    ShotSet::new(shots.to_vec()).expect("distinct shots")
}

/// `outcomes` by shot, then tag.
pub fn sorted(mut outcomes: Vec<ShotOutcome>) -> Vec<(ShotId, u32, Value)> {
    outcomes.sort_by_key(|o| (o.shot, o.tag));
    outcomes.iter().map(|o| (o.shot, o.tag, o.value)).collect()
}

/// `outcomes` by shot and tag, each with its conditions.
pub fn sorted_with_conditions(mut outcomes: Vec<ShotOutcome>) -> Vec<ShotOutcome> {
    outcomes.sort_by_key(|o| (o.shot, o.tag));
    outcomes
}

/// Each result's status in `outcomes`, at the shot and tag of each result
/// of `stack` (`StackRan::statuses`), in its order: the conditions the
/// outcome there carries, in the form of the Qodec runtime's own stack's.
///
/// # Panics
///
/// When `outcomes` has no outcome at one of those shots and tags.
pub fn each_status(
    outcomes: &[ShotOutcome],
    stack: &[(ShotId, u32, Status)],
) -> Vec<(ShotId, u32, Status)> {
    let by_tag: BTreeMap<_, _> = (outcomes.iter())
        .map(|o| ((o.shot, o.tag), status(o.conditions)))
        .collect();
    (stack.iter())
        .map(|&(shot, tag, _)| {
            let status = by_tag.get(&(shot, tag));
            (
                shot,
                tag,
                *status.expect("the chain reads every tag the stack reads"),
            )
        })
        .collect()
}

/// Two reads on shot 2 through `$adapter` alone, over coins of `stim+rz`
/// that refuse their call number `$refused` (from 0) and run the other:
/// the reads, the adapter's report, and what the coins ran (a
/// [`Profile`](qodec_demo::profiler::Profile)). A macro, since the coins'
/// type cannot be named in a function's bounds.
// Each test binary uses only part of this module.
#[allow(unused_macros)]
macro_rules! two_reads_over_coins_refusing {
    ($adapter:expr, $refused:expr) => {{
        use executor_protocol::{
            Close, Executor, ExecutorReport, InstructionBatchRef, Operand, ShotSet, compose, wait,
        };
        use qodec_demo::coin::coin_device;
        use qodec_demo::profiler::Profile;

        let refused: usize = $refused;
        let set = $crate::common::stim_rz().set;
        let coins = $crate::common::devices::RefusesCall::new(
            coin_device(std::sync::Arc::clone(&set), $crate::common::SEED),
            refused,
        );
        let mut chain = compose($adapter, coins).expect("every boundary matches");
        let shot = ShotSet::new(vec![2]).expect("one shot");
        let reads = [0, 1]
            .map(|tag| (set.call("M", [Operand::Block(0), Operand::Tag(tag)])).expect("an M"));
        for (at, read) in reads.iter().enumerate() {
            let called = Executor::call(
                &mut chain,
                InstructionBatchRef {
                    shots: &shot,
                    instructions: std::slice::from_ref(read),
                },
            );
            if at == refused {
                assert!(called.is_err(), "precondition: the coins refuse call {at}");
            } else {
                let reply = called.expect("precondition: the coins take the other call");
                wait(reply, &mut Vec::new()).expect("precondition: the other call runs");
            }
        }
        let (report, ran): (_, ExecutorReport<Profile>) = Close::close(&mut chain);
        (reads, report, ran.states)
    }};
}
#[allow(unused_imports)]
pub(crate) use two_reads_over_coins_refusing;
