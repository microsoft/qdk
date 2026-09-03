use std::collections::{BTreeMap, BTreeSet};

use qdk_simulators::{
    MeasurementResult, OutputRecord, QubitID,
    execution::{
        MeasuredQubit, MeasurementKind, MeasurementMetadataError, MeasurementRequest,
        PreparedAdaptiveProgram, QuantumEvolutionRegion, RegionConsumer, ShotExecutionError,
        drive_prepared_shot,
    },
};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub(crate) enum CuTensorNetMpsConsumerError {
    #[error(
        "cuTensorNet batch sampling requires exactly one quantum evolution region, found {actual}"
    )]
    UnsupportedRegionCount { actual: usize },

    #[error(
        "cuTensorNet batch sampling cannot execute a quantum evolution region after measuring qubit {qubit}"
    )]
    UnsupportedFeedforward { qubit: QubitID },

    #[error(
        "cuTensorNet batch sampling cannot replay a measurement of qubit {qubit} after MeasureResetZ"
    )]
    UnsupportedMeasurementAfterReset { qubit: QubitID },

    #[error("cuTensorNet batch sampling cannot resolve measurement metadata: {error}")]
    InvalidMeasurementMetadata { error: MeasurementMetadataError },

    #[error(
        "sample matrix has {actual} values; {shot_count} shots over {measured_qubit_count} measured qubits require {expected}"
    )]
    InvalidSampleCount {
        actual: usize,
        expected: usize,
        shot_count: usize,
        measured_qubit_count: usize,
    },

    #[error(
        "sample matrix size overflows for {shot_count} shots over {measured_qubit_count} measured qubits"
    )]
    SampleCountOverflow {
        shot_count: usize,
        measured_qubit_count: usize,
    },

    #[error("sample matrix contains {shot_count} shots, but shot {shot_index} was requested")]
    ShotIndexOutOfRange {
        shot_index: usize,
        shot_count: usize,
    },

    #[error("sample matrix has no column for measured qubit {qubit}")]
    MissingQubitColumn { qubit: QubitID },

    #[error("sampler output contains invalid value {value} at flat buffer index {index}")]
    InvalidSamplerOutput { index: usize, value: i64 },

    #[error("sample matrix contains invalid value {value} for shot {shot_index}, qubit {qubit}")]
    InvalidSampleValue {
        shot_index: usize,
        qubit: QubitID,
        value: u8,
    },
}

