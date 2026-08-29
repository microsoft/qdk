// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compatibility consumer for simulators implementing the legacy [`Simulator`] trait.

use std::convert::Infallible;

use crate::{MeasurementResult, OutputRecord, Simulator};

use super::{
    AdaptiveCommand, AdaptiveExecution, AdaptiveExecutionError, AdaptiveResponse, MeasurementKind,
    MeasurementRequest, PreparedAdaptiveProgram, QuantumEvolutionRegion, RegionConsumer,
    UnitaryOperation, apply_unitary_immediately,
};

/// Borrowed preparation token used by the immediate compatibility strategy.
pub struct ImmediatePreparedRegion<'region> {
    operations: &'region [UnitaryOperation],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImmediateRegionReport {
    pub operation_count: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ImmediateExecutionReport;

/// Executes region operations immediately against a legacy simulator.
pub struct ImmediateSimulatorConsumer<'simulator, S> {
    simulator: &'simulator mut S,
}

impl<'simulator, S> ImmediateSimulatorConsumer<'simulator, S> {
    pub fn new(simulator: &'simulator mut S) -> Self {
        Self { simulator }
    }
}

impl<S: Simulator> ImmediateSimulatorConsumer<'_, S> {
    pub(super) fn measure(&mut self, request: MeasurementRequest) -> MeasurementResult {
        match request.kind {
            MeasurementKind::MeasureZ => {
                self.simulator.mz(request.qubit, request.result_id);
            }
            MeasurementKind::MeasureResetZ => {
                self.simulator.mresetz(request.qubit, request.result_id);
            }
        }
        self.simulator.measurements()[request.result_id]
    }
}

impl<S: Simulator> RegionConsumer for ImmediateSimulatorConsumer<'_, S> {
    type PreparedRegion<'region> = ImmediatePreparedRegion<'region>;
    type RegionReport = ImmediateRegionReport;
    type ExecutionReport = ImmediateExecutionReport;
    type Error = Infallible;

    fn prepare_region<'region>(
        &mut self,
        region: &'region QuantumEvolutionRegion,
    ) -> Result<Self::PreparedRegion<'region>, Self::Error> {
        Ok(ImmediatePreparedRegion {
            operations: region.operations(),
        })
    }

    fn execute_region(
        &mut self,
        region: Self::PreparedRegion<'_>,
    ) -> Result<Self::RegionReport, Self::Error> {
        for operation in region.operations {
            apply_unitary_immediately(self.simulator, *operation);
        }
        Ok(ImmediateRegionReport {
            operation_count: region.operations.len(),
        })
    }

    fn finish_execution(&mut self) -> Result<Self::ExecutionReport, Self::Error> {
        Ok(ImmediateExecutionReport)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Drives one prepared adaptive shot through an immediate simulator consumer.
pub fn run_prepared_shot<S: Simulator>(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    simulator: &mut S,
) -> Result<Vec<OutputRecord>, AdaptiveExecutionError> {
    let mut execution = AdaptiveExecution::new(prepared_program);
    let mut consumer = ImmediateSimulatorConsumer::new(simulator);
    let mut response = None;

    loop {
        let command = match execution.next_command(response.take()) {
            Ok(command) => command,
            Err(error) => {
                consumer
                    .close()
                    .expect("immediate consumer close is infallible");
                return Err(error);
            }
        };
        response = Some(match command {
            AdaptiveCommand::ExecuteRegion { region, .. } => {
                let prepared = consumer
                    .prepare_region(&region)
                    .expect("immediate region preparation is infallible");
                consumer
                    .execute_region(prepared)
                    .expect("immediate region execution is infallible");
                AdaptiveResponse::RegionComplete
            }
            AdaptiveCommand::Measure(request) => {
                AdaptiveResponse::Measurement(consumer.measure(request))
            }
            AdaptiveCommand::Complete(records) => {
                consumer
                    .finish_execution()
                    .expect("immediate consumer completion is infallible");
                consumer
                    .close()
                    .expect("immediate consumer close is infallible");
                return Ok(records);
            }
        });
    }
}