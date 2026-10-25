// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The measurements, generic over the engine: the qualification runs them on
//! `CuTensorNetEngine`; any [`MpsEngine`] that tells its refusals from its
//! failures runs them the same way. Each prints its rows
//! to the [`Report`] it is given, and none asserts.

use crate::circuits::{self, Operator, cnot, cz, hadamard, ry};
use crate::report::{Report, device_mib, median, milliseconds, or_unavailable, scientific, timed};
use std::{error::Error, time::Duration};
use tensornet::{
    MpsEngine, Pauli, SampleSeed,
    conformance::{TOLERANCE, exact_capacity},
    reference::ExactReference,
};

/// An engine the measurements run on.
pub trait Measured: MpsEngine {
    /// Why the engine refused a request, when `error` is a refusal rather
    /// than a failure: a refused impossible condition is the answer E16
    /// asks for, not an error.
    fn refusal(error: &Self::Error) -> Option<String>;
}

type Outcome = Result<(), Box<dyn Error>>;

pub fn quick() -> bool {
    std::env::var_os("QDK_ENGINE_QUALIFICATION_QUICK").is_some_and(|value| value == "1")
}

/// `full`, or `smaller` in quick mode.
fn sweep<T: Copy>(full: &[T], smaller: &[T]) -> Vec<T> {
    (if quick() { smaller } else { full }).to_vec()
}

fn seed(value: u64) -> SampleSeed {
    SampleSeed::try_from(value).unwrap_or_else(|error| panic!("seed {}", error.0))
}

fn apply_all<E: MpsEngine>(
    engine: &E,
    state: &mut E::State,
    operators: &[Operator],
) -> Result<(), E::Error> {
    operators
        .iter()
        .try_for_each(|operator| engine.apply(state, &operator.sites, &operator.matrix))
}

fn float(count: usize) -> f64 {
    f64::from(u32::try_from(count).unwrap_or(u32::MAX))
}

/// Reads of a state no operator was applied to: the first compute then
/// computes |0…0⟩ alone.
pub fn empty_compute<E: Measured>(report: &mut Report, engine: &E) {
    for width in sweep(&[2, 16, 64], &[2, 16]) {
        let item = |metric: &str| format!("width={width};{metric}");
        if let Err(error) = empty_reads(report, engine, width, &item) {
            report.error(&item("run"), error);
        }
    }
}

fn empty_reads<E: Measured>(
    report: &mut Report,
    engine: &E,
    width: usize,
    item: &dyn Fn(&str) -> String,
) -> Outcome {
    let state = engine.prepare(&circuits::capacity(width, 4)?)?;
    if let Some(shape) = report.time(&item("shape"), || engine.shape(&state)) {
        report.row(&item("max_bond"), shape.max_bond(), "");
    }
    if let Some(p) = report.time(&item("probability_of_one"), || {
        engine.probability_of_one(&state, 0)
    }) {
        report.row(&item("p_one_site_0"), p, "");
    }
    let z = circuits::string(vec![(width - 1, Pauli::Z)]).ok_or("Z")?;
    if let Some(value) = report.time(&item("expectation"), || engine.expectation(&state, &z)) {
        report.row(&item("z_last_site"), value.re, "");
    }
    let sites: Vec<_> = (0..width).collect();
    if let Some(values) = report.time(&item("sample"), || {
        engine.sample(&state, &sites, 8, seed(1))
    }) {
        let ones = values.iter().filter(|&&one| one).count();
        report.row(&item("sampled_ones"), ones, "count");
    }
    if let Some(fork) = report.time(&item("fork"), || engine.fork(&state))
        && let Some(p) = report.time(&item("fork_probability_of_one"), || {
            engine.probability_of_one(&fork, 0)
        })
    {
        report.row(&item("fork_p_one_site_0"), p, "");
    }
    Ok(())
}

/// H₂'s shape, from `mps-a`'s plan: about 1,404 measurements, each followed
/// by a read, over about 48,000 gates.
const H2_READS: usize = 1_404;
const H2_GATES: usize = 48_000;

/// The operators applied between two timed reads: H₂'s gates per read,
/// rounded down.
const BATCH: usize = H2_GATES / H2_READS;

/// Below this probability a value is not projected on: conditioning on it
/// would divide by a number truncation error dominates.
const UNLIKELY: f64 = 1.0e-3;

