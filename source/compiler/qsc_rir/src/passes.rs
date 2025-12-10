// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod build_dominator_graph;
mod defer_meas;
mod insert_alloca_load;
mod prune_unneeded_stores;
mod reindex_qubits;
mod remap_block_ids;
mod simplify_control_flow;
mod ssa_check;
mod ssa_transform;
mod type_check;
mod unreachable_code_check;

use build_dominator_graph::build_dominator_graph;
use defer_meas::defer_measurements;
use qsc_data_structures::target::TargetCapabilityFlags;
use reindex_qubits::reindex_qubits;
use remap_block_ids::remap_block_ids;
use simplify_control_flow::simplify_control_flow;
use ssa_check::check_ssa_form;
use ssa_transform::transform_to_ssa;
pub use type_check::check_types;
pub use unreachable_code_check::check_unreachable_code;

use crate::{
    passes::{
        insert_alloca_load::insert_alloca_load_instrs, prune_unneeded_stores::prune_unneeded_stores,
    },
    rir::Program,
    utils::build_predecessors_map,
};

/// Run the default set of RIR check and transformation passes.
/// This includes:
/// - Simplifying control flow
/// - Checking for unreachable code
/// - Checking types
/// - Remapping block IDs
/// - Transforming the program to SSA form
/// - Checking that the program is in SSA form
/// - If the target has no adaptive capability, reindexing qubit IDs and removing resets.
/// - If the target has no mid-program measurement capability, deferring measurements to the end of the program.
pub fn check_and_transform(program: &mut Program) {
    simplify_control_flow(program);
    check_unreachable_code(program);
    check_types(program);
    remap_block_ids(program);

    if program.config.capabilities.is_advanced() {
        prune_unneeded_stores(program);
        insert_alloca_load_instrs(program);
    } else {
        let preds = build_predecessors_map(program);
        transform_to_ssa(program, &preds);
        let doms = build_dominator_graph(program, &preds);
        check_ssa_form(program, &preds, &doms);
        check_unreachable_code(program);
        check_types(program);

        // Run the RIR passes that are necessary for targets with no mid-program measurement.
        // This requires that qubits are not reused after measurement or reset, so qubit ids must be reindexed.
        // This also requires that the program has no loops and block ids form a topological ordering on a
        // directed acyclic graph.
        if program.config.capabilities == TargetCapabilityFlags::empty() {
            reindex_qubits(program);
            defer_measurements(program);
        }
    }
}
