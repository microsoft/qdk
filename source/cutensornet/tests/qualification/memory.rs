// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Device memory along H₂'s pattern: [`BATCH`] gates, a read, then a
//! measurement, step after step. The full qualification showed the memory
//! growing by about 80 MiB per `read_cost` checkpoint (one projection, five
//! computes, nine reads), until the state closes, while `free_after_compute`
//! (computes, reads, samples, forks, no projection) stayed flat. Each
//! variant here changes one thing in the step, so that the slope, in MiB per
//! step, says what the memory grows with:
//!
//! - `unitary` ends each step with Z instead of a projection: the control;
//! - `projection` ends it with a projection, as a measurement does;
//! - `two_projections` projects twice on the same value (the second has
//!   probability one): two non-unitary operators, still one compute;
//! - `full_rank` applies `diag(1, ½)`, rescaled to keep the norm: non-unitary
//!   but invertible, unlike a projector. [`MpsEngine::apply`] lets an engine
//!   assume a unitary; `CuTensorNetEngine` checks, and registers this one as
//!   non-unitary, as it does a projection;
//! - `projection_three_reads` reads three times before projecting: one
//!   compute, three expectations;
//! - `untruncated_*` keep the gates on the first ten sites, whose bonds never
//!   exceed χ = 32, so that no compute truncates;
//! - `projection` again at width 16 (`read_cost`'s), and at χ = 8: whether the
//!   growth depends on the width or χ.
//!
//! Each variant runs on a new state of the same engine, and records the
//! process's device memory before the state, every [`MARK`] steps (also just
//! before the step's last operator, so that growth at registration and at
//! the next compute can be told apart), and after the state closes; then the
//! slope, by least squares over the marks after step 0. Rows tagged
//! `extrapolation=h2` scale the slope and the mean step to H₂'s 1,404
//! measurements: estimates, not measurements.

use crate::circuits::{self, Operator, c};
use crate::measure::{BATCH, H2_READS, UNLIKELY, quick};
use crate::report::{Report, device_mib, or_unavailable};
use num_complex::Complex64;
use std::{
    error::Error,
    time::{Duration, Instant},
};
use tensornet::MpsEngine;

/// H₂'s qubits and the bond cap `mps-a` plans for it.
const H2_WIDTH: usize = 29;
const H2_CHI: usize = 32;

/// Sites whose bonds never exceed χ = 32: at most 2⁵ across any cut of ten.
const UNTRUNCATED_SITES: usize = 10;

/// The steps between two memory records.
pub const MARK: usize = 10;

/// The operator that ends each step, after its gates and its reads.
#[derive(Clone, Copy, Debug)]
pub enum Ending {
    /// Z on the read site: unitary, the control.
    Unitary,
    /// The read site projected on a value, as a measurement.
    Projection,
    /// The same projection twice; the second has probability one.
    TwoProjections,
    /// `diag(1, ½)` on the read site, rescaled to keep the norm.
    FullRank,
}

/// One run of the pattern.
#[derive(Clone, Copy, Debug)]
pub struct Variant {
    pub name: &'static str,
    pub width: usize,
    pub chi: usize,
    /// The gates act on sites `0..gated_sites`; the rest stay |0⟩.
    pub gated_sites: usize,
    /// Reads of the same site before the ending: the first computes.
    pub reads: usize,
    pub ending: Ending,
}

impl Variant {
    const fn new(name: &'static str, width: usize, chi: usize, ending: Ending) -> Self {
        Self {
            name,
            width,
            chi,
            gated_sites: width,
            reads: 1,
            ending,
        }
    }
}

/// The variants the qualification runs, in order.
pub fn variants() -> Vec<Variant> {
    let h2 = |name, ending| Variant::new(name, H2_WIDTH, H2_CHI, ending);
    vec![
        h2("unitary", Ending::Unitary),
        h2("projection", Ending::Projection),
        h2("two_projections", Ending::TwoProjections),
        h2("full_rank", Ending::FullRank),
        Variant {
            reads: 3,
            ..h2("projection_three_reads", Ending::Projection)
        },
        Variant {
            gated_sites: UNTRUNCATED_SITES,
            ..h2("untruncated_unitary", Ending::Unitary)
        },
        Variant {
            gated_sites: UNTRUNCATED_SITES,
            ..h2("untruncated_projection", Ending::Projection)
        },
        Variant::new("projection", 16, H2_CHI, Ending::Projection),
        Variant::new("projection", H2_WIDTH, 8, Ending::Projection),
    ]
}

/// The steps of each variant: 200, or 30 in quick mode.
pub fn steps() -> usize {
    if quick() { 30 } else { 200 }
}

/// Runs every variant of [`variants`] for [`steps`] steps.
pub fn projection_memory<E: MpsEngine>(report: &mut Report, engine: &E) {
    projection_memory_of(report, engine, &variants(), steps());
}

/// Runs `variants` for `steps` steps each; an engine error is a row, and the
/// next variant runs.
pub fn projection_memory_of<E: MpsEngine>(
    report: &mut Report,
    engine: &E,
    variants: &[Variant],
    steps: usize,
) {
    for variant in variants {
        let item = |metric: &str| {
            format!(
                "variant={};width={};chi={};{metric}",
                variant.name, variant.width, variant.chi
            )
        };
        if let Err(error) = run(report, engine, variant, steps, &item) {
            report.error(&item("run"), error);
        }
        report.row(
            &item("device_mib_after_close"),
            or_unavailable(device_mib()),
            "MiB",
        );
    }
}