/// How a read's cost grows with the operators applied, on each of
/// `engines`, tagged `capture=<name>`, on the same circuit and χ. Without
/// capture, a read computes the state from |0…0⟩ with every operator, so a
/// terminal that reads after each of `M` measurements costs `O(M·G)`; with
/// it, a read computes only the operators applied since the last one. The
/// `extrapolation=h2` rows estimate H₂'s shot from a fit of these reads:
/// they are an extrapolation, beyond the operators measured and at this
/// width and χ, not a measurement.
pub fn read_cost<E: Measured>(report: &mut Report, engines: &[(&str, &E)]) {
    let width = 16;
    let (most, repeats) = if quick() { (256, 1) } else { (2048, 3) };
    for chi in sweep(&[8, 16, 32], &[8, 16]) {
        for &(capture, engine) in engines {
            let item = |metric: &str| format!("capture={capture};width={width};chi={chi};{metric}");
            if let Err(error) =
                reads_as_operators_accumulate(report, engine, width, chi, most, repeats, &item)
            {
                report.error(&item("run"), error);
            }
        }
    }
}

/// Applies a brickwork of `most` operators, and at 1, 2, 4, …
/// of them times the reads of [`time_reads`]; then fits the reads after a
/// change and extrapolates them to H₂'s shape.
fn reads_as_operators_accumulate<E: Measured>(
    report: &mut Report,
    engine: &E,
    width: usize,
    chi: usize,
    most: usize,
    repeats: usize,
    item: &dyn Fn(&str) -> String,
) -> Outcome {
    let checkpoints = most.ilog2() as usize + 1;
    let mut operators = circuits::brickwork(
        width,
        (most + checkpoints * repeats * BATCH) / (width - 1) + 2,
        false,
        7,
    )
    .into_iter();
    let mut state = engine.prepare(&circuits::capacity(width, chi)?)?;
    let (mut checkpoint, mut applied, mut logged) = (1, 0, 0);
    let mut applying = Duration::ZERO;
    let mut points: Vec<(usize, Duration)> = Vec::new();
    while applied < most {
        let operator = operators.next().ok_or("brickwork too short")?;
        let ((), duration) = timed(|| engine.apply(&mut state, &operator.sites, &operator.matrix))?;
        applying += duration;
        applied += 1;
        logged += 1;
        if applied != checkpoint && applied != most {
            continue;
        }
        checkpoint *= 2;
        let at = |metric: &str| item(&format!("operators={applied};{metric}"));
        report.row(&at("logged"), logged, "count");
        report.row(&at("apply_total"), milliseconds(applying), "ms");
        report.row(&at("device_mib"), or_unavailable(device_mib()), "MiB");
        let prefer_one = points.len() % 2 == 1;
        let (after_change, changes) = time_reads(
            report,
            engine,
            &mut state,
            &mut operators,
            (repeats, prefer_one),
            &at,
        )?;
        logged += changes;
        if let Some(&(before, earlier)) = points.last() {
            let growth = (after_change.as_secs_f64() / earlier.as_secs_f64()).ln()
                / (float(logged) / float(before)).ln();
            report.row(&at("growth_exponent"), format!("{growth:.3}"), "");
        }
        points.push((logged, after_change));
    }
    extrapolate(report, &points, applying, applied, item);
    Ok(())
}

