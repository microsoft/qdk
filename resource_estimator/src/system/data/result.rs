// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::rc::Rc;

use crate::estimates::{ErrorBudget, LogicalPatch, Overhead, PhysicalResourceEstimationResult};
use crate::system::modeling::{Protocol, TFactory};

use super::LayoutReportData;
use super::{
    super::Error, FormattedPhysicalResourceCounts, JobParams, PhysicalResourceCounts,
    PhysicalResourceCountsBreakdown, Report,
};
use miette::Diagnostic;
use serde::{ser::SerializeMap, Serialize, Serializer};

#[derive(Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct Success<L: Serialize> {
    status: &'static str,
    job_params: JobParams,
    #[serde(skip_serializing_if = "Option::is_none")]
    physical_counts: Option<PhysicalResourceCounts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    physical_counts_formatted: Option<FormattedPhysicalResourceCounts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    logical_qubit: Option<LogicalPatch<Protocol>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tfactory: Option<TFactory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_budget: Option<ErrorBudget>,
    logical_counts: Rc<L>,
    report_data: Report,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    frontier_entries: Vec<FrontierEntry>,
}

impl<L: Overhead + LayoutReportData + Serialize> Success<L> {
    pub fn new(
        job_params: JobParams,
        result: PhysicalResourceEstimationResult<Protocol, TFactory, L>,
    ) -> Self {
        let counts = create_physical_resource_counts(&result);

        let formatted_counts: FormattedPhysicalResourceCounts =
            FormattedPhysicalResourceCounts::new(&result, &job_params);

        let report_data = Report::new(&job_params, &result, &formatted_counts);

        let logical_counts = result.layout_overhead().clone();
        let (logical_qubit, tfactory, error_budget) = result.take();

        Self {
            status: "success",
            job_params,
            physical_counts: Some(counts),
            physical_counts_formatted: Some(formatted_counts),
            logical_qubit: Some(logical_qubit),
            tfactory,
            error_budget: Some(error_budget),
            logical_counts,
            report_data,
            frontier_entries: Vec::new(),
        }
    }

    pub fn new_from_multiple(
        job_params: JobParams,
        mut results: Vec<PhysicalResourceEstimationResult<Protocol, TFactory, L>>,
    ) -> Self {
        let mut report_data: Option<Report> = None;

        let mut frontier_entries: Vec<FrontierEntry> = Vec::new();

        let logical_counts = results[0].layout_overhead().clone();

        // we will pick the shortest runtime result as the first result.
        results.sort_by_key(PhysicalResourceEstimationResult::runtime);
        for result in results {
            let (frontier_entry, report) =
                create_frontier_entry(&job_params, result, report_data.is_none());

            if report_data.is_none() {
                report_data = Some(report.expect("error should have report"));
            }

            frontier_entries.push(frontier_entry);
        }

        Self {
            status: "success",
            job_params,
            physical_counts: None,
            physical_counts_formatted: None,
            logical_qubit: None,
            tfactory: None,
            error_budget: None,
            logical_counts,
            report_data: report_data.expect("error should have report"), // Here we assume that at least a single solution was found.
            frontier_entries,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all(serialize = "camelCase"))]
pub struct FrontierEntry {
    pub logical_qubit: LogicalPatch<Protocol>,
    pub tfactory: Option<TFactory>,
    pub error_budget: ErrorBudget,
    pub physical_counts: PhysicalResourceCounts,
    pub physical_counts_formatted: FormattedPhysicalResourceCounts,
}

fn create_frontier_entry(
    job_params: &JobParams,
    result: PhysicalResourceEstimationResult<Protocol, TFactory, impl Overhead + LayoutReportData>,
    create_report: bool,
) -> (FrontierEntry, Option<Report>) {
    let physical_counts = create_physical_resource_counts(&result);

    let physical_counts_formatted: FormattedPhysicalResourceCounts =
        FormattedPhysicalResourceCounts::new(&result, job_params);

    let report_data = if create_report {
        Some(Report::new(job_params, &result, &physical_counts_formatted))
    } else {
        None
    };

    let (logical_qubit, tfactory, error_budget) = result.take();

    (
        FrontierEntry {
            logical_qubit,
            tfactory,
            error_budget,
            physical_counts,
            physical_counts_formatted,
        },
        report_data,
    )
}

fn create_physical_resource_counts(
    result: &PhysicalResourceEstimationResult<Protocol, TFactory, impl Overhead>,
) -> PhysicalResourceCounts {
    let breakdown = create_physical_resource_counts_breakdown(result);

    PhysicalResourceCounts {
        physical_qubits: result.physical_qubits(),
        runtime: result.runtime(),
        rqops: result.rqops(),
        breakdown,
    }
}

fn create_physical_resource_counts_breakdown(
    result: &PhysicalResourceEstimationResult<Protocol, TFactory, impl Overhead>,
) -> PhysicalResourceCountsBreakdown {
    let num_ts_per_rotation = result
        .layout_overhead()
        .num_magic_states_per_rotation(result.error_budget().rotations());
    PhysicalResourceCountsBreakdown {
        algorithmic_logical_qubits: result.layout_overhead().logical_qubits(),
        algorithmic_logical_depth: result
            .layout_overhead()
            .logical_depth(num_ts_per_rotation.unwrap_or_default()),
        logical_depth: result.num_cycles(),
        clock_frequency: result.logical_patch().logical_cycles_per_second(),
        num_tstates: result
            .layout_overhead()
            .num_magic_states(num_ts_per_rotation.unwrap_or_default()),
        num_tfactories: result.num_factories(),
        num_tfactory_runs: result.num_factory_runs(),
        physical_qubits_for_tfactories: result.physical_qubits_for_factories(),
        physical_qubits_for_algorithm: result.physical_qubits_for_algorithm(),
        required_logical_qubit_error_rate: result.required_logical_error_rate(),
        required_logical_tstate_error_rate: result.required_logical_magic_state_error_rate(),
        num_ts_per_rotation,
        clifford_error_rate: result
            .logical_patch()
            .physical_qubit()
            .clifford_error_rate(),
    }
}

pub struct Failure {
    error: Error,
    batch_index: Option<usize>,
}

impl Failure {
    #[must_use]
    pub fn new(error: Error) -> Self {
        Self {
            error,
            batch_index: None,
        }
    }
}

impl Serialize for Failure {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(4))?;

        map.serialize_entry(
            "code",
            &self
                .error
                .code()
                .expect("error should have code")
                .to_string(),
        )?;
        if let Some(batch_index) = self.batch_index {
            map.serialize_entry(
                "message",
                &format!("[batch index {}] {:?}", batch_index, self.error),
            )?;
        } else {
            map.serialize_entry("message", &self.error.to_string())?;
        }

        map.end()
    }
}

impl Serialize for LogicalPatch<Protocol> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("codeDistance", &self.code_parameter())?;
        map.serialize_entry("physicalQubits", &self.physical_qubits())?;
        map.serialize_entry("logicalCycleTime", &self.logical_cycle_time())?;
        map.serialize_entry("logicalErrorRate", &self.logical_error_rate())?;

        map.end()
    }
}

impl Serialize for ErrorBudget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("logical", &self.logical())?;
        map.serialize_entry("tstates", &self.magic_states())?;
        map.serialize_entry("rotations", &self.rotations())?;
        map.end()
    }
}
