// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Calculations make frequent use of conversion back and forth between f64 and u64.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

#[cfg(test)]
mod tests;

mod compiled_expression;
mod constants;
mod data;
pub(crate) mod error;
mod modeling;
mod optimization;
mod serialization;

use crate::estimates::{Overhead, PhysicalResourceEstimation};
use std::rc::Rc;

pub use self::modeling::{
    GateBasedPhysicalQubit, MajoranaQubit, PhysicalQubit, Protocol, ProtocolEvaluator,
    ProtocolSpecification, TFactory, floquet_code, load_protocol_from_specification,
    surface_code_gate_based, surface_code_measurement_based,
};
pub use self::optimization::TFactoryBuilder;
pub use self::{data::LogicalResourceCounts, error::Error};
use data::{EstimateType, JobParams};
pub use data::{LayoutReportData, PartitioningOverhead};
use serde::Serialize;

pub(crate) type Result<T> = std::result::Result<T, error::Error>;

pub fn estimate_physical_resources_from_json(
    logical_resources: &str,
    params: &str,
) -> std::result::Result<String, Error> {
    let logical_resources: LogicalResourceCounts = serde_json::from_str(logical_resources)
        .map_err(|e| error::Error::IO(error::IO::CannotParseJSON(e)))?;
    estimate_physical_resources(logical_resources, params)
}

pub fn estimate_physical_resources<
    L: Overhead + LayoutReportData + PartitioningOverhead + Serialize,
>(
    logical_resources: L,
    params: &str,
) -> Result<String> {
    let job_params_array = if params.is_empty() {
        vec![JobParams::default()]
    } else {
        serde_json::from_str(params).map_err(|e| error::Error::IO(error::IO::CannotParseJSON(e)))?
    };

    let mut results: Vec<String> = Vec::with_capacity(job_params_array.len());
    let logical_resources = Rc::new(logical_resources);
    for job_params in job_params_array {
        let result = estimate_single(logical_resources.clone(), job_params);
        match result {
            Ok(result) => results.push(
                serde_json::to_string(&result).expect("serializing to json string should succeed"),
            ),
            Err(err) => {
                results.push(serialize_error(err));
            }
        }
    }

    Ok(format!("[{}]", results.join(",")))
}

fn estimate_single<L: Overhead + LayoutReportData + PartitioningOverhead + Serialize>(
    logical_resources: Rc<L>,
    mut job_params: JobParams,
) -> Result<data::Success<L>> {
    let qubit = job_params.qubit_params().clone();

    let ftp = load_protocol_from_specification(job_params.qec_scheme_mut(), &qubit)?;
    let distillation_unit_templates = job_params
        .distillation_unit_specifications()
        .as_templates()?;
    // create error budget partitioning
    let partitioning = job_params
        .error_budget()
        .partitioning(logical_resources.as_ref())?;

    // The clone on the logical resources is on an Rc and therefore inexpensive,
    // the value is later used in creating the result object
    let mut estimation = PhysicalResourceEstimation::new(
        ftp,
        qubit,
        TFactoryBuilder::new(
            distillation_unit_templates,
            job_params.constraints().max_distillation_rounds,
        ),
        logical_resources.clone(),
    );
    if let Some(logical_depth_factor) = job_params.constraints().logical_depth_factor {
        estimation.set_logical_depth_factor(logical_depth_factor);
    }
    if let Some(max_t_factories) = job_params.constraints().max_t_factories {
        estimation.set_max_factories(max_t_factories);
    }
    if let Some(max_duration) = job_params.constraints().max_duration {
        estimation.set_max_duration(max_duration);
    }
    if let Some(max_physical_qubits) = job_params.constraints().max_physical_qubits {
        estimation.set_max_physical_qubits(max_physical_qubits);
    }

    match job_params.estimate_type() {
        EstimateType::Frontier => {
            if job_params.constraints().max_duration.is_some()
                || job_params.constraints().max_physical_qubits.is_some()
                || job_params.constraints().max_t_factories.is_some()
            {
                // We can technically handle those scenarios but do not see a practical use case for it.
                return Err(error::Error::InvalidInput(
                    error::InvalidInput::ConstraintsProvidedForFrontierEstimation,
                ));
            }

            let estimation_result = estimation
                .build_frontier(&partitioning)
                .map_err(std::convert::Into::into);
            estimation_result.map(|result| {
                data::Success::new_from_multiple(job_params, logical_resources, result)
            })
        }
        EstimateType::SinglePoint => {
            let estimation_result = estimation
                .estimate(&partitioning)
                .map_err(std::convert::Into::into);
            estimation_result
                .map(|result| data::Success::new(job_params, logical_resources, result))
        }
    }
}

fn serialize_error(err: error::Error) -> String {
    serde_json::to_string(&data::Failure::new(err))
        .expect("serializing to json string should succeed")
}
