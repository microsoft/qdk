// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What grows with a shot's history, and where the time goes, along H₂'s
//! pattern: [`BATCH`] gates of a random brickwork, a read, then a
//! projection, step after step, at H₂'s width 29 and χ 32. H₂'s first shot
//! on the A100 failed at its 1,252nd measurement, when cuTensorNet asked for
//! a 42 GB scratch: the recommended workspace had grown by about 32 MiB per
//! measurement, and the time per measurement from 1.8 s to 4.7 s.
//!
//! Each variant runs on an engine of its own
//! ([`Availability::mps_engine_variant`](qdk_cutensornet::Availability::mps_engine_variant)):
//!
//! - `current`: the engine `mps_engine` creates;
//! - `minimum_workspace`: each compute given cuTensorNet's minimum scratch
//!   instead of its recommended one;
//! - `restart_10`, `restart_50`: the native state restarted from its
//!   computed MPS every 10 or 50 computes (one per step here), which drops
//!   the operators the native state has accumulated.
//!
//! Every [`MARK`] steps it records, from the state's
//! [`StateProfile`](qdk_cutensornet::StateProfile):
//!
//! - the minimum and recommended workspace of the last compute, in bytes;
//!   with a restart period that divides [`MARK`], the mark falls on the
//!   last compute before a restart, the largest of the period;
//! - the time per step spent in each kind of call since the last mark
//!   (`<kind>_ms`, the profile's fields), and `unprofiled_ms`, the step's
//!   time outside them: the engine's own checks and bookkeeping, and the
//!   test's;
//! - the process's device memory, after the step's projection;
//! - the state's live device allocations and operator tensors, and its
//!   computes and restarts so far;
//! - the step's P(One), which every variant should agree on, up to
//!   rounding.
//!
//! The time per step excludes the records. Then, per variant, the
//! least-squares slopes per step over the marks, the largest recommended
//! workspace, and an extrapolation to H₂'s 1,404 measurements of the fitted
//! lines (tagged `extrapolation=h2`): estimates, not measurements.

use crate::circuits;
use crate::measure::{BATCH, H2_READS, quick};
use crate::memory::{Ending, MARK, end, float};
use crate::report::{Report, device_mib, or_unavailable};
use qdk_cutensornet::{CuTensorNetEngine, EngineVariant, StateProfile, discover};
use std::{
    error::Error,
    num::NonZeroUsize,
    time::{Duration, Instant},
};
use tensornet::MpsEngine;

const WIDTH: usize = 29;
const CHI: usize = 32;

/// The steps of each variant: 300, or 30 in quick mode.
pub fn steps() -> usize {
    if quick() { 30 } else { 300 }
}

/// The variants the qualification runs, in order.
pub fn variants() -> Vec<(&'static str, EngineVariant)> {
    let restart = |period| EngineVariant {
        restart_period: NonZeroUsize::new(period),
        ..EngineVariant::default()
    };
    vec![
        ("current", EngineVariant::default()),
        (
            "minimum_workspace",
            EngineVariant {
                minimum_workspace: true,
                ..EngineVariant::default()
            },
        ),
        ("restart_10", restart(10)),
        ("restart_50", restart(50)),
    ]
}