/// Times the reads at one checkpoint: the read after the operators applied
/// since the last checkpoint, `repeats` reads each after [`BATCH`] more
/// operators (whose median it returns), a cached read, and a read after a
/// projection of site 0 on `prefer_one`'s value, or on the other if that one
/// has probability below [`UNLIKELY`]. Records the probability read before
/// the projection, and the value projected on: with and without capture the
/// states truncate differently, and stay comparable while both project on
/// the same values. Returns, too, how many operators it logged.
fn time_reads<E: Measured>(
    report: &mut Report,
    engine: &E,
    state: &mut E::State,
    operators: &mut impl Iterator<Item = Operator>,
    (repeats, prefer_one): (usize, bool),
    item: &dyn Fn(&str) -> String,
) -> Result<(Duration, usize), Box<dyn Error>> {
    let (_, accumulated) = timed(|| engine.probability_of_one(state, 0))?;
    report.row(
        &item("read_after_accumulation"),
        milliseconds(accumulated),
        "ms",
    );
    let mut changes = 0;
    let after_change = median(repeats, || -> Result<Duration, Box<dyn Error>> {
        for _ in 0..BATCH {
            let operator = operators.next().ok_or("brickwork too short")?;
            engine.apply(state, &operator.sites, &operator.matrix)?;
            changes += 1;
        }
        Ok(timed(|| engine.probability_of_one(state, 0))?.1)
    })?;
    report.row(
        &item(&format!("read_after_{BATCH}_operators")),
        milliseconds(after_change),
        "ms",
    );
    let cached = median(repeats, || {
        timed(|| engine.probability_of_one(state, 0)).map(|(_, duration)| duration)
    })?;
    report.row(&item("cached_read"), milliseconds(cached), "ms");
    report.row(&item("max_bond"), engine.shape(state)?.max_bond(), "");
    let p_one = engine.probability_of_one(state, 0)?;
    report.row(&item("p_one_site_0"), p_one, "");
    let one = if prefer_one {
        p_one >= UNLIKELY
    } else {
        1.0 - p_one < UNLIKELY
    };
    report.row(&item("projected_one"), one, "");
    engine.project(state, 0, one, if one { p_one } else { 1.0 - p_one })?;
    changes += 1;
    let (_, after_projection) = timed(|| engine.probability_of_one(state, 0))?;
    report.row(
        &item("read_after_projection"),
        milliseconds(after_projection),
        "ms",
    );
    Ok((after_change, changes))
}

/// Fits `t = a·Mᵇ` to the reads after a change, by least squares on
/// `ln t` over the last four checkpoints, where `M` is the operators
/// logged; then sums the fit over H₂'s reads, the `i`-th after
/// `i·`[`H2_GATES`]`/`[`H2_READS`] operators, and scales the mean apply to
/// H₂'s gates. Rows tagged `extrapolation=h2`.
fn extrapolate(
    report: &mut Report,
    points: &[(usize, Duration)],
    applying: Duration,
    applied: usize,
    item: &dyn Fn(&str) -> String,
) {
    let at = |metric: &str| {
        item(&format!(
            "extrapolation=h2;reads={H2_READS};gates={H2_GATES};{metric}"
        ))
    };
    let fitted: Vec<(f64, f64)> = points
        .iter()
        .rev()
        .take(4)
        .filter(|(_, duration)| !duration.is_zero())
        .map(|&(logged, duration)| (float(logged).ln(), duration.as_secs_f64().ln()))
        .collect();
    let Some((exponent, coefficient)) = fit(&fitted) else {
        report.note(&at("fit"), "fewer than two distinct checkpoints");
        return;
    };
    report.row(&at("fit_exponent"), format!("{exponent:.3}"), "");
    let reads: f64 = (1..=H2_READS)
        .map(|read| coefficient * (float(read * H2_GATES) / float(H2_READS)).powf(exponent))
        .sum();
    report.row(&at("reads_total"), format!("{reads:.3}"), "s");
    let apply = applying.as_secs_f64() / float(applied) * float(H2_GATES);
    report.row(&at("apply_total"), format!("{apply:.3}"), "s");
}

/// The least-squares line `y = b·x + ln a` through `points`, as `(b, a)`.
fn fit(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = float(points.len());
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    if points.len() < 2 || sxx <= 0.0 {
        return None;
    }
    let sxy: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    let exponent = sxy / sxx;
    Some((exponent, (mean_y - exponent * mean_x).exp()))
}

/// What a fork costs (D21): a new native state with every operator applied
/// again, so its first read computes from |0…0⟩; and how far a fork is from
/// a parent that was read, so captured, along the way.
pub fn fork_cost<E: Measured>(report: &mut Report, engine: &E) {
    for width in sweep(&[8, 32, 64], &[8, 32]) {
        for chi in sweep(&[16, 64, 256], &[16, 64]) {
            let item = |metric: &str| format!("width={width};chi={chi};{metric}");
            if let Err(error) = time_fork(report, engine, width, chi, &item) {
                report.error(&item("run"), error);
            }
        }
    }
}

