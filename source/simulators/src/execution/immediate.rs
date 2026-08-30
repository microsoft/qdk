// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Compatibility consumer for simulators implementing the legacy [`Simulator`] trait.

use std::{convert::Infallible, fmt};

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

#[derive(Debug, PartialEq)]
pub struct ShotExecutionOutput<RegionReport, ExecutionReport> {
    records: Vec<OutputRecord>,
    region_reports: Vec<RegionReport>,
    execution_report: ExecutionReport,
}

impl<RegionReport, ExecutionReport> ShotExecutionOutput<RegionReport, ExecutionReport> {
    #[must_use]
    pub fn records(&self) -> &[OutputRecord] {
        &self.records
    }

    #[must_use]
    pub fn region_reports(&self) -> &[RegionReport] {
        &self.region_reports
    }

    #[must_use]
    pub fn execution_report(&self) -> &ExecutionReport {
        &self.execution_report
    }

    #[must_use]
    pub fn into_records(self) -> Vec<OutputRecord> {
        self.records
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum ShotExecutionError<ConsumerError> {
    Control(AdaptiveExecutionError),
    Consumer(ConsumerError),
    Close(ConsumerError),
    ControlAndClose {
        control: AdaptiveExecutionError,
        close: ConsumerError,
    },
    ConsumerAndClose {
        consumer: ConsumerError,
        close: ConsumerError,
    },
}

impl<ConsumerError: fmt::Display> fmt::Display for ShotExecutionError<ConsumerError> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Control(error) => write!(formatter, "control execution failed: {error}"),
            Self::Consumer(error) => write!(formatter, "consumer execution failed: {error}"),
            Self::Close(error) => write!(formatter, "consumer close failed: {error}"),
            Self::ControlAndClose { control, close } => write!(
                formatter,
                "control execution failed: {control}; consumer close also failed: {close}"
            ),
            Self::ConsumerAndClose { consumer, close } => write!(
                formatter,
                "consumer execution failed: {consumer}; consumer close also failed: {close}"
            ),
        }
    }
}

impl<ConsumerError> std::error::Error for ShotExecutionError<ConsumerError> where
    ConsumerError: std::error::Error + 'static
{
}

pub type ShotExecutionResult<C> = Result<
    ShotExecutionOutput<
        <C as RegionConsumer>::RegionReport,
        <C as RegionConsumer>::ExecutionReport,
    >,
    ShotExecutionError<<C as RegionConsumer>::Error>,
>;

/// Executes region operations immediately against a legacy simulator.
pub struct ImmediateSimulatorConsumer<'simulator, S> {
    simulator: &'simulator mut S,
}

impl<'simulator, S> ImmediateSimulatorConsumer<'simulator, S> {
    pub fn new(simulator: &'simulator mut S) -> Self {
        Self { simulator }
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

    fn measure(&mut self, request: MeasurementRequest) -> Result<MeasurementResult, Self::Error> {
        match request.kind {
            MeasurementKind::MeasureZ => {
                self.simulator.mz(request.qubit, request.result_id);
            }
            MeasurementKind::MeasureResetZ => {
                self.simulator.mresetz(request.qubit, request.result_id);
            }
        }
        Ok(self.simulator.measurements()[request.result_id])
    }

    fn finish_execution(&mut self) -> Result<Self::ExecutionReport, Self::Error> {
        Ok(ImmediateExecutionReport)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Drives one prepared adaptive shot through a region consumer.
pub fn drive_prepared_shot<C: RegionConsumer>(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    consumer: &mut C,
) -> ShotExecutionResult<C> {
    let mut execution = AdaptiveExecution::new(prepared_program);
    let mut region_reports = Vec::new();
    let mut response = None;

    loop {
        let command = match execution.next_command(response.take()) {
            Ok(command) => command,
            Err(error) => {
                return Err(match consumer.close() {
                    Ok(()) => ShotExecutionError::Control(error),
                    Err(close) => ShotExecutionError::ControlAndClose {
                        control: error,
                        close,
                    },
                });
            }
        };
        response = Some(match command {
            AdaptiveCommand::ExecuteRegion { region, .. } => {
                let prepared = match consumer.prepare_region(&region) {
                    Ok(prepared) => prepared,
                    Err(error) => return Err(close_after_consumer_error(consumer, error)),
                };
                let report = match consumer.execute_region(prepared) {
                    Ok(report) => report,
                    Err(error) => return Err(close_after_consumer_error(consumer, error)),
                };
                region_reports.push(report);
                AdaptiveResponse::RegionComplete
            }
            AdaptiveCommand::Measure(request) => match consumer.measure(request) {
                Ok(result) => AdaptiveResponse::Measurement(result),
                Err(error) => return Err(close_after_consumer_error(consumer, error)),
            },
            AdaptiveCommand::Complete(records) => {
                let execution_report = match consumer.finish_execution() {
                    Ok(report) => report,
                    Err(error) => return Err(close_after_consumer_error(consumer, error)),
                };
                consumer.close().map_err(ShotExecutionError::Close)?;
                return Ok(ShotExecutionOutput {
                    records,
                    region_reports,
                    execution_report,
                });
            }
        });
    }
}

fn close_after_consumer_error<C: RegionConsumer>(
    consumer: &mut C,
    error: C::Error,
) -> ShotExecutionError<C::Error> {
    match consumer.close() {
        Ok(()) => ShotExecutionError::Consumer(error),
        Err(close) => ShotExecutionError::ConsumerAndClose {
            consumer: error,
            close,
        },
    }
}

/// Drives one prepared adaptive shot through an immediate simulator consumer.
pub fn run_prepared_shot<S: Simulator>(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    simulator: &mut S,
) -> Result<Vec<OutputRecord>, AdaptiveExecutionError> {
    let mut consumer = ImmediateSimulatorConsumer::new(simulator);
    match drive_prepared_shot(prepared_program, &mut consumer) {
        Ok(output) => Ok(output.into_records()),
        Err(ShotExecutionError::Control(error)) => Err(error),
        Err(ShotExecutionError::Consumer(error) | ShotExecutionError::Close(error)) => {
            match error {}
        }
        Err(
            ShotExecutionError::ControlAndClose { close, .. }
            | ShotExecutionError::ConsumerAndClose { close, .. },
        ) => match close {},
    }
}
