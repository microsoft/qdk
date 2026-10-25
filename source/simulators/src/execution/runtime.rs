// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The walk as a Runtime adapter: it runs a program's classical part itself
//! and sends the quantum part, in the region ISA, to the terminal it wraps.

use std::fmt;

use executor_protocol::{
    Batch, Capabilities, Close, Describe, ErrorKind, Executor, Fold, Immediate, Outcome, Rejected,
    Value, wait,
};

use crate::{MeasurementResult, OutputRecord};

use super::{
    AdaptiveCommand, AdaptiveExecution, AdaptiveExecutionError, AdaptiveResponse,
    PreparedAdaptiveProgram, RegionInstruction,
};

/// Runs prepared Adaptive programs on an executor of the region ISA.
///
/// A program's branches, loops and output records are classical: the walk
/// ([`AdaptiveExecution`]) runs them here, and sends the terminal only
/// reached regions, measurements and resets. A batch sent down ends at a
/// measurement, because the walk needs its result to go on, or at the end of
/// the program. The programs of one call run in order on the same shot, so
/// each starts from the state the one before left.
///
/// Its reply has one [`Outcome`] per output record, tagged with the record's
/// index among the records of the call.
pub struct RuntimeAdapter<E> {
    inner: E,
    report: Counts,
    closed: bool,
}

#[derive(Default)]
struct Counts {
    programs: u64,
    batches: u64,
}

impl<E> RuntimeAdapter<E> {
    /// The adapter over `inner`.
    #[must_use]
    pub fn new(inner: E) -> Self {
        Self {
            inner,
            report: Counts::default(),
            closed: false,
        }
    }

    /// The executor it wraps.
    #[must_use]
    pub fn inner(&self) -> &E {
        &self.inner
    }

    /// The executor it wraps, with the state the programs so far left.
    #[must_use]
    pub fn into_inner(self) -> E {
        self.inner
    }
}

impl<E: Executor<Batch<RegionInstruction>>> RuntimeAdapter<E> {
    /// Runs `program` and appends its output records to `records`.
    ///
    /// # Errors
    ///
    /// A [`RuntimeError`]. `records` is then left as it was; the
    /// instructions the terminal ran before the error stay applied.
    pub fn run(
        &mut self,
        program: &PreparedAdaptiveProgram<u64>,
        records: &mut Vec<OutputRecord>,
    ) -> Result<(), RuntimeError> {
        if self.closed {
            return Err(RuntimeError::Rejected(Rejected::Closed));
        }
        let mut execution = AdaptiveExecution::new(program);
        let mut pending = Vec::new();
        let mut response = None;
        loop {
            let command = match execution.next_command(response.take()) {
                Ok(command) => command,
                Err(error) => {
                    // What the walk sent before its error runs first, so that
                    // the terminal's own failure, earlier in the program, is
                    // the one reported, as it would be with one call per
                    // command.
                    self.send(&mut pending)?;
                    return Err(RuntimeError::Control(error));
                }
            };
            response = Some(match command {
                AdaptiveCommand::ExecuteRegion { region_id, region } => {
                    pending.push(RegionInstruction::ExecuteRegion { region_id, region });
                    AdaptiveResponse::RegionComplete
                }
                AdaptiveCommand::Reset { qubit } => {
                    pending.push(RegionInstruction::Reset { qubit });
                    AdaptiveResponse::ResetComplete
                }
                AdaptiveCommand::Measure(request) => {
                    pending.push(RegionInstruction::Measure(request));
                    let outcomes = self.send(&mut pending)?;
                    let result_id = request.result_id;
                    let value = u32::try_from(result_id)
                        .ok()
                        .and_then(|tag| outcomes.iter().rev().find(|o| o.tag == tag))
                        .ok_or(RuntimeError::MissingOutcome { result_id })?
                        .value;
                    let result = MeasurementResult::try_from(value)
                        .map_err(|_| RuntimeError::MeasurementError { result_id })?;
                    AdaptiveResponse::Measurement(result)
                }
                AdaptiveCommand::Complete(output) => {
                    self.send(&mut pending)?;
                    records.extend(output);
                    self.report.programs += 1;
                    return Ok(());
                }
            });
        }
    }

    /// Sends the pending instructions as one batch, if there are any, and
    /// waits for its outcomes.
    fn send(&mut self, pending: &mut Vec<RegionInstruction>) -> Result<Vec<Outcome>, RuntimeError> {
        let mut outcomes = Vec::new();
        if pending.is_empty() {
            return Ok(outcomes);
        }
        let reply = self
            .inner
            .call(pending.as_slice())
            .map_err(RuntimeError::Rejected)?;
        pending.clear();
        self.report.batches += 1;
        wait(reply, &mut outcomes).map_err(RuntimeError::Failed)?;
        Ok(outcomes)
    }
}