fn time_fork<E: Measured>(
    report: &mut Report,
    engine: &E,
    width: usize,
    chi: usize,
    item: &dyn Fn(&str) -> String,
) -> Outcome {
    let operators = circuits::brickwork(width, circuits::layers_to_saturate(chi), false, 11);
    report.row(&item("operators"), operators.len(), "count");
    let capacity = circuits::capacity(width, chi)?;
    let (state, build) = timed(|| {
        let mut state = engine.prepare(&capacity)?;
        apply_all(engine, &mut state, &operators).map(|()| state)
    })?;
    report.row(&item("build"), milliseconds(build), "ms");
    let site = width / 2;
    let (p, first) = timed(|| engine.probability_of_one(&state, site))?;
    report.row(&item("first_read"), milliseconds(first), "ms");
    let (_, cached) = timed(|| engine.probability_of_one(&state, site))?;
    report.row(&item("cached_read"), milliseconds(cached), "ms");
    report.row(&item("max_bond"), engine.shape(&state)?.max_bond(), "");
    let (fork, forking) = timed(|| engine.fork(&state))?;
    report.row(&item("fork"), milliseconds(forking), "ms");
    let (q, fork_first) = timed(|| engine.probability_of_one(&fork, site))?;
    report.row(&item("fork_first_read"), milliseconds(fork_first), "ms");
    let (_, fork_cached) = timed(|| engine.probability_of_one(&fork, site))?;
    report.row(&item("fork_cached_read"), milliseconds(fork_cached), "ms");
    report.row(&item("fork_difference"), scientific((p - q).abs()), "");
    report.row(&item("device_mib"), or_unavailable(device_mib()), "MiB");
    drop(fork);
    fork_after_reads(report, engine, &capacity, &operators, (site, p), item)
}

/// A state built with a read after every [`BATCH`] operators, as a terminal
/// reads after each measurement, against its fork. With capture, the parent
/// is computed in steps, each from the last captured MPS, and the fork in
/// one compute from |0…0⟩ with every operator, so under truncation the two
/// may differ; the rows record by how much, and how far the stepped parent
/// is from the state computed once (`plain`: the site's probability).
fn fork_after_reads<E: Measured>(
    report: &mut Report,
    engine: &E,
    capacity: &tensornet::MpsShape,
    operators: &[Operator],
    (site, plain): (usize, f64),
    item: &dyn Fn(&str) -> String,
) -> Outcome {
    let mut state = engine.prepare(capacity)?;
    let mut reads = 0;
    for batch in operators.chunks(BATCH) {
        apply_all(engine, &mut state, batch)?;
        engine.probability_of_one(&state, site)?;
        reads += 1;
    }
    report.row(&item("stepped_reads"), reads, "count");
    let p = engine.probability_of_one(&state, site)?;
    report.row(
        &item("stepped_difference_from_plain"),
        scientific((p - plain).abs()),
        "",
    );
    report.row(
        &item("stepped_max_bond"),
        engine.shape(&state)?.max_bond(),
        "",
    );
    let fork = engine.fork(&state)?;
    let q = engine.probability_of_one(&fork, site)?;
    report.row(
        &item("stepped_fork_difference"),
        scientific((p - q).abs()),
        "",
    );
    report.row(
        &item("stepped_fork_max_bond"),
        engine.shape(&fork)?.max_bond(),
        "",
    );
    Ok(())
}

/// A long-lived state against [`ExactReference`]: each cycle applies two
/// random operators and reads every site; every tenth also samples and
/// forks. A compute frees the previous compute's buffers (E17), so the
/// device memory should not grow with the cycles.
pub fn free_after_compute<E: Measured>(report: &mut Report, engine: &E) {
    let cycles = if quick() { 40 } else { 200 };
    if let Err(error) = cycle(report, engine, 10, cycles) {
        report.error("run", error);
    }
}

fn cycle<E: Measured>(report: &mut Report, engine: &E, width: usize, cycles: usize) -> Outcome {
    let reference = ExactReference::new();
    let capacity = exact_capacity(width)?;
    let mut state = engine.prepare(&capacity)?;
    let mut exact = reference.prepare(&capacity)?;
    let mut draws = circuits::Draws::new(23);
    let zz = circuits::string(vec![(0, Pauli::Z), (width - 1, Pauli::Z)]).ok_or("ZZ")?;
    let sites: Vec<_> = (0..width).collect();
    let marks = [0, cycles / 4, cycles / 2, 3 * cycles / 4, cycles - 1];
    let (mut worst, mut beyond) = (0.0f64, 0usize);
    let mut compare = |error: f64| {
        worst = worst.max(error);
        beyond += usize::from(error > TOLERANCE);
    };
    for at in 0..cycles {
        for operator in circuits::random_operators(&mut draws, width) {
            engine.apply(&mut state, &operator.sites, &operator.matrix)?;
            reference.apply(&mut exact, &operator.sites, &operator.matrix)?;
        }
        for site in 0..width {
            let p = engine.probability_of_one(&state, site)?;
            compare((p - reference.probability_of_one(&exact, site)?).abs());
        }
        let value = engine.expectation(&state, &zz)?;
        compare((value - reference.expectation(&exact, &zz)?).norm());
        if at % 10 == 9 {
            let values = engine.sample(&state, &sites, 16, seed(1 + u64::try_from(at)?))?;
            if values.len() != 16 * width {
                report.error(&format!("cycle={at};sample"), "wrong number of values");
            }
            let fork = engine.fork(&state)?;
            let p = engine.probability_of_one(&fork, width - 1)?;
            compare((p - reference.probability_of_one(&exact, width - 1)?).abs());
        }
        if marks.contains(&at) {
            report.row(
                &format!("cycle={at};device_mib"),
                or_unavailable(device_mib()),
                "MiB",
            );
        }
    }
    report.row("cycles", cycles, "count");
    report.row("max_abs_error", scientific(worst), "");
    report.row("beyond_tolerance", beyond, "count");
    Ok(())
}

