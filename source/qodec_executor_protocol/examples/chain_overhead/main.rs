// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What running a qodec's layers as a chain costs, against Qodec's runtime
//! driven directly: the same program, on the same shots, through
//!
//! ```text
//! TopIsaDriver >> QodecLayerAdapter >> ActionSimulatorExecutor
//! ```
//!
//! and through one Qodec runtime per shot over the same stabilizer
//! simulator. It first checks that both read the same bits on every shot,
//! then times each the best of a few rounds, and prints the time per shot
//! and per logical instruction of each, and their ratio.
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

mod timing;

use executor_protocol::{Adapter, Driver, Executor, ShotSet};
use qodec_executor_protocol::{ActionSimulatorExecutor, QodecLayerAdapter};
use qodec_runtime_recipes::runtime;

use crate::direct_runtime::{run_directly, simulator, stabilizer};
use crate::programs::{Program, ShotReadouts};
use crate::top_isa_driver::TopIsaDriver;

/// The seed of every run.
const SEED: u64 = 0x5EED;

/// The shots of a run, unless the command line gives them.
const SHOTS: u32 = 2_000;

/// The rounds each variant is timed over; the best counts.
const ROUNDS: usize = 5;

/// Each qodec, with a program of Qodec's own REPL tests.
const CASES: [(&str, &str); 3] = [
    (
        "steane/steane.qodec.yaml",
        "prepare_z 0; prepare_x 1; idle_ft 0; h 0; cnot 0 1; measure_z 0; measure_x 1",
    ),
    (
        "c4c6/qodec.yaml",
        "prepare_x_all 0; hadamard_all 0; mul_u 0; measure_z_all 0",
    ),
    (
        "surface/surface.qodec.yaml",
        "prepare_z 0; prepare_z 1; idle 0; merge_zz 0 1; measure_merged_z 0",
    ),
];

fn main() {
    let shots = std::env::args()
        .nth(1)
        .map_or(SHOTS, |arg| arg.parse().expect("the shots are a number"));
    println!("{shots} shots, best of {ROUNDS} rounds");
    println!(
        "{:<28} {:>14} {:>14} {:>14} {:>14} {:>7}",
        "qodec", "chain/shot", "direct/shot", "chain/instr", "direct/instr", "ratio"
    );
    for (path, text) in CASES {
        let model = qodecs::model(path);
        let plan = qodecs::plan(&model);
        let program = Program::parse(text);
        let shot_set = ShotSet::ids(0..shots);

        let mut executor = ActionSimulatorExecutor::stabilizer(&model.bottom)
            .expect("the example's bottom ISA is a protocol set");
        let mut chain = (Driver::new(TopIsaDriver::new(&model.top, &program))
            >> Adapter::new(
                QodecLayerAdapter::whole(&model).expect("the example's layers assemble"),
            )
            >> Executor::new(&mut executor))
        .expect("the layers fit the driver above and the simulator below");
        let mut run_chain = || {
            chain
                .run(&shot_set, SEED)
                .expect("the program runs")
                .into_iter()
                .map(|run| (run.stream, run.readouts))
                .collect::<Vec<_>>()
        };
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
        assert_eq!(
            chained,
            run_direct(&streams),
            "{path}: both read the same bits"
        );

        let chain_time = timing::best_of(ROUNDS, &mut run_chain);
        let direct_time = timing::best_of(ROUNDS, || run_direct(&streams));
        let instructions: u32 = program
            .executes
            .iter()
            .map(|steps| u32::try_from(steps.len()).expect("a short program"))
            .sum();
        println!(
            "{path:<28} {:>14.1?} {:>14.1?} {:>14.1?} {:>14.1?} {:>7.3}",
            chain_time / shots,
            direct_time / shots,
            chain_time / (shots * instructions),
            direct_time / (shots * instructions),
            chain_time.as_secs_f64() / direct_time.as_secs_f64()
        );
    }
}
