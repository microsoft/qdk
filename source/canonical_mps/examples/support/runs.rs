// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A circuit run on the engine and on the sparse simulator: the state
//! each one ends in and the seconds it took, and the process's peak
//! memory.

use super::{
    Res,
    circuit::{Circuit, Op},
};
use canonical_mps::{CanonicalMpsState, CpuAlgebra, CpuMpsEngine};
use qdk_simulators::SparseStateSim;
use std::{fs, hint::black_box, time::Instant};
use tensornet::{MpsEngine, MpsShape};

/// A chain of `n` qubits with every bond at most `chi`.
pub fn chain(n: usize, chi: usize) -> MpsShape {
    MpsShape::qubit_chain(n, chi).expect("a chain of at least one site")
}

/// The engine's run of a circuit: its state, the seconds it took, and the
/// largest gap between a projection's probability and the recorded one.
pub struct EngineRun {
    pub state: CanonicalMpsState<CpuAlgebra>,
    pub seconds: f64,
    pub max_projection_gap: f64,
}

/// Runs `circuit` on `engine`, qubit `q` on site `order[q]`, every bond at
/// most `chi`. A projection takes the engine's own probability.
pub fn run_engine(
    engine: &CpuMpsEngine,
    circuit: &Circuit,
    order: &[usize],
    chi: usize,
) -> Res<EngineRun> {
    let mut state = engine.prepare(&chain(circuit.qubits, chi))?;
    let mut max_projection_gap: f64 = 0.0;
    let started = Instant::now();
    for op in &circuit.ops {
        match op {
            Op::Gate { qubits, matrix, .. } => {
                let sites: Vec<usize> = qubits.iter().map(|&q| order[q]).collect();
                engine.apply(&mut state, &sites, matrix)?;
            }
            Op::Project {
                qubit,
                one,
                probability,
            } => {
                let p = engine.project(&mut state, order[*qubit], *one)?;
                max_projection_gap = max_projection_gap.max((p - probability).abs());
            }
        }
    }
    // The engine leaves the last fused pair unsplit until a read; this read
    // splits it, so the time and the statistics include that split.
    black_box(engine.retained_weight(&state)?);
    Ok(EngineRun {
        state,
        seconds: started.elapsed().as_secs_f64(),
        max_projection_gap,
    })
}

/// The sparse simulator's run of a circuit: its state, the seconds it took
/// and the largest gap between a projection's probability and the
/// recorded one.
pub struct SparseRun {
    pub sim: SparseStateSim,
    pub seconds: f64,
    pub max_projection_gap: f64,
}

/// Runs `circuit` on the sparse simulator, through its native operations,
/// and reads it once so that no gate stays queued.
pub fn run_sparse(circuit: &Circuit) -> Res<SparseRun> {
    let mut sim = SparseStateSim::new(None);
    for _ in 0..circuit.qubits {
        let _ = sim.allocate();
    }
    let mut max_projection_gap: f64 = 0.0;
    let started = Instant::now();
    for op in &circuit.ops {
        match op {
            Op::Gate { natives, .. } => {
                for native in natives.as_ref().ok_or("a gate with no native form")? {
                    native.on(&mut sim);
                }
            }
            Op::Project {
                qubit,
                one,
                probability,
            } => {
                let p = sim.force_collapse(*one, *qubit);
                max_projection_gap = max_projection_gap.max((p - probability).abs());
            }
        }
    }
    // The simulator queues some gates until a read; this read applies them.
    let every: Vec<usize> = (0..circuit.qubits).collect();
    black_box(sim.joint_probability(&every));
    Ok(SparseRun {
        sim,
        seconds: started.elapsed().as_secs_f64(),
        max_projection_gap,
    })
}

/// This process's peak resident memory, in kB (Linux only; 0 elsewhere).
pub fn peak_memory_kb() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status
                .lines()
                .find(|line| line.starts_with("VmHWM:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|kb| kb.parse().ok())
        })
        .unwrap_or(0)
}