/// The engine's realized shape and values against [`ExactReference`]'s,
/// under the qualified cutoff (1e-10) and [`TOLERANCE`] (1e-9), on
/// brickwork circuits with a long-range CNOT in each layer.
pub fn shape_and_tolerance<E: Measured>(report: &mut Report, engine: &E) {
    for width in sweep(&[4, 8, 12, 16], &[4, 8]) {
        for depth in sweep(&[2, 4, 8], &[2, 4]) {
            for seed in sweep(&[0u64, 1], &[0]) {
                let item =
                    |metric: &str| format!("width={width};depth={depth};seed={seed};{metric}");
                if let Err(error) = agreement(report, engine, width, depth, seed, &item) {
                    report.error(&item("run"), error);
                }
            }
        }
    }
}

fn agreement<E: Measured>(
    report: &mut Report,
    engine: &E,
    width: usize,
    depth: usize,
    seed: u64,
    item: &dyn Fn(&str) -> String,
) -> Outcome {
    let reference = ExactReference::new();
    let capacity = exact_capacity(width)?;
    let operators = circuits::brickwork(width, depth, true, 100 + seed);
    let mut state = engine.prepare(&capacity)?;
    apply_all(engine, &mut state, &operators)?;
    let mut exact = reference.prepare(&capacity)?;
    apply_all(&reference, &mut exact, &operators)?;
    let (realized, expected) = (engine.shape(&state)?, reference.shape(&exact)?);
    let differing = (0..width - 1)
        .filter(|&cut| realized.bond_dim(cut) != expected.bond_dim(cut))
        .count();
    report.row(&item("shape_equal"), realized == expected, "");
    report.row(&item("differing_cuts"), differing, "count");
    report.row(&item("engine_max_bond"), realized.max_bond(), "");
    report.row(&item("reference_max_bond"), expected.max_bond(), "");
    let mut worst_p = 0.0f64;
    for site in 0..width {
        let p = engine.probability_of_one(&state, site)?;
        worst_p = worst_p.max((p - reference.probability_of_one(&exact, site)?).abs());
    }
    let mut worst = (0.0f64, String::new());
    for (name, observable) in circuits::observables(width) {
        let value = engine.expectation(&state, &observable)?;
        let error = (value - reference.expectation(&exact, &observable)?).norm();
        if error > worst.0 {
            worst = (error, name);
        }
    }
    report.row(&item("max_p_one_error"), scientific(worst_p), "");
    report.row(&item("max_expectation_error"), scientific(worst.0), "");
    let name = if worst.1.is_empty() { "none" } else { &worst.1 };
    report.row(&item("worst_observable"), name, "");
    report.row(
        &item("within_tolerance"),
        worst_p.max(worst.0) <= TOLERANCE,
        "",
    );
    Ok(())
}

/// Conditions of probability zero on GHZ states, alone or after noise
/// layers and their inverse: the engine should refuse them (E16), and the
/// noise layers show how far from zero rounding takes such a weight.
pub fn impossible_condition<E: Measured>(report: &mut Report, engine: &E) {
    for width in sweep(&[3, 8, 16, 32], &[3, 16]) {
        for noise in [0, 8] {
            let item = |metric: &str| format!("width={width};noise_layers={noise};{metric}");
            if let Err(error) = condition_ghz(report, engine, width, noise, &item) {
                report.error(&item("run"), error);
            }
        }
    }
}

