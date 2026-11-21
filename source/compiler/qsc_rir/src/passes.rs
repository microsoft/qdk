// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

mod defer_meas;
mod insert_alloca_load;
mod prune_unneeded_stores;
mod reindex_qubits;
mod remap_block_ids;
mod result_literal;
mod simplify_control_flow;
mod ssa_transform;

#[cfg(debug_assertions)]
mod build_dominator_graph;
#[cfg(debug_assertions)]
mod ssa_check;
#[cfg(debug_assertions)]
mod type_check;
#[cfg(debug_assertions)]
mod unreachable_code_check;

use defer_meas::defer_measurements;
use qsc_data_structures::target::{Profile, TargetCapabilityFlags};
use reindex_qubits::reindex_qubits;
use remap_block_ids::remap_block_ids;
use simplify_control_flow::simplify_control_flow;
use ssa_transform::transform_to_ssa;

#[cfg(debug_assertions)]
use build_dominator_graph::build_dominator_graph;
#[cfg(debug_assertions)]
use ssa_check::check_ssa_form;
#[cfg(debug_assertions)]
use type_check::check_types;
#[cfg(debug_assertions)]
use unreachable_code_check::check_unreachable_code;

use crate::{
    passes::{
        insert_alloca_load::insert_alloca_load_instrs,
        prune_unneeded_stores::prune_unneeded_stores, result_literal::transform_result_literals,
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
/// - If the target has no reset capability, reindexing qubit IDs and removing resets.
/// - If the target has no mid-program measurement capability, deferring measurements to the end of the program.
pub fn check_and_transform(program: &mut Program) {
    simplify_control_flow(program);
    remap_block_ids(program);

    // These checks are used to validate the structure of the RIR during development.
    #[cfg(debug_assertions)]
    {
        check_unreachable_code(program);
        check_types(program);
    }

    prune_unneeded_stores(program);

    let uses_non_ssa_pipeline = program.config.capabilities >= Profile::Adaptive.into();
    if uses_non_ssa_pipeline {
        insert_alloca_load_instrs(program);
        transform_result_literals(program);
    } else {
        // Any transformation after this point assumes that the control-flow graph is acyclic.
        // This assumption is validated during construction of the predecessor map by passing `true`.
        let preds = build_predecessors_map(program, true);
        transform_to_ssa(program, &preds);

        // These checks are used to validate the structure of the RIR during development.
        // In particular, the SSA check can be expensive so we skip it in release builds.
        #[cfg(debug_assertions)]
        {
            let doms = build_dominator_graph(program, &preds);
            check_ssa_form(program, &preds, &doms);
            check_unreachable_code(program);
            check_types(program);
        }

        if program.config.capabilities == TargetCapabilityFlags::empty() {
            reindex_qubits(program);
            defer_measurements(program);
        }
    }
}
