// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The FIR optimization stage.
//!
//! This is the one assembly of the FIR stage in the tree: the producer lowers a
//! callable body into a graph, the shared rule cancels what it can, and the
//! return leg subtracts the statements the rewrite retired. Two assemblies
//! would be two definitions of what the stage is, free to disagree, so the
//! measurement that prices this stage drives this function rather than wiring
//! the three pieces up a second time.
//!
//! The caller supplies the point. This function edits FIR in place and knows
//! nothing about where in the transform schedule it is being run, because
//! `qsc_fir_transforms::run_pipeline_with_diagnostics_and_optimize` owns that
//! and hands the store through a closure. Item pruning, node collection, and
//! the execution-graph rebuild all run after that point, so a retirement here
//! does not have to clean up after itself.
//!
//! Declining is the ordinary case. The producer accepts straight-line bodies
//! and supported loops, but refuses unsupported loop shapes, controlled calls
//! and valued calls with quantum effects. The return leg refuses a retirement
//! it cannot express as a removed statement. Every refusal leaves the body
//! exactly as it arrived.
//!
//! Off unless a caller asks, for the same reason the stage below is: cancelling
//! a pair is sound about the unitary and wrong about the device, and the
//! compiler cannot see the noise model from here.

use qsc_fir::fir::{ItemKind, LocalItemId, PackageId, PackageStore, StoreItemId};

use crate::fir_qhdg::{lower_callable_with_census, retire_into_fir};
use crate::pass_pipeline::{Cancellation, OptimizationLevel, cancel_inverse_pairs};

/// What the stage did with one package.
///
/// For measurement rather than control flow. The stage declines silently by
/// design, so without a report the difference between declining on everything
/// and helping on everything is invisible from outside — which is how the first
/// wiring of the stage below passed every test while doing nothing.
///
/// Three outcomes rather than two. A body the producer lowered and the rule
/// found nothing to remove in is not a decline, and reporting it as one would
/// blame the producer for a program that simply held no redundancy. Without the
/// third count a stage that lowered everything and removed nothing reports
/// `{ 0, 0 }`, which is the same report a stage that never ran gives.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StageReport {
    /// Statements the return leg removed.
    pub edits: usize,
    /// Bodies the stage could not act on: the producer refused, the rewrite
    /// driver refused a patch, or the return leg could not express the
    /// retirement as a removed statement.
    pub declined: usize,
    /// Bodies the producer lowered and the rule left alone.
    pub unchanged: usize,
}

/// Cancels adjacent inverse pairs in every callable of `package`, or leaves
/// them alone.
pub fn optimize(
    store: &mut PackageStore,
    package: PackageId,
    level: OptimizationLevel,
) -> StageReport {
    if level == OptimizationLevel::None {
        return StageReport::default();
    }

    let mut report = StageReport::default();
    let callables: Vec<LocalItemId> = store
        .get(package)
        .items
        .iter()
        .filter_map(|(id, item)| matches!(item.kind, ItemKind::Callable(_)).then_some(id))
        .collect();

    for item in callables {
        let callable = StoreItemId { package, item };
        let Ok(mut lowered) = lower_callable_with_census(store, callable).0 else {
            report.declined += 1;
            continue;
        };
        match cancel_inverse_pairs(&mut lowered.graph) {
            Ok(Cancellation::Changed) => {}
            Ok(Cancellation::NoFacts | Cancellation::Unchanged) => {
                report.unchanged += 1;
                continue;
            }
            Err(_) => {
                report.declined += 1;
                continue;
            }
        }
        match retire_into_fir(store, &lowered.graph, &lowered) {
            Ok(edits) => report.edits += edits,
            Err(_) => report.declined += 1,
        }
    }
    report
}
