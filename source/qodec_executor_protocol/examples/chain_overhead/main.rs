// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What running a qodec's layers as a chain costs, against Qodec's runtime
//! driven directly: the same program, on the same shots, through
//!
//! ```text
//! TopIsaDriver >> QodecLayerAdapter >> CountingExecutor<ActionSimulatorExecutor>
//! ```
//!
//! and through one Qodec runtime per shot over the same stabilizer
//! simulator. The chain runs twice: over a simulator that takes batches of
//! any size, and over one that takes at most a few instructions per batch,
//! so that the adapter splits the layers' batches. It first checks that
//! every variant reads the same bits on every shot, then times each the
//! best of a few rounds, and prints the time per shot and per logical
//! instruction, each chain's ratio to the direct runtime, and the batches
//! each chain sends the simulator per shot.
//!
//! ```text
//! cargo run --release -p qodec_executor_protocol \
//!     --example chain_overhead -- [shots]
//! ```
//!
//! Build it with every function and branch target aligned, so that code
//! placement does not skew the ratio:
//!
//! ```text
//! RUSTFLAGS="-C llvm-args=-align-all-functions=6 \
//!     -C llvm-args=-align-all-nofallthru-blocks=5"
//! ```

// The chains' tests' driver, direct runtime, programs and qodecs, so that
// the bench runs what the tests check.
#[expect(dead_code, reason = "the tests' state-vector simulator")]
#[path = "../../tests/chains/direct_runtime.rs"]
mod direct_runtime;
#[path = "../../tests/chains/programs.rs"]
mod programs;
#[path = "../../tests/chains/qodecs.rs"]
mod qodecs;
#[expect(dead_code, reason = "the tests' malformed and checked variants")]
#[path = "../../tests/chains/top_isa_driver.rs"]
mod top_isa_driver;

mod counting_executor;
mod timing;
mod workload;

use std::sync::atomic::{AtomicUsize, Ordering};

use executor_protocol::{Adapter, Driver, Executor, ShotSet};
use qodec_executor_protocol::{ActionSimulatorExecutor, QodecLayerAdapter};
use qodec_runtime_recipes::{QodecModel, runtime};

use crate::counting_executor::CountingExecutor;
use crate::direct_runtime::{run_directly, simulator, stabilizer};
use crate::programs::{Program, ShotReadouts};
use crate::top_isa_driver::TopIsaDriver;
use crate::workload::{CASES, MOST_BELOW};

/// The seed of every run.
const SEED: u64 = 0x5EED;

/// The shots of a run, unless the command line gives them.
const SHOTS: u32 = 2_000;

/// The rounds each variant is timed over; the best counts.
const ROUNDS: usize = 5;

fn main() {
    let shots = std::env::args()
        .nth(1)
        .map_or(SHOTS, |arg| arg.parse().expect("the shots are a number"));
    println!(
        "{shots} shots, best of {ROUNDS} rounds; the split chain's simulator takes at most \
         {MOST_BELOW} instructions per batch"
    );
    println!(
        "{:<28} {:>12} {:>12} {:>12} {:>12} {:>7} {:>7} {:>9} {:>9}",
        "qodec",
        "chain/shot",
        "split/shot",
        "direct/shot",
        "chain/instr",
        "ratio",
        "split",
        "batches",
        "split"
    );
    for (path, text) in CASES {
        let model = qodecs::model(path);
        let plan = qodecs::plan(&model);
        let program = Program::parse(text);
        let shot_set = ShotSet::ids(0..shots);

        let mut unsplit = CountingExecutor::new(stabilizer_of(&model));
        let unsplit_batches = unsplit.batches();
        let mut run_chain = chain_over(&model, &program, &mut unsplit, &shot_set);
        let mut split = CountingExecutor::new(stabilizer_of(&model)).taking_at_most(MOST_BELOW);
        let split_batches = split.batches();
        let mut run_split = chain_over(&model, &program, &mut split, &shot_set);
        let run_direct = |streams: &[u64]| -> Vec<ShotReadouts> {
            streams
                .iter()
                .map(|&stream| {
                    let mut direct = runtime(&plan, simulator(plan.bottom(), stabilizer(stream)));
                    run_directly(plan.top(), &program, &mut direct)
                })
                .collect()
        };

        let (streams, chained): (Vec<u64>, Vec<ShotReadouts>) = run_chain().into_iter().unzip();
        let direct = run_direct(&streams);
        assert_eq!(chained, direct, "{path}: the chain reads the direct bits");
        let (_, split_read): (Vec<u64>, Vec<ShotReadouts>) = run_split().into_iter().unzip();
        assert_eq!(split_read, direct, "{path}: the split chain reads them too");
        let per_shot = |batches: &AtomicUsize| {
            let batches = u32::try_from(batches.load(Ordering::Relaxed)).expect("a short run");
            f64::from(batches) / f64::from(shots)
        };
        let (unsplit_per_shot, split_per_shot) =
            (per_shot(&unsplit_batches), per_shot(&split_batches));
        assert!(
            split_per_shot > unsplit_per_shot,
            "{path}: the split chain sends more batches below"
        );

        let chain_time = timing::best_of(ROUNDS, &mut run_chain);
        let split_time = timing::best_of(ROUNDS, &mut run_split);
        let direct_time = timing::best_of(ROUNDS, || run_direct(&streams));
        let instructions: u32 = program
            .executes
            .iter()
            .map(|steps| u32::try_from(steps.len()).expect("a short program"))
            .sum();
        println!(
            "{path:<28} {:>12.1?} {:>12.1?} {:>12.1?} {:>12.1?} {:>7.3} {:>7.3} {:>9.1} {:>9.1}",
            chain_time / shots,
            split_time / shots,
            direct_time / shots,
            chain_time / (shots * instructions),
            chain_time.as_secs_f64() / direct_time.as_secs_f64(),
            split_time.as_secs_f64() / direct_time.as_secs_f64(),
            unsplit_per_shot,
            split_per_shot
        );
    }
}

/// A stabilizer simulator of `model`'s bottom ISA.
fn stabilizer_of(model: &QodecModel) -> ActionSimulatorExecutor {
    ActionSimulatorExecutor::stabilizer(&model.bottom)
        .expect("the example's bottom ISA is a protocol set")
}

/// A run of `program` on `shots` through every layer of `model` over
/// `executor`: each shot's stream and readouts.
fn chain_over<'a>(
    model: &QodecModel,
    program: &Program,
    executor: &'a mut CountingExecutor<ActionSimulatorExecutor>,
    shots: &'a ShotSet,
) -> impl FnMut() -> Vec<(u64, ShotReadouts)> + 'a {
    let mut chain = (Driver::new(TopIsaDriver::new(&model.top, program))
        >> Adapter::new(QodecLayerAdapter::whole(model).expect("the example's layers assemble"))
        >> Executor::new(executor))
    .expect("the layers fit the driver above and the simulator below");
    move || {
        chain
            .run(shots, SEED)
            .expect("the program runs")
            .into_iter()
            .map(|run| (run.stream, run.readouts))
            .collect()
    }
}
