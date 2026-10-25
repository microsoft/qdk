// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A Base program for the batch strategy: M0's preflight, which checks it on
//! the host before any state is prepared, and M0's replay, which runs each
//! shot's sampled row through the program's walk to its output records.
//!
//! Both drive the walk through [`RuntimeAdapter`] over [`WalkTerminal`], an
//! executor of the region ISA that holds no state: in the preflight it
//! answers every measurement Zero, as M0's `CircuitPreparationConsumer`
//! did; in the replay it answers each measurement from its qubit's column
//! of the shot's row, as M0's `CuTensorNetMpsConsumer` did. So the output
//! records come from the execution core, not from this crate.

use executor_protocol::{Batch, Executor, Immediate, Outcome, Rejected, Value};
use qdk_simulators::{
    OutputRecord, QubitID,
    execution::{
        MeasurementKind, MeasurementMetadataError, MeasurementRequest, PreparedAdaptiveProgram,
        RegionInstruction, RuntimeAdapter, UnitaryOperation,
    },
};

use crate::lowering;

/// Why a program cannot run on the batch strategy, found before any state is
/// prepared. The messages are M0's ④'s, with "cuTensorNet" replaced by
/// "MPS": the engine may be any [`MpsEngine`](tensornet::MpsEngine).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ProgramError {
    /// Not exactly one quantum evolution region: the batch strategy samples
    /// one state, which one region prepares.
    #[error("MPS batch sampling requires exactly one quantum evolution region, found {found}")]
    RegionCount {
        /// The program's regions.
        found: usize,
    },
    /// A region after a measurement: what it applies may depend on the
    /// measurement, which the batch strategy samples only at the end.
    #[error(
        "MPS batch sampling cannot execute a quantum evolution region after measuring qubit {qubit}"
    )]
    Feedforward {
        /// The qubit measured last before the region.
        qubit: QubitID,
    },
    /// A measurement of a qubit that a `MeasureResetZ` reset: its value is not
    /// the sampled column's.
    #[error("MPS batch sampling cannot replay a measurement of qubit {qubit} after MeasureResetZ")]
    MeasurementAfterReset {
        /// The qubit.
        qubit: QubitID,
    },
    /// A reset, which the batch strategy does not offer.
    #[error("reset is not supported by MPS batch sampling (qubit {qubit})")]
    Reset {
        /// The qubit reset.
        qubit: QubitID,
    },
    /// The program's measurements cannot be listed statically, so the
    /// sampled qubits are not known.
    #[error("MPS batch sampling cannot resolve measurement metadata: {0}")]
    Metadata(MeasurementMetadataError),
    /// An operation of the region that cannot be applied (a qubit beyond the
    /// program's, two equal qubits, an angle that is not finite).
    #[error("region {region}: {reason}")]
    Operation {
        /// The region's id.
        region: u32,
        /// Why, in M0's `Circuit`'s words.
        reason: String,
    },
    /// A measurement of a qubit beyond the program's.
    #[error("a measurement of qubit {qubit}, but the program has {qubits} qubits")]
    Qubit {
        /// The qubit measured.
        qubit: QubitID,
        /// The program's qubits.
        qubits: usize,
    },
    /// A replayed measurement of a qubit that the program's static
    /// measurements do not list, so no column was sampled for it. Not
    /// reachable when the program's metadata lists every measurement, as
    /// `PreparedAdaptiveProgram` does.
    #[error("the sample has no column for measured qubit {qubit}")]
    MissingColumn {
        /// The qubit.
        qubit: QubitID,
    },
    /// The walk itself failed (its control flow, or a result it was not
    /// given).
    #[error("{0}")]
    Walk(String),
}

/// What the preflight found: the program's qubits, each execution of its
/// region as the instruction that applies it, and the sampled qubits, one
/// column each.
#[derive(Clone, Debug)]
pub(crate) struct BasePlan {
    pub(crate) qubits: usize,
    /// Every execution of the one region the walk reached, in order: none
    /// when it reaches none, more than one when a classical loop runs it
    /// again (M0 appended the region's gates each time).
    pub(crate) regions: Vec<RegionInstruction>,
    /// The distinct measured qubits, in the order of the program's
    /// measurements (M0's `CuTensorNetSampleMatrix::sampled_qubits`).
    pub(crate) columns: Vec<QubitID>,
}

impl BasePlan {
    /// The batch the batch strategy runs: each execution of the region,
    /// then one `MeasureZ` per column, tagged with the column. These are the
    /// sites, in order, that M0 sampled, so the same seed draws the same
    /// values.
    pub(crate) fn batch(&self) -> Vec<RegionInstruction> {
        self.regions
            .iter()
            .cloned()
            .chain(self.columns.iter().enumerate().map(|(column, &qubit)| {
                RegionInstruction::Measure(MeasurementRequest {
                    kind: MeasurementKind::MeasureZ,
                    qubit,
                    result_id: column,
                })
            }))
            .collect()
    }

    /// The operations of every execution of the region, in order: what ψ
    /// is, before the measurements.
    pub(crate) fn operations(&self) -> impl Iterator<Item = UnitaryOperation> + '_ {
        self.regions
            .iter()
            .flat_map(|instruction| match instruction {
                RegionInstruction::ExecuteRegion { region, .. } => region.operations(),
                RegionInstruction::Measure(_) | RegionInstruction::Reset { .. } => &[],
            })
            .copied()
    }
}

