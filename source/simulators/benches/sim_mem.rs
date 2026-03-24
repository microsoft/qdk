// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(target_os = "linux")]
mod bench {
    #![allow(clippy::unit_arg)]

    use iai_callgrind::{library_benchmark, library_benchmark_group};
    use qdk_simulators::{
        Simulator as _,
        noise_config::{CumulativeNoiseConfig, NoiseConfig},
        stabilizer_simulator::{
            StabilizerSimulator,
            noise::Fault,
            operation::{Operation, cz, h, id, mz, s, x, y, z},
        },
    };
    use std::{hint::black_box, sync::Arc};

    const SEED: u32 = 1000;

    fn setup(gates: Vec<Operation>) -> (StabilizerSimulator, Vec<Operation>) {
        const NUM_QUBITS: usize = 1224;
        const NUM_RESULTS: usize = NUM_QUBITS;
        let noise: Arc<CumulativeNoiseConfig<Fault>> =
            Arc::new(<NoiseConfig<f64, f64>>::NOISELESS.into());
        let simulator = StabilizerSimulator::new(NUM_QUBITS, NUM_RESULTS, SEED, noise);
        (simulator, gates)
    }

    fn teardown(_: (StabilizerSimulator, Vec<Operation>)) {}

    fn run_simulation(simulator: &mut StabilizerSimulator, gates: &[Operation]) {
        simulator.apply_gates(gates);
    }

    #[library_benchmark]
    #[benches::with_setup(
        args = [
            vec![id(0)],
            vec![x(0)],
            vec![y(0)],
            vec![z(0)],
            vec![h(0)],
            vec![s(0)],
            vec![cz(0, 1)],
            vec![mz(0)],
            vec![h(0), mz(0)],
        ],
        setup = setup,
        teardown = teardown)
    ]
    fn gates(
        (mut simulator, gates): (StabilizerSimulator, Vec<Operation>),
    ) -> (StabilizerSimulator, Vec<Operation>) {
        black_box(run_simulation(&mut simulator, &gates));
        (simulator, gates)
    }

    library_benchmark_group!(
        name = bench_gates;
        benchmarks = gates
    );
}

#[cfg(target_os = "linux")]
use bench::bench_gates;

#[cfg(target_os = "linux")]
iai_callgrind::main!(
    config = iai_callgrind::LibraryBenchmarkConfig::default()
        .tool(iai_callgrind::Callgrind::default().flamegraph(iai_callgrind::FlamegraphConfig::default()));
    library_benchmark_groups = bench_gates
);

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("bench 'sim_mem' is Linux-only; skipping on this platform.");
}