fn condition_ghz<E: Measured>(
    report: &mut Report,
    engine: &E,
    width: usize,
    noise: usize,
    item: &dyn Fn(&str) -> String,
) -> Outcome {
    // Eight noise layers raise a bond to at most 2⁴, so the χ = 64 capacity
    // never truncates.
    let mut operators = vec![hadamard(0)];
    operators.extend((0..width - 1).map(|site| cnot(site, site + 1)));
    let layers = circuits::brickwork(width, noise, false, 31);
    operators.extend(layers.iter().cloned());
    operators.extend(layers.iter().rev().map(Operator::adjoint));
    let mut state = engine.prepare(&circuits::capacity(width, 64)?)?;
    apply_all(engine, &mut state, &operators)?;
    let impossible = [(0, true), (width - 1, false)];
    let weight = engine.expectation(&state, &circuits::projector(&impossible).ok_or("Π")?)?;
    report.row(&item("weight"), scientific(weight.norm()), "");
    conditional(report, engine, &state, 1, &impossible, item);
    let possible = [(0, true), (width - 1, true)];
    if let Some(value) = report.record(
        &item("possible_value"),
        engine.conditional_probability_of_one(&state, 1, &possible),
    ) {
        report.row(&item("possible_value"), value, "");
        report.row(&item("possible_error"), scientific((value - 1.0).abs()), "");
    }
    Ok(())
}

/// Records whether the engine refuses P(One) of `site` given `given`, and
/// the value it gives when it does not.
fn conditional<E: Measured>(
    report: &mut Report,
    engine: &E,
    state: &E::State,
    site: usize,
    given: &[(usize, bool)],
    item: &dyn Fn(&str) -> String,
) -> Option<f64> {
    match engine.conditional_probability_of_one(state, site, given) {
        Ok(value) => {
            report.row(&item("refused"), false, "");
            report.row(&item("value"), value, "");
            Some(value)
        }
        Err(error) => {
            if let Some(reason) = E::refusal(&error) {
                report.row(&item("refused"), true, "");
                report.note(&item("refused"), reason);
            } else {
                report.error(&item("value"), error);
            }
            None
        }
    }
}

/// Conditions whose probability shrinks with their length, from about
/// `p` to about 1e-15 for the longest: where the engine starts refusing
/// (E16's threshold is 1e-12), and how accurate it is above that.
pub fn rare_condition<E: Measured>(report: &mut Report, engine: &E) {
    let width = if quick() { 8 } else { 12 };
    if let Err(error) = condition_rare(report, engine, width) {
        report.error(&format!("width={width};run"), error);
    }
}

/// Each site is One with probability `p = 10^(−15/(width − 1))`; CZs on
/// every neighbouring pair entangle the sites without changing that, and a
/// small `Ry` on every site then makes the chain's probabilities depend on
/// the CZs' phases, so that the conditionals are not the marginals.
fn condition_rare<E: Measured>(report: &mut Report, engine: &E, width: usize) -> Outcome {
    let p = 10f64.powf(-15.0 / (float(width) - 1.0));
    let theta = circuits::ry_angle(p);
    let mut operators: Vec<_> = (0..width).map(|site| ry(site, theta)).collect();
    operators.extend((0..width - 1).step_by(2).map(|site| cz(site, site + 1)));
    operators.extend((1..width - 1).step_by(2).map(|site| cz(site, site + 1)));
    operators.extend((0..width).map(|site| ry(site, 0.02)));
    let reference = ExactReference::new();
    let capacity = exact_capacity(width)?;
    let mut state = engine.prepare(&capacity)?;
    apply_all(engine, &mut state, &operators)?;
    let mut exact = reference.prepare(&capacity)?;
    apply_all(&reference, &mut exact, &operators)?;
    for length in 1..width {
        let item = |metric: &str| format!("width={width};given={length};{metric}");
        let given: Vec<_> = (0..length).map(|site| (site, true)).collect();
        let projector = circuits::projector(&given).ok_or("Π")?;
        let weight = reference.expectation(&exact, &projector)?.re;
        report.row(&item("weight"), scientific(weight), "");
        let expected = reference.conditional_probability_of_one(&exact, length, &given)?;
        report.row(&item("expected"), expected, "");
        if let Some(value) = conditional(report, engine, &state, length, &given, &item) {
            let error = (value - expected).abs();
            report.row(&item("abs_error"), scientific(error), "");
            report.row(&item("rel_error"), scientific(error / expected), "");
        }
    }
    Ok(())
}