/// M0's preflight: more than one region, then the walk (feedforward, reset,
/// a measurement after `MeasureResetZ`, an operation that cannot be
/// applied), then no region, then the measurement metadata.
pub(crate) fn plan(program: &PreparedAdaptiveProgram<u64>) -> Result<BasePlan, ProgramError> {
    let regions = program.regions().len();
    if regions > 1 {
        return Err(ProgramError::RegionCount { found: regions });
    }
    let qubits = usize::try_from(program.program().num_qubits).unwrap_or(usize::MAX);
    let terminal = walk(program, WalkTerminal::new(qubits, None))?;
    if regions != 1 {
        return Err(ProgramError::RegionCount { found: regions });
    }
    let measured = program.measured_qubits().map_err(ProgramError::Metadata)?;
    let mut columns = Vec::new();
    for measured in measured {
        if !columns.contains(&measured.qubit) {
            columns.push(measured.qubit);
        }
    }
    Ok(BasePlan {
        qubits,
        regions: terminal.regions,
        columns,
    })
}

/// The output records of one shot whose sampled values are `row`, one per
/// column of `plan`.
pub(crate) fn replay(
    program: &PreparedAdaptiveProgram<u64>,
    plan: &BasePlan,
    row: &[bool],
) -> Result<Vec<OutputRecord>, ProgramError> {
    let mut records = Vec::new();
    let mut adapter =
        RuntimeAdapter::new(WalkTerminal::new(plan.qubits, Some((&plan.columns, row))));
    match adapter.run(program, &mut records) {
        Ok(()) => Ok(records),
        Err(error) => Err(adapter
            .into_inner()
            .fault
            .unwrap_or_else(|| ProgramError::Walk(error.to_string()))),
    }
}

fn walk<'a>(
    program: &PreparedAdaptiveProgram<u64>,
    terminal: WalkTerminal<'a>,
) -> Result<WalkTerminal<'a>, ProgramError> {
    let mut adapter = RuntimeAdapter::new(terminal);
    let result = adapter.run(program, &mut Vec::new());
    let mut terminal = adapter.into_inner();
    match (result, terminal.fault.take()) {
        (_, Some(fault)) => Err(fault),
        (Ok(()), None) => Ok(terminal),
        (Err(error), None) => Err(ProgramError::Walk(error.to_string())),
    }
}

/// An executor of the region ISA without a state, for the walk: see the
/// module's documentation. A fault fails the batch, and is kept for the
/// caller, typed.
struct WalkTerminal<'a> {
    qubits: usize,
    /// The columns and the shot's values; `None` answers Zero.
    row: Option<(&'a [QubitID], &'a [bool])>,
    last_measured: Option<QubitID>,
    reset_by_measurement: Vec<QubitID>,
    regions: Vec<RegionInstruction>,
    fault: Option<ProgramError>,
}

impl<'a> WalkTerminal<'a> {
    fn new(qubits: usize, row: Option<(&'a [QubitID], &'a [bool])>) -> Self {
        Self {
            qubits,
            row,
            last_measured: None,
            reset_by_measurement: Vec::new(),
            regions: Vec::new(),
            fault: None,
        }
    }

    fn step(
        &mut self,
        instruction: &RegionInstruction,
        outcomes: &mut Vec<Outcome>,
    ) -> Result<(), ProgramError> {
        match instruction {
            RegionInstruction::ExecuteRegion { region_id, region } => {
                if let Some(qubit) = self.last_measured {
                    return Err(ProgramError::Feedforward { qubit });
                }
                if let Some(reason) = lowering::check_region(region, self.qubits) {
                    return Err(ProgramError::Operation {
                        region: region_id.value(),
                        reason,
                    });
                }
                self.regions.push(instruction.clone());
                Ok(())
            }
            RegionInstruction::Measure(request) => {
                let qubit = request.qubit;
                if qubit >= self.qubits {
                    return Err(ProgramError::Qubit {
                        qubit,
                        qubits: self.qubits,
                    });
                }
                if self.reset_by_measurement.contains(&qubit) {
                    return Err(ProgramError::MeasurementAfterReset { qubit });
                }
                if request.kind == MeasurementKind::MeasureResetZ {
                    self.reset_by_measurement.push(qubit);
                }
                self.last_measured = Some(qubit);
                let one = match self.row {
                    None => false,
                    Some((columns, values)) => columns
                        .iter()
                        .position(|&column| column == qubit)
                        .and_then(|column| values.get(column).copied())
                        .ok_or(ProgramError::MissingColumn { qubit })?,
                };
                // A result beyond a tag is left out: the walk then reports
                // the missing outcome.
                if let Ok(tag) = u32::try_from(request.result_id) {
                    outcomes.push(Outcome {
                        tag,
                        value: Value::from(one),
                    });
                }
                Ok(())
            }
            RegionInstruction::Reset { qubit } => Err(ProgramError::Reset { qubit: *qubit }),
        }
    }
}

impl Executor<Batch<RegionInstruction>> for WalkTerminal<'_> {
    type Response = Immediate<Outcome>;

    fn call(&mut self, batch: &[RegionInstruction]) -> Result<Self::Response, Rejected> {
        let mut outcomes = Vec::new();
        for instruction in batch {
            if let Err(fault) = self.step(instruction, &mut outcomes) {
                let reason = fault.to_string();
                self.fault = Some(fault);
                return Ok(Immediate::failed(outcomes, reason));
            }
        }
        Ok(Immediate::done(outcomes))
    }
}
