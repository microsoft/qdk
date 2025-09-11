extern crate criterion;
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use paulimer::{bits::BitVec, pauli::PauliUnitary};
use rand::prelude::*;

pub fn multiply_benchmark(criterion: &mut Criterion) {
    let mut group = criterion.benchmark_group("Pauli::multiply");
    for size in [100usize, 1000usize, 10000usize] {
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |bencher, size| {
            bencher.iter_batched(
                || (random_pauli(*size), random_pauli(*size)),
                |mut pair| pair.0 *= &pair.1,
                BatchSize::LargeInput,
            );
        });
    }
    group.finish();
}

#[cfg(not(unix))]
criterion_group!(benches, multiply_benchmark);

#[cfg(unix)]
criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(PProfProfiler::new(100, Output::Flamegraph(None)));
    targets = multiply_benchmark
}

criterion_main!(benches);

fn random_pauli(dimension: usize) -> PauliUnitary<BitVec, u8> {
    let x_bits = std::iter::from_fn(move || Some(thread_rng().gen::<bool>())).take(dimension);
    let z_bits = std::iter::from_fn(move || Some(thread_rng().gen::<bool>())).take(dimension);
    PauliUnitary::from_bits(x_bits.collect(), z_bits.collect(), 0u8)
}

//
//
//
//
//
// Copyright 2020 TiKV Project Authors. Licensed under Apache-2.0.
#[cfg(unix)]
use pprof::flamegraph::Options as FlamegraphOptions;

#[cfg(unix)]
use criterion::profiler::Profiler;
#[cfg(unix)]
use pprof::ProfilerGuard;

#[cfg(unix)]
use std::fs::File;
#[cfg(unix)]
use std::marker::PhantomData;
#[cfg(unix)]
use std::os::raw::c_int;
#[cfg(unix)]
use std::path::Path;

#[cfg(unix)]
#[allow(clippy::large_enum_variant)]
pub enum Output<'a> {
    Flamegraph(Option<FlamegraphOptions<'a>>),

    #[deprecated(note = "This branch is used to include lifetime parameter. Don't use it directly.")]
    _Phantom(PhantomData<&'a ()>),
}
#[cfg(unix)]
pub struct PProfProfiler<'a, 'b> {
    frequency: c_int,
    output: Output<'b>,
    active_profiler: Option<ProfilerGuard<'a>>,
}
#[cfg(unix)]
impl<'b> PProfProfiler<'_, 'b> {
    #[must_use]
    pub fn new(frequency: c_int, output: Output<'b>) -> Self {
        Self {
            frequency,
            output,
            active_profiler: None,
        }
    }
}
#[cfg(unix)]
impl Profiler for PProfProfiler<'_, '_> {
    fn start_profiling(&mut self, _benchmark_id: &str, _benchmark_dir: &Path) {
        self.active_profiler = Some(ProfilerGuard::new(self.frequency).unwrap());
    }

    fn stop_profiling(&mut self, _benchmark_id: &str, benchmark_dir: &Path) {
        std::fs::create_dir_all(benchmark_dir).unwrap();

        let filename = match self.output {
            Output::Flamegraph(_) => "flamegraph.svg",
            // This is `""` but not `unreachable!()`, because `unreachable!()`
            // will result in another compile error, so that the user may not
            // realize the error thrown by `compile_error!()` at the first time.
            Output::_Phantom(_) => "",
        };
        let output_path = benchmark_dir.join(filename);
        let output_file = File::create(&output_path)
            .unwrap_or_else(|_| panic!("File system error while creating {}", output_path.display()));

        if let Some(profiler) = self.active_profiler.take() {
            match &mut self.output {
                Output::Flamegraph(options) => {
                    let default_options = &mut FlamegraphOptions::default();
                    let options = options.as_mut().unwrap_or(default_options);

                    profiler
                        .report()
                        .build()
                        .unwrap()
                        .flamegraph_with_options(output_file, options)
                        .expect("Error while writing flamegraph");
                }

                Output::_Phantom(_) => unreachable!(),
            }
        }
    }
}