impl<E: Executor<Batch<RegionInstruction>>> Executor<Batch<PreparedAdaptiveProgram<u64>>>
    for RuntimeAdapter<E>
{
    type Response = Immediate<Outcome>;

    /// Runs the programs in order, each to its end, before it returns.
    ///
    /// So `call` blocks: it takes each batch's reply with [`wait`], and the
    /// caller's thread is held until the terminal ends, however the caller
    /// then takes this reply. That is M1's assumption, not the protocol's:
    /// M1's terminals reply [`Immediate`], so `wait` never parks. The walk
    /// does need each measurement's result before it goes on (H₂ has 1,404
    /// measurements and 713 branches on them), but a shot is sequential:
    /// nothing in this call could run while the terminal works, so holding
    /// the thread loses nothing. A milestone with a terminal that replies
    /// `Pending` (a GPU stream, M4) would make `Response` a completion that
    /// drives the walk on each poll; callers, which already take a
    /// [`Completion`](executor_protocol::Completion), would not change.
    ///
    /// # Errors
    ///
    /// [`Rejected::Closed`] after [`Close::close`]. A rejection by the
    /// terminal before it ran anything of this call passes through, except
    /// that [`Rejected::Instruction`] then names the program, so that `at`
    /// indexes this call's batch. A failure after the terminal started is not
    /// a rejection: the reply fails, after the outcomes of the records before.
    fn call(
        &mut self,
        programs: &[PreparedAdaptiveProgram<u64>],
    ) -> Result<Self::Response, Rejected> {
        if self.closed {
            return Err(Rejected::Closed);
        }
        let mut records = Vec::new();
        let batches_before = self.report.batches;
        let programs_before = self.report.programs;
        for (at, program) in programs.iter().enumerate() {
            if let Err(error) = self.run(program, &mut records) {
                if self.report.batches == batches_before
                    && let RuntimeError::Rejected(rejected) = error
                {
                    // Programs before this one may have run to their end
                    // without a batch; a rejected call counts none of them.
                    self.report.programs = programs_before;
                    return Err(match rejected {
                        Rejected::Instruction {
                            at: instruction,
                            reason,
                        } => Rejected::Instruction {
                            at,
                            reason: format!("region instruction {instruction}: {reason}"),
                        },
                        rejected => rejected,
                    });
                }
                return Ok(Immediate::failed(
                    outcomes_of(&records),
                    format!("program {at}: {error}"),
                ));
            }
        }
        Ok(Immediate::done(outcomes_of(&records)))
    }
}

fn outcomes_of(records: &[OutputRecord]) -> Vec<Outcome> {
    (0u32..)
        .zip(records)
        .map(|(tag, record)| Outcome {
            tag,
            value: match record {
                OutputRecord::Result(result) => (*result).into(),
                OutputRecord::Bool(value) => (*value).into(),
                // The walk records only results; any other record has no
                // value in the protocol.
                OutputRecord::Int(_) | OutputRecord::Double(_) => Value::Error(ErrorKind::Other),
            },
        })
        .collect()
}

impl<E: Describe> Describe for RuntimeAdapter<E> {
    /// Those of the terminal: the walk adds no limit of its own.
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
}

impl<E: Close> Close for RuntimeAdapter<E> {
    type Report = RuntimeReport<E::Report>;

    fn close(&mut self) -> Self::Report {
        self.closed = true;
        let counts = std::mem::take(&mut self.report);
        RuntimeReport {
            programs: counts.programs,
            batches: counts.batches,
            inner: self.inner.close(),
        }
    }
}

/// What a [`RuntimeAdapter`] ran, with the report of the executor it wraps.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RuntimeReport<R> {
    /// The programs it ran to their end.
    pub programs: u64,
    /// The batches the terminal accepted.
    pub batches: u64,
    /// The report of the executor it wraps.
    pub inner: R,
}

impl<R: Fold> Fold for RuntimeReport<R> {
    fn fold(&mut self, other: Self) {
        self.programs += other.programs;
        self.batches += other.batches;
        self.inner.fold(other.inner);
    }
}

/// Why a program did not run to its end.
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeError {
    /// The terminal rejected a batch: nothing of that batch ran.
    Rejected(Rejected),
    /// The program's classical part failed.
    Control(AdaptiveExecutionError),
    /// The terminal's run of a batch failed.
    Failed(String),
    /// The terminal's reply has no outcome for a measurement.
    MissingOutcome { result_id: usize },
    /// The terminal reports a measurement as an error other than a lost
    /// qubit, which the walk cannot branch on.
    MeasurementError { result_id: usize },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(rejected) => {
                write!(formatter, "the terminal rejected a batch: {rejected}")
            }
            Self::Control(error) => write!(formatter, "control execution failed: {error}"),
            Self::Failed(reason) => write!(formatter, "the terminal failed: {reason}"),
            Self::MissingOutcome { result_id } => write!(
                formatter,
                "the terminal did not report the outcome of result {result_id}"
            ),
            Self::MeasurementError { result_id } => write!(
                formatter,
                "the terminal reported an error for result {result_id}"
            ),
        }
    }
}

impl std::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rejected(rejected) => Some(rejected),
            Self::Control(error) => Some(error),
            _ => None,
        }
    }
}