/// The profile's times, by kind of call.
fn kinds(profile: &StateProfile) -> [(&'static str, Duration); 9] {
    [
        ("apply", profile.apply),
        ("prepare", profile.prepare),
        ("workspace_query", profile.workspace_query),
        ("allocate", profile.allocate),
        ("compute", profile.compute),
        ("capture", profile.capture),
        ("free", profile.free),
        ("reads", profile.reads),
        ("restart", profile.restart),
    ]
}

/// The values at each mark that the slopes are fitted to, by metric.
#[derive(Default)]
struct Series {
    points: Vec<(&'static str, Vec<(f64, f64)>)>,
}

impl Series {
    fn push(&mut self, metric: &'static str, step: usize, value: f64) {
        let at = (float(step), value);
        match self.points.iter_mut().find(|(name, _)| *name == metric) {
            Some((_, points)) => points.push(at),
            None => self.points.push((metric, vec![at])),
        }
    }

    fn get(&self, metric: &str) -> &[(f64, f64)] {
        self.points
            .iter()
            .find(|(name, _)| *name == metric)
            .map_or(&[], |(_, points)| points)
    }
}

/// Runs every variant of [`variants`] for [`steps`] steps; an engine error
/// is a row, and the next variant runs.
pub fn history_growth(report: &mut Report) {
    let availability = discover().unwrap_or_else(|error| panic!("no cuTensorNet: {error}"));
    report.row("steps", steps(), "");
    for (name, variant) in variants() {
        let item = |metric: &str| format!("variant={name};{metric}");
        report.row(
            &item("device_mib_before_engine"),
            or_unavailable(device_mib()),
            "MiB",
        );
        let engine = match availability.mps_engine_variant(variant) {
            Ok(engine) => engine,
            Err(error) => {
                report.error(&item("engine"), error);
                continue;
            }
        };
        let mut series = Series::default();
        if let Err(error) = run(report, &engine, steps(), &item, &mut series) {
            report.error(&item("run"), error);
        }
        // Also after an error: the trend before it may explain it.
        trends(report, &item, &series);
        if let Err(error) = engine.close() {
            report.error(&item("closed"), error);
        }
        report.row(
            &item("device_mib_after_close"),
            or_unavailable(device_mib()),
            "MiB",
        );
    }
}

fn run(
    report: &mut Report,
    engine: &CuTensorNetEngine,
    steps: usize,
    item: &dyn Fn(&str) -> String,
    series: &mut Series,
) -> Result<(), Box<dyn Error>> {
    let site = WIDTH / 2;
    // A brickwork layer has at least one gate per site.
    let layers = steps * BATCH / WIDTH + 2;
    let mut gates = circuits::brickwork(WIDTH, layers, false, 41).into_iter();
    let mut state = engine.prepare(&circuits::capacity(WIDTH, CHI)?)?;
    report.row(
        &item("step=0;device_mib"),
        or_unavailable(device_mib()),
        "MiB",
    );
    let mut last = engine.profile(&state)?;
    let (mut since_mark, mut total, mut previous) = (Duration::ZERO, Duration::ZERO, 0);
    for step in 1..=steps {
        let start = Instant::now();
        for _ in 0..BATCH {
            let gate = gates.next().ok_or("brickwork too short")?;
            engine.apply(&mut state, &gate.sites, &gate.matrix)?;
        }
        let p_one = engine.probability_of_one(&state, site)?;
        end(engine, &mut state, Ending::Projection, site, step, p_one)?;
        let elapsed = start.elapsed();
        since_mark += elapsed;
        total += elapsed;
        if step % MARK != 0 && step != steps {
            continue;
        }
        let at = |metric: &str| item(&format!("step={step};{metric}"));
        // To compare the variants' values: a restart changes none.
        report.row(&at("p_one"), format!("{p_one:.15e}"), "");
        let per_step = |duration: Duration| duration.as_secs_f64() * 1e3 / float(step - previous);
        let profile = engine.profile(&state)?;
        let wall = per_step(since_mark);
        report.row(&at("ms_per_step"), format!("{wall:.3}"), "ms");
        series.push("ms_per_step", step, wall);
        let mut profiled = 0.0;
        for ((kind, now), (_, before)) in kinds(&profile).into_iter().zip(kinds(&last)) {
            let spent = per_step(now.saturating_sub(before));
            profiled += spent;
            report.row(&at(&format!("{kind}_ms")), format!("{spent:.3}"), "ms");
            series.push(kind, step, spent);
        }
        let unprofiled = wall - profiled;
        report.row(&at("unprofiled_ms"), format!("{unprofiled:.3}"), "ms");
        series.push("unprofiled", step, unprofiled);
        for (metric, bytes) in [
            ("minimum_workspace", profile.minimum_workspace_bytes),
            ("recommended_workspace", profile.recommended_workspace_bytes),
        ] {
            report.row(&at(&format!("{metric}_bytes")), bytes, "B");
            // In KiB, which fit a u32 up to 4 TiB.
            let kib = u32::try_from(bytes >> 10).unwrap_or(u32::MAX);
            series.push(metric, step, f64::from(kib) / 1024.0);
        }
        let mib = device_mib();
        report.row(&at("device_mib"), or_unavailable(mib), "MiB");
        if let Some(mib) = mib.and_then(|mib| u32::try_from(mib).ok()) {
            series.push("device_mib", step, f64::from(mib));
        }
        report.row(&at("live_allocations"), profile.live_allocations, "count");
        series.push("live_allocations", step, float(profile.live_allocations));
        report.row(&at("operator_tensors"), profile.operator_tensors, "count");
        report.row(&at("computes"), profile.computes, "count");
        report.row(&at("restarts"), profile.restarts, "count");
        last = profile;
        since_mark = Duration::ZERO;
        previous = step;
    }
    let mean = total.as_secs_f64() * 1e3 / float(steps);
    report.row(&item("ms_per_step"), format!("{mean:.3}"), "ms");
    drop(state);
    Ok(())
}

/// The least-squares slope per step of each metric, the largest
/// recommended workspace, and the fitted lines' extrapolation to H₂.
fn trends(report: &mut Report, item: &dyn Fn(&str) -> String, series: &Series) {
    let mut metrics = vec![
        ("device_mib", "MiB/step"),
        ("recommended_workspace", "MiB/step"),
        ("minimum_workspace", "MiB/step"),
        ("live_allocations", "count/step"),
        ("ms_per_step", "ms/step"),
    ];
    metrics.extend(kinds(&StateProfile::default()).map(|(kind, _)| (kind, "ms/step")));
    metrics.push(("unprofiled", "ms/step"));
    for (metric, unit) in metrics {
        let name = format!("slope;{metric}");
        match fit(series.get(metric)) {
            Some((_, slope)) => report.row(&item(&name), format!("{slope:.4}"), unit),
            None => report.note(&item(&name), "fewer than two marks"),
        }
    }
    let peak = series
        .get("recommended_workspace")
        .iter()
        .map(|point| point.1)
        .fold(None, |peak: Option<f64>, value| {
            Some(peak.map_or(value, |peak| peak.max(value)))
        });
    if let Some(peak) = peak {
        report.row(
            &item("peak_recommended_workspace_mib"),
            format!("{peak:.1}"),
            "MiB",
        );
    }
    let h2 = float(H2_READS);
    let extrapolation = |metric: &str| {
        item(&format!(
            "extrapolation=h2;measurements={H2_READS};{metric}"
        ))
    };
    if let Some((intercept, slope)) = fit(series.get("ms_per_step")) {
        // The sum of the fitted line over steps 1 to 1,404.
        let seconds = (intercept * h2 + slope * h2 * (h2 + 1.0) / 2.0) / 1e3;
        report.row(
            &extrapolation("seconds_per_shot"),
            format!("{seconds:.1}"),
            "s",
        );
    }
    if let Some((intercept, slope)) = fit(series.get("recommended_workspace")) {
        let mib = intercept + slope * h2;
        report.row(
            &extrapolation("recommended_workspace_mib"),
            format!("{mib:.0}"),
            "MiB",
        );
    }
}

/// The least-squares line through `points`, as (intercept, slope), or
/// `None` with fewer than two distinct abscissas.
fn fit(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    let n = float(points.len());
    let mean_x = points.iter().map(|p| p.0).sum::<f64>() / n;
    let mean_y = points.iter().map(|p| p.1).sum::<f64>() / n;
    let sxx: f64 = points.iter().map(|p| (p.0 - mean_x).powi(2)).sum();
    if points.len() < 2 || sxx <= 0.0 {
        return None;
    }
    let sxy: f64 = points.iter().map(|p| (p.0 - mean_x) * (p.1 - mean_y)).sum();
    let slope = sxy / sxx;
    Some((mean_y - slope * mean_x, slope))
}
