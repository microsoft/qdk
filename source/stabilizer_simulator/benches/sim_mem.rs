#[cfg(target_os = "linux")]
mod bench {
    #![allow(clippy::unit_arg)]

    use iai_callgrind::{library_benchmark, library_benchmark_group};
    use stabilizer_simulator::{Simulator, noise_config::NoiseConfig, operation::*};
    use std::hint::black_box;
    fn setup(gates: Vec<Operation>) -> (Simulator, Vec<Operation>) {
        const NUM_QUBITS: usize = 1224;
        let simulator = Simulator::new(NUM_QUBITS, NoiseConfig::NOISELESS);
        (simulator, gates)
    }

    fn teardown(_: (Simulator, Vec<Operation>)) {}

    fn run_simulation(simulator: &mut Simulator, gates: &[Operation]) {
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
    fn gates((mut simulator, gates): (Simulator, Vec<Operation>)) -> (Simulator, Vec<Operation>) {
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