#[derive(Debug, Error)]
pub(crate) enum CuTensorNetMpsShotLoopError {
    #[error(transparent)]
    Consumer(#[from] CuTensorNetMpsConsumerError),

    #[error(transparent)]
    Shot(#[from] ShotExecutionError<CuTensorNetMpsConsumerError>),
}

#[allow(
    clippy::boxed_local,
    clippy::needless_pass_by_value,
    reason = "PreparedSampler transfers its owned Box<[i64]> buffer to the narrowing boundary"
)]
pub(crate) fn narrow_samples(
    samples: Box<[i64]>,
) -> Result<Box<[u8]>, CuTensorNetMpsConsumerError> {
    samples
        .iter()
        .copied()
        .enumerate()
        .map(|(index, value)| {
            let narrowed = u8::try_from(value)
                .map_err(|_| CuTensorNetMpsConsumerError::InvalidSamplerOutput { index, value })?;
            if matches!(narrowed, 0 | 1) {
                Ok(narrowed)
            } else {
                Err(CuTensorNetMpsConsumerError::InvalidSamplerOutput { index, value })
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Vec::into_boxed_slice)
}

#[derive(Debug)]
pub(crate) struct CuTensorNetSampleMatrix<'samples> {
    samples: &'samples [u8],
    column_by_qubit: BTreeMap<QubitID, usize>,
    shot_count: usize,
}

impl<'samples> CuTensorNetSampleMatrix<'samples> {
    fn columns(measured_qubits: &[MeasuredQubit]) -> BTreeMap<QubitID, usize> {
        let mut column_by_qubit = BTreeMap::new();
        for request in measured_qubits {
            let next_column = column_by_qubit.len();
            column_by_qubit.entry(request.qubit).or_insert(next_column);
        }
        column_by_qubit
    }

    pub(crate) fn sampled_qubits(measured_qubits: &[MeasuredQubit]) -> Box<[QubitID]> {
        let column_by_qubit = Self::columns(measured_qubits);
        let mut qubits = vec![0; column_by_qubit.len()];
        for (qubit, column) in column_by_qubit {
            qubits[column] = qubit;
        }
        qubits.into_boxed_slice()
    }

    pub(crate) fn new(
        measured_qubits: &[MeasuredQubit],
        shot_count: usize,
        samples: &'samples [u8],
    ) -> Result<Self, CuTensorNetMpsConsumerError> {
        let column_by_qubit = Self::columns(measured_qubits);
        let measured_qubit_count = column_by_qubit.len();
        let expected = shot_count.checked_mul(measured_qubit_count).ok_or(
            CuTensorNetMpsConsumerError::SampleCountOverflow {
                shot_count,
                measured_qubit_count,
            },
        )?;
        if samples.len() != expected {
            return Err(CuTensorNetMpsConsumerError::InvalidSampleCount {
                actual: samples.len(),
                expected,
                shot_count,
                measured_qubit_count,
            });
        }
        Ok(Self {
            samples,
            column_by_qubit,
            shot_count,
        })
    }

    fn sample(
        &self,
        shot_index: usize,
        qubit: QubitID,
    ) -> Result<MeasurementResult, CuTensorNetMpsConsumerError> {
        if shot_index >= self.shot_count {
            return Err(CuTensorNetMpsConsumerError::ShotIndexOutOfRange {
                shot_index,
                shot_count: self.shot_count,
            });
        }
        let column = self
            .column_by_qubit
            .get(&qubit)
            .copied()
            .ok_or(CuTensorNetMpsConsumerError::MissingQubitColumn { qubit })?;
        let measured_qubit_count = self.column_by_qubit.len();
        let value = self.samples[shot_index * measured_qubit_count + column];
        match value {
            0 => Ok(MeasurementResult::Zero),
            1 => Ok(MeasurementResult::One),
            value => Err(CuTensorNetMpsConsumerError::InvalidSampleValue {
                shot_index,
                qubit,
                value,
            }),
        }
    }
}

pub(crate) fn collect_sampled_shots(
    prepared_program: &PreparedAdaptiveProgram<u64>,
    shot_count: usize,
    samples: Box<[i64]>,
) -> Result<Vec<Vec<OutputRecord>>, CuTensorNetMpsShotLoopError> {
    let samples = narrow_samples(samples)?;
    let matrix = CuTensorNetSampleMatrix::new(
        prepared_program
            .measured_qubits()
            .map_err(|error| CuTensorNetMpsConsumerError::InvalidMeasurementMetadata { error })?,
        shot_count,
        &samples,
    )?;
    (0..shot_count)
        .map(|shot_index| {
            let mut consumer = CuTensorNetMpsConsumer::new(prepared_program, &matrix, shot_index)?;
            let output = drive_prepared_shot(prepared_program, &mut consumer)?;
            Ok(output.into_records())
        })
        .collect()
}

#[derive(Debug)]
pub(crate) struct CuTensorNetMpsConsumer<'matrix, 'samples> {
    samples: &'matrix CuTensorNetSampleMatrix<'samples>,
    shot_index: usize,
    last_measured_qubit: Option<QubitID>,
    reset_qubits: BTreeSet<QubitID>,
}

impl<'matrix, 'samples> CuTensorNetMpsConsumer<'matrix, 'samples> {
    pub(crate) fn new(
        prepared_program: &PreparedAdaptiveProgram<u64>,
        samples: &'matrix CuTensorNetSampleMatrix<'samples>,
        shot_index: usize,
    ) -> Result<Self, CuTensorNetMpsConsumerError> {
        let actual = prepared_program.regions().len();
        // Whole-state batch sampling is sound only for a Base-shaped single region.
        // The incremental-measurement iteration removes this guard by evolving each conditional state.
        if actual != 1 {
            return Err(CuTensorNetMpsConsumerError::UnsupportedRegionCount { actual });
        }
        if shot_index >= samples.shot_count {
            return Err(CuTensorNetMpsConsumerError::ShotIndexOutOfRange {
                shot_index,
                shot_count: samples.shot_count,
            });
        }
        Ok(Self {
            samples,
            shot_index,
            last_measured_qubit: None,
            reset_qubits: BTreeSet::new(),
        })
    }
}

impl RegionConsumer for CuTensorNetMpsConsumer<'_, '_> {
    type PreparedRegion<'region> = &'region QuantumEvolutionRegion;
    type RegionReport = ();
    type ExecutionReport = ();
    type Error = CuTensorNetMpsConsumerError;

    fn prepare_region<'region>(
        &mut self,
        region: &'region QuantumEvolutionRegion,
    ) -> Result<Self::PreparedRegion<'region>, Self::Error> {
        Ok(region)
    }

    fn execute_region(
        &mut self,
        _region: Self::PreparedRegion<'_>,
    ) -> Result<Self::RegionReport, Self::Error> {
        // This view can acknowledge state evolution performed before the shot loop, but not feedforward.
        // The incremental-measurement iteration removes this guard by executing reached regions here.
        if let Some(qubit) = self.last_measured_qubit {
            return Err(CuTensorNetMpsConsumerError::UnsupportedFeedforward { qubit });
        }
        Ok(())
    }

    fn measure(&mut self, request: MeasurementRequest) -> Result<MeasurementResult, Self::Error> {
        if self.reset_qubits.contains(&request.qubit) {
            return Err(
                CuTensorNetMpsConsumerError::UnsupportedMeasurementAfterReset {
                    qubit: request.qubit,
                },
            );
        }
        let result = self.samples.sample(self.shot_index, request.qubit)?;
        match request.kind {
            MeasurementKind::MeasureZ => {}
            MeasurementKind::MeasureResetZ => {
                self.reset_qubits.insert(request.qubit);
            }
        }
        self.last_measured_qubit = Some(request.qubit);
        Ok(result)
    }

    fn finish_execution(&mut self) -> Result<Self::ExecutionReport, Self::Error> {
        Ok(())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CuTensorNetMpsConsumer, CuTensorNetMpsConsumerError, CuTensorNetSampleMatrix,
        narrow_samples,
    };
    use qdk_simulators::{
        MeasurementResult, OutputRecord,
        bytecode::{AdaptiveProgram, Block, Instruction, Op},
        execution::{
            MeasurementKind, MeasurementRequest, PreparedAdaptiveProgram, RegionConsumer,
            ShotExecutionError, drive_prepared_shot,
        },
    };

    const IMMEDIATE_SRC0: u64 = 1 << 16;
    const IMMEDIATE_AUX1: u64 = 1 << 20;
    const IMMEDIATE_AUX2: u64 = 1 << 21;
    const OP_QUANTUM_GATE: u64 = 0x10;
    const OP_MEASURE: u64 = 0x11;
    const OP_RECORD_OUTPUT: u64 = 0x14;
    const OP_RET: u64 = 0x02;
    const OPID_H: u64 = 5;
    const OPID_MZ: u64 = 21;
    const OPID_MRESETZ: u64 = 22;

    fn operation(operation_id: u64) -> Op<u64> {
        Op {
            op_id: operation_id,
            q1: 0,
            q2: 0,
            q3: 0,
            angle: 0,
        }
    }

    fn program(
        instructions: Vec<Instruction<u64>>,
        quantum_ops: Vec<Op<u64>>,
        result_count: u32,
    ) -> PreparedAdaptiveProgram<u64> {
        PreparedAdaptiveProgram::new(AdaptiveProgram {
            num_qubits: 2,
            num_results: result_count,
            num_registers: 0,
            entry_block: 0,
            block_table: vec![Block {
                instr_offset: 0,
                instr_count: instructions.len() as u64,
            }],
            instructions,
            function_table: Vec::new(),
            phi_entries: Vec::new(),
            switch_cases: Vec::new(),
            call_args: Vec::new(),
            constant_data: Vec::new(),
            quantum_ops,
        })
        .expect("test adaptive program should prepare")
    }

    fn gate(target: u64) -> Instruction<u64> {
        Instruction {
            opcode: OP_QUANTUM_GATE | IMMEDIATE_SRC0 | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
            aux0: 0,
            aux1: target,
            ..Instruction::default()
        }
    }

    fn measurement(operation_index: u64, qubit: u64, result_id: u64) -> Instruction<u64> {
        Instruction {
            opcode: OP_MEASURE | IMMEDIATE_AUX1 | IMMEDIATE_AUX2,
            aux0: operation_index,
            aux1: qubit,
            aux2: result_id,
            ..Instruction::default()
        }
    }

    fn measure(qubit: u64, result_id: u64) -> Instruction<u64> {
        measurement(1, qubit, result_id)
    }

    fn record(result_id: u64) -> Instruction<u64> {
        Instruction {
            opcode: OP_RECORD_OUTPUT | IMMEDIATE_SRC0,
            src0: result_id,
            ..Instruction::default()
        }
    }

    fn ret() -> Instruction<u64> {
        Instruction {
            opcode: OP_RET,
            ..Instruction::default()
        }
    }

    #[test]
    fn sample_backed_consumer_maps_qubits_and_leaves_output_to_control() {
        let prepared = program(
            vec![
                gate(0),
                measure(1, 0),
                measure(0, 1),
                record(1),
                record(0),
                ret(),
            ],
            vec![operation(OPID_H), operation(OPID_MZ)],
            2,
        );
        let matrix = CuTensorNetSampleMatrix::new(
            prepared
                .measured_qubits()
                .expect("measurement operands should be immediate"),
            2,
            &[0, 1, 1, 0],
        )
        .expect("sample matrix shape should match prepared measurements");
        let mut consumer = CuTensorNetMpsConsumer::new(&prepared, &matrix, 1)
            .expect("single-region shot should be supported");

        let output = drive_prepared_shot(&prepared, &mut consumer)
            .expect("sample-backed shot should execute");

        assert_eq!(
            output.records(),
            [
                OutputRecord::Result(MeasurementResult::Zero),
                OutputRecord::Result(MeasurementResult::One),
            ]
        );
        assert_eq!(output.region_reports(), [()]);
        assert_eq!(output.execution_report(), &());

        consumer.close().expect("repeated close should be harmless");
        consumer.close().expect("close should remain idempotent");
        assert_eq!(
            consumer.measure(MeasurementRequest {
                kind: MeasurementKind::MeasureZ,
                qubit: 1,
                result_id: 0,
            }),
            Ok(MeasurementResult::One)
        );
    }

    #[test]
    fn sample_narrowing_rejects_negative_values_with_the_flat_index() {
        assert_eq!(
            narrow_samples(vec![0, 1, -1].into_boxed_slice()),
            Err(CuTensorNetMpsConsumerError::InvalidSamplerOutput {
                index: 2,
                value: -1,
            })
        );
    }

    #[test]
    fn sample_narrowing_rejects_values_above_one_without_truncation() {
        assert_eq!(
            narrow_samples(vec![1, 256].into_boxed_slice()),
            Err(CuTensorNetMpsConsumerError::InvalidSamplerOutput {
                index: 1,
                value: 256,
            })
        );
    }

    #[test]
    fn consumer_rejects_programs_without_exactly_one_region() {
        let prepared = program(
            vec![gate(0), record(0), gate(1), ret()],
            vec![operation(OPID_H)],
            1,
        );
        let matrix = CuTensorNetSampleMatrix::new(
            prepared
                .measured_qubits()
                .expect("measurement metadata should be available"),
            1,
            &[],
        )
        .expect("empty measurement matrix should be valid");

        assert_eq!(
            CuTensorNetMpsConsumer::new(&prepared, &matrix, 0)
                .expect_err("multiple regions should be rejected"),
            CuTensorNetMpsConsumerError::UnsupportedRegionCount { actual: 2 }
        );
    }

    #[test]
    fn consumer_rejects_feedforward_after_precomputed_measurement() {
        let prepared = program(
            vec![measure(0, 0), gate(1), ret()],
            vec![operation(OPID_H), operation(OPID_MZ)],
            1,
        );
        let matrix = CuTensorNetSampleMatrix::new(
            prepared
                .measured_qubits()
                .expect("measurement operands should be immediate"),
            1,
            &[1],
        )
        .expect("sample matrix shape should match prepared measurements");
        let mut consumer = CuTensorNetMpsConsumer::new(&prepared, &matrix, 0)
            .expect("program has exactly one region");

        assert_eq!(
            drive_prepared_shot(&prepared, &mut consumer),
            Err(ShotExecutionError::Consumer(
                CuTensorNetMpsConsumerError::UnsupportedFeedforward { qubit: 0 }
            ))
        );
    }

    #[test]
    fn consumer_rejects_measurement_after_measure_reset_z() {
        let prepared = program(
            vec![gate(0), measurement(2, 0, 0), measure(0, 1), ret()],
            vec![
                operation(OPID_H),
                operation(OPID_MZ),
                operation(OPID_MRESETZ),
            ],
            2,
        );
        let matrix = CuTensorNetSampleMatrix::new(
            prepared
                .measured_qubits()
                .expect("measurement operands should be immediate"),
            1,
            &[1],
        )
        .expect("repeated measurements share one sample column");
        let mut consumer = CuTensorNetMpsConsumer::new(&prepared, &matrix, 0)
            .expect("program has exactly one region");

        assert_eq!(
            drive_prepared_shot(&prepared, &mut consumer),
            Err(ShotExecutionError::Consumer(
                CuTensorNetMpsConsumerError::UnsupportedMeasurementAfterReset { qubit: 0 }
            ))
        );
    }
}