fn run<E: MpsEngine>(
    report: &mut Report,
    engine: &E,
    variant: &Variant,
    steps: usize,
    item: &dyn Fn(&str) -> String,
) -> Result<(), Box<dyn Error>> {
    report.row(
        &item("device_mib_before_prepare"),
        or_unavailable(device_mib()),
        "MiB",
    );
    let site = variant.gated_sites / 2;
    // A brickwork layer has at least one gate per gated site.
    let layers = steps * BATCH / variant.gated_sites + 2;
    let mut gates = circuits::brickwork(variant.gated_sites, layers, false, 41).into_iter();
    let mut state = engine.prepare(&circuits::capacity(variant.width, variant.chi)?)?;
    let mut marks = vec![(0, device_mib())];
    report.row(
        &item("step=0;device_mib"),
        or_unavailable(marks[0].1),
        "MiB",
    );
    let (mut since_mark, mut total) = (Duration::ZERO, Duration::ZERO);
    for step in 1..=steps {
        let at = |metric: &str| item(&format!("step={step};{metric}"));
        let marked = step % MARK == 0 || step == steps;
        let start = Instant::now();
        for _ in 0..BATCH {
            let gate = gates.next().ok_or("brickwork too short")?;
            engine.apply(&mut state, &gate.sites, &gate.matrix)?;
        }
        let mut p_one = 0.0;
        for _ in 0..variant.reads {
            p_one = engine.probability_of_one(&state, site)?;
        }
        let mut elapsed = start.elapsed();
        if marked {
            report.row(
                &at("device_mib_before_ending"),
                or_unavailable(device_mib()),
                "MiB",
            );
        }
        let start = Instant::now();
        end(engine, &mut state, variant.ending, site, step, p_one)?;
        elapsed += start.elapsed();
        since_mark += elapsed;
        total += elapsed;
        if marked {
            let mib = device_mib();
            report.row(&at("device_mib"), or_unavailable(mib), "MiB");
            let previous = marks.last().map_or(0, |mark| mark.0);
            let mean = since_mark.as_secs_f64() * 1e3 / float(step - previous);
            report.row(&at("ms_per_step"), format!("{mean:.3}"), "ms");
            marks.push((step, mib));
            since_mark = Duration::ZERO;
        }
    }
    let mean = total.as_secs_f64() * 1e3 / float(steps);
    report.row(&item("ms_per_step"), format!("{mean:.3}"), "ms");
    report.row(
        &item(&format!(
            "extrapolation=h2;measurements={H2_READS};seconds_per_shot"
        )),
        format!("{:.1}", mean * float(H2_READS) / 1e3),
        "s",
    );
    let measured: Vec<(f64, f64)> = marks
        .iter()
        .filter(|(step, _)| *step > 0)
        .filter_map(|&(step, mib)| Some((float(step), f64::from(u32::try_from(mib?).ok()?))))
        .collect();
    match slope(&measured) {
        Some(slope) => {
            report.row(&item("slope"), format!("{slope:.2}"), "MiB/step");
            report.row(
                &item(&format!(
                    "extrapolation=h2;measurements={H2_READS};mib_per_shot"
                )),
                format!("{:.0}", slope * float(H2_READS)),
                "MiB",
            );
        }
        None => report.note(&item("slope"), "fewer than two memory records"),
    }
    drop(state);
    Ok(())
}

/// Applies `ending` on `site`, whose P(One) is `p_one`. A projection takes
/// the value `step`'s parity prefers, or the other if that one has
/// probability below [`UNLIKELY`].
fn end<E: MpsEngine>(
    engine: &E,
    state: &mut E::State,
    ending: Ending,
    site: usize,
    step: usize,
    p_one: f64,
) -> Result<(), E::Error> {
    let one = if step % 2 == 1 {
        p_one >= UNLIKELY
    } else {
        1.0 - p_one < UNLIKELY
    };
    let probability = if one { p_one } else { 1.0 - p_one };
    match ending {
        Ending::Unitary => engine.apply(state, &[site], &z().matrix),
        Ending::Projection => engine.project(state, site, one, probability),
        Ending::TwoProjections => {
            engine.project(state, site, one, probability)?;
            engine.project(state, site, one, 1.0)
        }
        Ending::FullRank => {
            // ‖diag(1, ½)ψ‖² = P(Zero) + P(One)/4.
            let scale = 1.0 / ((1.0 - p_one) + p_one / 4.0).sqrt();
            let matrix = [c(scale, 0.0), c(0.0, 0.0), c(0.0, 0.0), c(scale / 2.0, 0.0)];
            engine.apply(state, &[site], &matrix)
        }
    }
}

fn z() -> Operator {
    let (one, zero) = (Complex64::new(1.0, 0.0), Complex64::new(0.0, 0.0));
    Operator {
        sites: vec![0],
        matrix: vec![one, zero, zero, -one],
    }
}

/// The least-squares slope of `points`, or `None` with fewer than two
/// distinct abscissas.
fn slope(points: &[(f64, f64)]) -> Option<f64> {
    let n = float(points.len());
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    if points.len() < 2 || sxx <= 0.0 {
        return None;
    }
    let sxy: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    Some(sxy / sxx)
}

fn float(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}
