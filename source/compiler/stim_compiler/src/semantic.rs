// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::parser;
use crate::parser::{ArgValue, PauliTarget, args_span};
use miette::Diagnostic;
use parser::Pauli;
use qsc_data_structures::{
    display::{write_list_field, writeln_field, writeln_header, writeln_header_with_span},
    span::Span,
};
use std::{
    f64::consts::PI,
    fmt::{self, Display, Formatter},
    slice::Chunks,
};
use thiserror::Error;

pub type StimQubitId = u32;
pub type Radians = f64;
pub type Probability = f64;

const MAX_COORDINATES: usize = 16;

#[derive(Clone, Copy, Debug)]
pub struct MeasurementRecord {
    pub offset: u32,
    pub span: Span,
}

#[derive(Debug)]
pub struct NegatableMeasurementRecord {
    pub record: MeasurementRecord,
    pub negated: bool,
}

#[derive(Debug)]
pub struct Circuit {
    pub span: Span,
    pub items: Vec<Item>,
}

#[derive(Debug)]
pub enum Item {
    Block(Block),
    Instruction(Instruction),
}

#[derive(Debug)]
pub enum Block {
    RepeatBlock { count: u32, body: Vec<Item> },
    SelectBlock { body: Vec<Item> },
}

#[derive(Debug)]
pub struct Instruction {
    pub span: Span,
    pub kind: InstructionKind,
}

#[derive(Debug)]
pub enum InstructionKind {
    Reset {
        qubit: StimQubitId,
        basis: Pauli,
    },
    SingleQubitGate {
        qubit: StimQubitId,
        gate: SingleQubitGateKind,
    },
    TwoQubitGate {
        q0: StimQubitId,
        q1: StimQubitId,
        gate: TwoQubitGateKind,
    },
    ThreeQubitGate {
        q0: StimQubitId,
        q1: StimQubitId,
        q2: StimQubitId,
        gate: ThreeQubitGateKind,
    },
    PauliProductGate {
        product: PauliProduct,
        gate: PauliProductGateKind,
    },
    ClassicallyControlledPauli {
        control: MeasurementRecord,
        target: StimQubitId,
        pauli: Pauli,
    },
    Noise(Noise),
    SingleQubitMeasurement {
        reset: bool,
        observable: Pauli,
        readout_noise: Probability,
        negated: bool,
        qubit: StimQubitId,
    },
    TwoQubitMeasurement {
        readout_noise: Probability,
        observable: PauliPair,
        negated: bool,
        q0: StimQubitId,
        q1: StimQubitId,
    },
    PauliProductMeasurement {
        readout_noise: Probability,
        product: PauliProduct,
    },
    PeekLoss {
        readout_noise: Probability,
        qubit: StimQubitId,
    },
    Require {
        records: Vec<NegatableMeasurementRecord>,
    },
    NotLeaked {
        records: Vec<MeasurementRecord>,
    },
    Annotation(Annotation),
    SingleQubitRotation {
        axis: Pauli,
        angle: Radians,
        qubit: StimQubitId,
    },
    TwoQubitRotation {
        axis: PauliPair,
        angle: Radians,
        q0: StimQubitId,
        q1: StimQubitId,
    },
    U3 {
        theta: Radians,
        phi: Radians,
        lambda: Radians,
        qubit: StimQubitId,
    },
    PauliProductRotation {
        angle: Radians,
        product: PauliProduct,
    },
}

impl Display for Circuit {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln_header_with_span(f, "Circuit", self.span)?;
        write_list_field(f, "items", &self.items)
    }
}

impl Display for Item {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Block(block) => write!(f, "{block}"),
            Self::Instruction(instruction) => write!(f, "{instruction}"),
        }
    }
}

impl Display for Block {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::RepeatBlock { count, body } => {
                writeln_header(f, "RepeatBlock")?;
                writeln_field(f, "count", count)?;
                write_list_field(f, "body", body)
            }
            Self::SelectBlock { body } => {
                writeln_header(f, "SelectBlock")?;
                write_list_field(f, "body", body)
            }
        }
    }
}

impl Display for Instruction {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} {:#?}", self.span, self.kind)
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(non_camel_case_types)]
pub enum SingleQubitGateKind {
    I,
    X,
    Y,
    Z,
    C_NXYZ,
    C_NZYX,
    C_XNYZ,
    C_XYNZ,
    C_XYZ,
    C_ZNYX,
    C_ZYNX,
    C_ZYX,
    H,
    H_NXY,
    H_NXZ,
    H_NYZ,
    H_XY,
    H_YZ,
    S,
    SQRT_X,
    SQRT_X_DAG,
    SQRT_Y,
    SQRT_Y_DAG,
    S_DAG,
    T,
    T_DAG,
}

impl SingleQubitGateKind {
    fn from_name(name: &str) -> Self {
        match name {
            "I" => Self::I,
            "X" => Self::X,
            "Y" => Self::Y,
            "Z" => Self::Z,
            "C_NXYZ" => Self::C_NXYZ,
            "C_NZYX" => Self::C_NZYX,
            "C_XNYZ" => Self::C_XNYZ,
            "C_XYNZ" => Self::C_XYNZ,
            "C_XYZ" => Self::C_XYZ,
            "C_ZNYX" => Self::C_ZNYX,
            "C_ZYNX" => Self::C_ZYNX,
            "C_ZYX" => Self::C_ZYX,
            "H" | "H_XZ" => Self::H,
            "H_NXY" => Self::H_NXY,
            "H_NXZ" => Self::H_NXZ,
            "H_NYZ" => Self::H_NYZ,
            "H_XY" => Self::H_XY,
            "H_YZ" => Self::H_YZ,
            "S" | "SQRT_Z" => Self::S,
            "SQRT_X" => Self::SQRT_X,
            "SQRT_X_DAG" => Self::SQRT_X_DAG,
            "SQRT_Y" => Self::SQRT_Y,
            "SQRT_Y_DAG" => Self::SQRT_Y_DAG,
            "S_DAG" | "SQRT_Z_DAG" => Self::S_DAG,
            "T" => Self::T,
            "T_DAG" => Self::T_DAG,
            _ => unreachable!("unknown single-qubit gate: {name}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
#[allow(non_camel_case_types)]
pub enum TwoQubitGateKind {
    CX,
    CXSWAP,
    CY,
    CZ,
    CZSWAP,
    II,
    ISWAP,
    ISWAP_DAG,
    SQRT_XX,
    SQRT_XX_DAG,
    SQRT_YY,
    SQRT_YY_DAG,
    SQRT_ZZ,
    SQRT_ZZ_DAG,
    SWAP,
    SWAPCX,
    XCX,
    XCY,
    XCZ,
    YCX,
    YCY,
    YCZ,
    CH,
}

impl TwoQubitGateKind {
    fn from_name(name: &str) -> Self {
        match name {
            "CX" | "CNOT" | "ZCX" => Self::CX,
            "CXSWAP" => Self::CXSWAP,
            "CY" | "ZCY" => Self::CY,
            "CZ" | "ZCZ" => Self::CZ,
            "CZSWAP" | "SWAPCZ" => Self::CZSWAP,
            "II" => Self::II,
            "ISWAP" => Self::ISWAP,
            "ISWAP_DAG" => Self::ISWAP_DAG,
            "SQRT_XX" => Self::SQRT_XX,
            "SQRT_XX_DAG" => Self::SQRT_XX_DAG,
            "SQRT_YY" => Self::SQRT_YY,
            "SQRT_YY_DAG" => Self::SQRT_YY_DAG,
            "SQRT_ZZ" => Self::SQRT_ZZ,
            "SQRT_ZZ_DAG" => Self::SQRT_ZZ_DAG,
            "SWAP" => Self::SWAP,
            "SWAPCX" => Self::SWAPCX,
            "XCX" => Self::XCX,
            "XCY" => Self::XCY,
            "XCZ" => Self::XCZ,
            "YCX" => Self::YCX,
            "YCY" => Self::YCY,
            "YCZ" => Self::YCZ,
            "CH" => Self::CH,
            _ => unreachable!("unknown two-qubit gate: {name}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ThreeQubitGateKind {
    CCZ,
    CCX,
}

impl ThreeQubitGateKind {
    fn from_name(name: &str) -> Self {
        match name {
            "CCZ" => Self::CCZ,
            "CCX" => Self::CCX,
            _ => unreachable!("unknown three-qubit gate: {name}"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PauliPair {
    XX,
    YY,
    ZZ,
}

#[derive(Debug)]
pub struct PauliProduct {
    pub factors: Vec<PauliFactor>,
    pub negated: bool,
}

#[derive(Debug)]
pub struct PauliFactor {
    pub pauli: Pauli,
    pub qubit: StimQubitId,
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug)]
pub enum PauliProductGateKind {
    S,
    S_DAG,
    T,
    T_DAG,
}

impl PauliProductGateKind {
    fn from_name(name: &str) -> Self {
        match name {
            "SPP" => Self::S,
            "SPP_DAG" => Self::S_DAG,
            "TPP" => Self::T,
            "TPP_DAG" => Self::T_DAG,
            _ => unreachable!("unknown Pauli product gate: {name}"),
        }
    }
}

#[derive(Debug)]
pub enum Noise {
    CorrelatedError {
        kind: CorrelatedErrorKind,
        probability: Probability,
        faults: Vec<Fault>,
    },
    Depolarize2 {
        probability: Probability,
        q0: StimQubitId,
        q1: StimQubitId,
    },
    HeraldedErase {
        probability: Probability,
        qubit: StimQubitId,
    },
    HeraldedPauliChannel1 {
        probabilities: [Probability; 4],
        qubit: StimQubitId,
    },
    PauliChannel1 {
        probabilities: [Probability; 3],
        qubit: StimQubitId,
    },
    PauliChannel2 {
        probabilities: [Probability; 15],
        q0: StimQubitId,
        q1: StimQubitId,
    },
    SingleQubitNoise {
        kind: SingleQubitNoiseKind,
        probability: Probability,
        qubit: StimQubitId,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum CorrelatedErrorKind {
    Initial,
    Else,
}

#[derive(Clone, Copy, Debug)]
pub enum FaultKind {
    X,
    Y,
    Z,
    Loss,
}

impl FaultKind {
    fn from_pauli(pauli: Pauli) -> Self {
        match pauli {
            Pauli::X => Self::X,
            Pauli::Y => Self::Y,
            Pauli::Z => Self::Z,
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
            Self::Loss => "L",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Fault {
    pub kind: FaultKind,
    pub qubit: StimQubitId,
}

#[derive(Clone, Copy, Debug)]
pub enum SingleQubitNoiseKind {
    Depolarize,
    Fault(FaultKind),
}
#[derive(Debug)]
pub enum Annotation {
    Detector {
        coordinates: Vec<f64>,
        records: Vec<MeasurementRecord>,
    },
    MeasurementPadding {
        readout_noise: Probability,
        value: bool,
    },
    ObservableInclude {
        logical_observable: u32,
        targets: Vec<ObservableTarget>,
    },
    QubitCoordinates {
        coordinates: Vec<f64>,
        qubit: StimQubitId,
    },
    ShiftCoordinates {
        offsets: Vec<f64>,
    },
    Tick,
}

#[derive(Debug)]
pub enum ObservableTarget {
    Record(MeasurementRecord),
    Pauli(PauliFactor),
}

#[derive(Clone, Debug, Error, Diagnostic)]
pub enum Error {
    #[error("unknown instruction: {name}")]
    #[diagnostic(code("Qdk.Stim.Semantic.UnknownInstruction"))]
    UnknownInstruction {
        name: String,
        #[label]
        span: Span,
    },
    #[error("{instruction} instruction must start a block")]
    #[diagnostic(code("Qdk.Stim.Semantic.InstructionWithoutBlock"))]
    InstructionWithoutBlock {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("unsupported argument in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Semantic.UnsupportedArgument"))]
    UnsupportedArgument {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("argument for {instruction} cannot be specified in radians")]
    #[diagnostic(code("Qdk.Stim.Semantic.UnexpectedRadians"))]
    UnexpectedRadians {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("missing argument in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Semantic.MissingArg"))]
    MissingArg {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("too few arguments for instruction {instruction}; expected {expected}, found {found}")]
    #[diagnostic(code("Qdk.Stim.Semantic.TooFewArgs"))]
    TooFewArgs {
        instruction: String,
        expected: usize,
        found: usize,
        #[label]
        span: Span,
    },
    #[error("too many arguments for instruction {instruction}; expected {expected}, found {found}")]
    #[diagnostic(code("Qdk.Stim.Semantic.TooManyArgs"))]
    TooManyArgs {
        instruction: String,
        expected: usize,
        found: usize,
        #[label]
        span: Span,
    },
    #[error("angle for {instruction} must be finite and representable in radians")]
    #[diagnostic(code("Qdk.Stim.Semantic.InvalidAngle"))]
    InvalidAngle {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("logical observable index must be a non-negative 32-bit integer")]
    #[diagnostic(code("Qdk.Stim.Semantic.InvalidLogicalObservableIndex"))]
    InvalidLogicalObservableIndex {
        #[label]
        span: Span,
    },
    #[error("probability for {instruction} must be between 0 and 1; found {probability}")]
    #[diagnostic(code("Qdk.Stim.Semantic.InvalidProbability"))]
    InvalidProbability {
        instruction: String,
        probability: f64,
        #[label]
        span: Span,
    },
    #[error("probabilities for {instruction} must sum to at most 1.0, but they sum to {total}")]
    #[diagnostic(code("Qdk.Stim.Semantic.InvalidProbabilitySum"))]
    InvalidProbabilitySum {
        instruction: String,
        total: f64,
        #[label]
        span: Span,
    },
    #[error("unsupported target in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Semantic.UnsupportedTarget"))]
    UnsupportedTarget {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("unsupported targets in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Semantic.UnsupportedTargets"))]
    UnsupportedTargets {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("missing target in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Semantic.MissingTarget"))]
    MissingTarget {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("target cannot be negated in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Semantic.NegatedTarget"))]
    NegatedTarget {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("instruction {instruction} requires an even number of targets")]
    #[diagnostic(code("Qdk.Stim.Semantic.OddTargetCount"))]
    OddTargetCount {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("instruction {instruction} requires a multiple of three targets")]
    #[diagnostic(code("Qdk.Stim.Semantic.TargetCountNotMultipleOfThree"))]
    TargetCountNotMultipleOfThree {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("qubit {qubit} is repeated in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Semantic.RepeatedQubit"))]
    RepeatedQubit {
        instruction: String,
        qubit: StimQubitId,
        #[label]
        span: Span,
    },
    #[error("measurement record target in an unsupported position in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Semantic.MisplacedMeasurementRecord"))]
    MisplacedMeasurementRecord {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error(
        "controlled instruction {instruction} requires a qubit target, but both targets are measurement records"
    )]
    #[diagnostic(code("Qdk.Stim.Semantic.BothTargetsAreMeasurementRecords"))]
    BothTargetsAreMeasurementRecords {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("measurement record is out of bounds")]
    #[diagnostic(code("Qdk.Stim.Semantic.MeasurementRecordOutOfBounds"))]
    MeasurementRecordOutOfBounds {
        #[label]
        span: Span,
    },
    #[error("the circuit exceeds the limit of 18,446,744,073,709,551,615 measurement records")]
    #[diagnostic(code("Qdk.Stim.Semantic.MeasurementRecordCounterOverflow"))]
    MeasurementRecordCounterOverflow,
    #[error("all measurement records referenced by {instruction} are out of scope")]
    #[diagnostic(code("Qdk.Stim.Semantic.AllMeasurementRecordsOutOfScope"))]
    AllMeasurementRecordsOutOfScope {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("{instruction} must appear inside a SELECT block")]
    #[diagnostic(code("Qdk.Stim.Semantic.InstructionOutsideSelectBlock"))]
    InstructionOutsideSelectBlock {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("a REPEAT count of zero is not supported")]
    #[diagnostic(code("Qdk.Stim.Semantic.ZeroRepeatCount"))]
    ZeroRepeatCount {
        #[label]
        span: Span,
    },
    #[error("Pauli product must be Hermitian")]
    #[diagnostic(code("Qdk.Stim.Semantic.AntiHermitianPauliProduct"))]
    AntiHermitianPauliProduct {
        #[label]
        span: Span,
    },
}

#[derive(Clone, Copy)]
enum AllowedRecPosition {
    First,
    Second,
    Either,
}

impl AllowedRecPosition {
    fn allows_first(self) -> bool {
        matches!(self, Self::First | Self::Either)
    }

    fn allows_second(self) -> bool {
        matches!(self, Self::Second | Self::Either)
    }
}

struct MeasurementRecordTracker {
    count: u64,
    select_starts: Vec<u64>,
}

impl MeasurementRecordTracker {
    fn new() -> Self {
        Self {
            count: 0,
            select_starts: Vec::new(),
        }
    }

    fn try_increase_record_count(&mut self, count: usize) -> Option<u64> {
        let updated_count = self.count.checked_add(count as u64)?;
        self.count = updated_count;
        Some(updated_count)
    }

    fn try_update_record_count_after_repeat(
        &mut self,
        count_before_repeat: u64,
        repeat_count: u32,
    ) -> Option<u64> {
        let records_per_iteration = self.count - count_before_repeat;
        let updated_count = records_per_iteration
            .checked_mul(u64::from(repeat_count))
            .and_then(|records| count_before_repeat.checked_add(records))?;
        self.count = updated_count;
        Some(updated_count)
    }

    fn is_offset_out_of_bounds(&self, offset: u32) -> bool {
        self.count < u64::from(offset)
    }

    fn enter_select_scope(&mut self) {
        self.select_starts.push(self.count);
    }

    fn exit_select_scope(&mut self) {
        self.select_starts.pop();
    }

    fn has_active_select_scope(&self) -> bool {
        !self.select_starts.is_empty()
    }

    fn is_offset_outside_current_select_scope(&self, offset: u32) -> bool {
        let Some(&select_start) = self.select_starts.last() else {
            return false;
        };
        let referenced_record = self.count - u64::from(offset); // already checked it's not out of bounds
        referenced_record < select_start
    }
}

struct Lowerer {
    errors: Vec<Error>,
    record_tracker: MeasurementRecordTracker,
}

impl Lowerer {
    fn new() -> Self {
        Self {
            errors: Vec::new(),
            record_tracker: MeasurementRecordTracker::new(),
        }
    }

    fn increase_record_count(&mut self, count: usize) -> Option<u64> {
        let Some(updated_count) = self.record_tracker.try_increase_record_count(count) else {
            self.errors.push(Error::MeasurementRecordCounterOverflow);
            return None;
        };
        Some(updated_count)
    }

    fn update_record_count_after_repeat(
        &mut self,
        count_before_repeat: u64,
        repeat_count: u32,
    ) -> Option<u64> {
        let Some(updated_count) = self
            .record_tracker
            .try_update_record_count_after_repeat(count_before_repeat, repeat_count)
        else {
            self.errors.push(Error::MeasurementRecordCounterOverflow);
            return None;
        };
        Some(updated_count)
    }

    fn lower_circuit(&mut self, circuit: &parser::Circuit) -> Circuit {
        Circuit {
            span: circuit.span,
            items: self.lower_items(&circuit.items),
        }
    }

    fn lower_items(&mut self, items: &[parser::Item]) -> Vec<Item> {
        let mut lowered_items = Vec::new();

        for item in items {
            match item {
                parser::Item::Block(block) => {
                    if let Some(block) = self.lower_block(block) {
                        lowered_items.push(Item::Block(block));
                    }
                }
                parser::Item::Instruction(parser_instruction) => {
                    lowered_items.extend(
                        self.lower_instruction(parser_instruction)
                            .into_iter()
                            .map(Item::Instruction),
                    );
                }
            }
        }

        lowered_items
    }

    fn lower_block(&mut self, block: &parser::Block) -> Option<Block> {
        let parser::Block {
            block_instruction, ..
        } = block;

        match block_instruction.name.as_str() {
            "REPEAT" => Some(self.lower_repeat_block(block)?),
            "SELECT" => Some(self.lower_select_block(block)?),
            _ => {
                self.unknown(block_instruction);
                None
            }
        }
    }

    fn lower_repeat_block(&mut self, block: &parser::Block) -> Option<Block> {
        let parser::Block {
            block_instruction: instruction,
            items,
            ..
        } = block;

        self.unsupported_args(instruction);

        if instruction.targets.is_empty() {
            self.push_error(Error::MissingTarget {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        } else if instruction.targets.len() > 1 {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: instruction
                    .targets
                    .get(1)
                    .map(|t| t.span)
                    .unwrap_or(instruction.span),
            });
            return None;
        }

        let repeat_target = &instruction.targets[0];
        let parser::TargetKind::Qubit {
            // arbitrary choice by the parser, it's just a number of repeats, not a qubit
            value: num_repeats,
            negated: false,
        } = repeat_target.kind
        else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: repeat_target.span,
            });
            return None;
        };

        if num_repeats == 0 {
            self.push_error(Error::ZeroRepeatCount {
                span: repeat_target.span,
            });
            return None;
        }

        let record_count_before_repeat = self.record_tracker.count;
        let body = self.lower_items(items);
        // The body is lowered once but executes num_repeats times, so later record
        // references must account for the records produced by every iteration.
        self.update_record_count_after_repeat(record_count_before_repeat, num_repeats)?;

        Some(Block::RepeatBlock {
            count: num_repeats,
            body,
        })
    }

    fn lower_select_block(&mut self, block: &parser::Block) -> Option<Block> {
        let parser::Block {
            block_instruction: instruction,
            items,
            ..
        } = block;

        self.unsupported_args(instruction);
        if !instruction.targets.is_empty() {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: instruction
                    .targets
                    .first()
                    .map(|t| t.span)
                    .unwrap_or(instruction.span),
            });
            return None;
        }
        self.record_tracker.enter_select_scope();
        let body = self.lower_items(items);
        self.record_tracker.exit_select_scope();

        Some(Block::SelectBlock { body })
    }

    fn lower_instruction(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        match instruction.name.as_str() {
            // Single Qubit Clifford Gates
            name @ ("I" | "X" | "Y" | "Z" | "C_NXYZ" | "C_NZYX" | "C_XNYZ" | "C_XYNZ" | "C_XYZ"
            | "C_ZNYX" | "C_ZYNX" | "C_ZYX" | "H" | "H_XZ" | "H_NXY" | "H_NXZ"
            | "H_NYZ" | "H_XY" | "H_YZ" | "S" | "SQRT_Z" | "SQRT_X" | "SQRT_X_DAG"
            | "SQRT_Y" | "SQRT_Y_DAG" | "S_DAG" | "SQRT_Z_DAG") => {
                self.broadcast_single_qubit_gate(instruction, SingleQubitGateKind::from_name(name))
            }

            // Two Qubit Clifford Gates
            name @ ("CXSWAP" | "CZSWAP" | "SWAPCZ" | "II" | "ISWAP" | "ISWAP_DAG" | "SQRT_XX"
            | "SQRT_XX_DAG" | "SQRT_YY" | "SQRT_YY_DAG" | "SQRT_ZZ" | "SQRT_ZZ_DAG"
            | "SWAP" | "SWAPCX" | "XCX" | "XCY" | "YCX" | "YCY") => {
                self.broadcast_two_qubit_gate(instruction, TwoQubitGateKind::from_name(name))
            }
            "CX" | "CNOT" | "ZCX" => self.broadcast_classically_controllable_gate(
                instruction,
                TwoQubitGateKind::CX,
                AllowedRecPosition::First,
                Pauli::X,
            ),
            "CY" | "ZCY" => self.broadcast_classically_controllable_gate(
                instruction,
                TwoQubitGateKind::CY,
                AllowedRecPosition::First,
                Pauli::Y,
            ),
            "CZ" | "ZCZ" => self.broadcast_classically_controllable_gate(
                instruction,
                TwoQubitGateKind::CZ,
                AllowedRecPosition::Either,
                Pauli::Z,
            ),
            "XCZ" => self.broadcast_classically_controllable_gate(
                instruction,
                TwoQubitGateKind::XCZ,
                AllowedRecPosition::Second,
                Pauli::X,
            ),
            "YCZ" => self.broadcast_classically_controllable_gate(
                instruction,
                TwoQubitGateKind::YCZ,
                AllowedRecPosition::Second,
                Pauli::Y,
            ),

            // Noise Channels
            "E" | "CORRELATED_ERROR" => self
                .lower_correlated_error(instruction, CorrelatedErrorKind::Initial)
                .into_iter()
                .collect(),
            "ELSE_CORRELATED_ERROR" => self
                .lower_correlated_error(instruction, CorrelatedErrorKind::Else)
                .into_iter()
                .collect(),
            "DEPOLARIZE1" => {
                self.broadcast_single_qubit_noise(instruction, SingleQubitNoiseKind::Depolarize)
            }
            "DEPOLARIZE2" => self.broadcast_depolarize2(instruction),
            "HERALDED_ERASE" => self.broadcast_heralded_erase(instruction),
            "HERALDED_PAULI_CHANNEL_1" => self.broadcast_heralded_pauli_channel_1(instruction),
            "II_ERROR" => {
                self.expect_probabilities(instruction);
                self.expect_qubit_pairs(instruction, false);
                Vec::new()
            }
            "I_ERROR" => {
                self.expect_probabilities(instruction);
                self.expect_qubit_targets(instruction, false);
                Vec::new()
            }
            "PAULI_CHANNEL_1" => self.broadcast_pauli_channel_1(instruction),
            "PAULI_CHANNEL_2" => self.broadcast_pauli_channel_2(instruction),
            "X_ERROR" => self.broadcast_single_qubit_noise(
                instruction,
                SingleQubitNoiseKind::Fault(FaultKind::X),
            ),
            "Y_ERROR" => self.broadcast_single_qubit_noise(
                instruction,
                SingleQubitNoiseKind::Fault(FaultKind::Y),
            ),
            "Z_ERROR" => self.broadcast_single_qubit_noise(
                instruction,
                SingleQubitNoiseKind::Fault(FaultKind::Z),
            ),
            "LOSS_ERROR" => self.broadcast_single_qubit_noise(
                instruction,
                SingleQubitNoiseKind::Fault(FaultKind::Loss),
            ),

            // Collapsing Gates
            "M" | "MZ" => self.broadcast_single_qubit_measurement(instruction, false, Pauli::Z),
            "MR" | "MRZ" => self.broadcast_single_qubit_measurement(instruction, true, Pauli::Z),
            "MRX" => self.broadcast_single_qubit_measurement(instruction, true, Pauli::X),
            "MRY" => self.broadcast_single_qubit_measurement(instruction, true, Pauli::Y),
            "MX" => self.broadcast_single_qubit_measurement(instruction, false, Pauli::X),
            "MY" => self.broadcast_single_qubit_measurement(instruction, false, Pauli::Y),
            "R" | "RZ" => self.broadcast_reset(instruction, Pauli::Z),
            "RX" => self.broadcast_reset(instruction, Pauli::X),
            "RY" => self.broadcast_reset(instruction, Pauli::Y),

            // Pair Measurement Gates
            "MXX" => self.broadcast_two_qubit_measurement(instruction, PauliPair::XX),
            "MYY" => self.broadcast_two_qubit_measurement(instruction, PauliPair::YY),
            "MZZ" => self.broadcast_two_qubit_measurement(instruction, PauliPair::ZZ),

            // Generalized Pauli Product Gates
            "MPP" => self.broadcast_pauli_product_measurement(instruction),
            name @ ("SPP" | "SPP_DAG") => self
                .broadcast_pauli_product_gate(instruction, PauliProductGateKind::from_name(name)),

            // Control Flow
            "REPEAT" | "SELECT" => {
                self.push_error(Error::InstructionWithoutBlock {
                    instruction: instruction.name.clone(),
                    span: instruction.span,
                });
                Vec::new()
            }
            "REQUIRE" => self.lower_require(instruction).into_iter().collect(),
            "NOTLEAKED" => self.lower_not_leaked(instruction).into_iter().collect(),

            // Miscellaneous
            "PEEK_LOSS" => self.broadcast_peek_loss(instruction),

            // Annotations
            "DETECTOR" => self.lower_detector(instruction).into_iter().collect(),
            "MPAD" => self.broadcast_mpad(instruction),
            "OBSERVABLE_INCLUDE" => self
                .lower_observable_include(instruction)
                .into_iter()
                .collect(),
            "QUBIT_COORDS" => self.broadcast_qubit_coords(instruction),
            "SHIFT_COORDS" => self.lower_shift_coords(instruction).into_iter().collect(),
            "TICK" => self.lower_tick(instruction).into_iter().collect(),

            // Non-Clifford Gates
            name @ ("T" | "T_DAG") => {
                self.broadcast_single_qubit_gate(instruction, SingleQubitGateKind::from_name(name))
            }
            name @ ("TPP" | "TPP_DAG") => self
                .broadcast_pauli_product_gate(instruction, PauliProductGateKind::from_name(name)),
            name @ "CH" => {
                self.broadcast_two_qubit_gate(instruction, TwoQubitGateKind::from_name(name))
            }
            name @ ("CCZ" | "CCX") => {
                self.broadcast_three_qubit_gate(instruction, ThreeQubitGateKind::from_name(name))
            }
            "R_X" => self.broadcast_single_qubit_rotation(instruction, Pauli::X),
            "R_Y" => self.broadcast_single_qubit_rotation(instruction, Pauli::Y),
            "R_Z" => self.broadcast_single_qubit_rotation(instruction, Pauli::Z),
            "U3" | "U" => self.broadcast_u3(instruction),
            "R_XX" => self.broadcast_two_qubit_rotation(instruction, PauliPair::XX),
            "R_YY" => self.broadcast_two_qubit_rotation(instruction, PauliPair::YY),
            "R_ZZ" => self.broadcast_two_qubit_rotation(instruction, PauliPair::ZZ),
            "R_PAULI" => self.broadcast_pauli_product_rotation(instruction),
            _ => {
                self.unknown(instruction);
                Vec::new()
            }
        }
    }

    fn broadcast_reset(
        &mut self,
        instruction: &parser::Instruction,
        basis: Pauli,
    ) -> Vec<Instruction> {
        self.unsupported_args(instruction);
        let qubit_targets = self.expect_qubit_targets(instruction, false);
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::Reset { qubit, basis },
            })
            .collect()
    }

    fn broadcast_single_qubit_gate(
        &mut self,
        instruction: &parser::Instruction,
        gate: SingleQubitGateKind,
    ) -> Vec<Instruction> {
        self.unsupported_args(instruction);
        let qubit_targets = self.expect_qubit_targets(instruction, false);
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::SingleQubitGate { qubit, gate },
            })
            .collect()
    }

    fn broadcast_two_qubit_gate(
        &mut self,
        instruction: &parser::Instruction,
        gate: TwoQubitGateKind,
    ) -> Vec<Instruction> {
        self.unsupported_args(instruction);
        let qubit_target_pairs = self.expect_qubit_pairs(instruction, false);
        qubit_target_pairs
            .into_iter()
            .map(|[(q0, _), (q1, _)]| Instruction {
                span: instruction.span,
                kind: InstructionKind::TwoQubitGate { q0, q1, gate },
            })
            .collect()
    }

    fn broadcast_three_qubit_gate(
        &mut self,
        instruction: &parser::Instruction,
        gate: ThreeQubitGateKind,
    ) -> Vec<Instruction> {
        self.unsupported_args(instruction);
        let qubit_triples = self.expect_qubit_triples(instruction);
        qubit_triples
            .into_iter()
            .map(|[q0, q1, q2]| Instruction {
                span: instruction.span,
                kind: InstructionKind::ThreeQubitGate { q0, q1, q2, gate },
            })
            .collect()
    }

    fn broadcast_classically_controllable_gate(
        &mut self,
        instruction: &parser::Instruction,
        gate: TwoQubitGateKind,
        allowed_rec_position: AllowedRecPosition,
        classically_controlled_pauli: Pauli,
    ) -> Vec<Instruction> {
        self.unsupported_args(instruction);
        let Some(target_pairs) = self.expect_target_pairs(instruction) else {
            return Vec::new();
        };

        let mut instructions = Vec::with_capacity(target_pairs.len());
        for pair in target_pairs {
            let first_is_record =
                matches!(&pair[0].kind, parser::TargetKind::MeasurementRecord { .. });
            let second_is_record =
                matches!(&pair[1].kind, parser::TargetKind::MeasurementRecord { .. });

            let kind = match (first_is_record, second_is_record) {
                (false, false) => self
                    .expect_qubit_pair(instruction, pair, false)
                    .map(|[(q0, _), (q1, _)]| InstructionKind::TwoQubitGate { gate, q0, q1 }),
                (true, true) => {
                    self.push_error(Error::BothTargetsAreMeasurementRecords {
                        instruction: instruction.name.clone(),
                        span: Span {
                            lo: pair[0].span.lo,
                            hi: pair[1].span.hi,
                        },
                    });
                    None
                }
                (true, false) => self.lower_classically_controlled_pauli(
                    instruction,
                    &pair[0],
                    &pair[1],
                    allowed_rec_position.allows_first(),
                    classically_controlled_pauli,
                ),
                (false, true) => self.lower_classically_controlled_pauli(
                    instruction,
                    &pair[1],
                    &pair[0],
                    allowed_rec_position.allows_second(),
                    classically_controlled_pauli,
                ),
            };

            if let Some(kind) = kind {
                instructions.push(Instruction {
                    span: instruction.span,
                    kind,
                });
            }
        }
        instructions
    }

    fn lower_classically_controlled_pauli(
        &mut self,
        instruction: &parser::Instruction,
        record_target: &parser::Target,
        qubit_target: &parser::Target,
        record_position_is_allowed: bool,
        pauli: Pauli,
    ) -> Option<InstructionKind> {
        if !record_position_is_allowed {
            self.push_error(Error::MisplacedMeasurementRecord {
                instruction: instruction.name.clone(),
                span: record_target.span,
            });
            return None;
        }

        let control = self.expect_measurement_record(instruction, record_target)?;
        let (target, _) = self.expect_qubit(instruction, qubit_target, false)?;
        Some(InstructionKind::ClassicallyControlledPauli {
            control,
            target,
            pauli,
        })
    }

    fn broadcast_pauli_product_gate(
        &mut self,
        instruction: &parser::Instruction,
        gate: PauliProductGateKind,
    ) -> Vec<Instruction> {
        self.unsupported_args(instruction);
        let pauli_products = self.expect_pauli_products(instruction);
        pauli_products
            .into_iter()
            .map(|pauli_product| Instruction {
                span: instruction.span,
                kind: InstructionKind::PauliProductGate {
                    gate,
                    product: pauli_product,
                },
            })
            .collect()
    }

    fn broadcast_single_qubit_measurement(
        &mut self,
        instruction: &parser::Instruction,
        reset: bool,
        observable: Pauli,
    ) -> Vec<Instruction> {
        let Some(readout_noise) = self.expect_optional_readout_probability(instruction) else {
            return Vec::new();
        };
        let qubit_targets = self.expect_qubit_targets(instruction, true);
        self.increase_record_count(qubit_targets.len());
        qubit_targets
            .into_iter()
            .map(|(qubit, negated)| Instruction {
                span: instruction.span,
                kind: InstructionKind::SingleQubitMeasurement {
                    reset,
                    observable,
                    readout_noise,
                    negated,
                    qubit,
                },
            })
            .collect()
    }

    fn broadcast_two_qubit_measurement(
        &mut self,
        instruction: &parser::Instruction,
        observable: PauliPair,
    ) -> Vec<Instruction> {
        let Some(readout_noise) = self.expect_optional_readout_probability(instruction) else {
            return Vec::new();
        };
        let qubit_target_pairs = self.expect_qubit_pairs(instruction, true);
        self.increase_record_count(qubit_target_pairs.len());
        qubit_target_pairs
            .into_iter()
            .map(|[(q0, neg0), (q1, neg1)]| Instruction {
                span: instruction.span,
                kind: InstructionKind::TwoQubitMeasurement {
                    readout_noise,
                    observable,
                    negated: neg0 ^ neg1,
                    q0,
                    q1,
                },
            })
            .collect()
    }

    fn broadcast_pauli_product_measurement(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Vec<Instruction> {
        let Some(readout_noise) = self.expect_optional_readout_probability(instruction) else {
            return Vec::new();
        };
        let pauli_products = self.expect_pauli_products(instruction);
        self.increase_record_count(pauli_products.len());
        pauli_products
            .into_iter()
            .map(|pauli_product| Instruction {
                span: instruction.span,
                kind: InstructionKind::PauliProductMeasurement {
                    readout_noise,
                    product: pauli_product,
                },
            })
            .collect()
    }

    fn broadcast_single_qubit_noise(
        &mut self,
        instruction: &parser::Instruction,
        kind: SingleQubitNoiseKind,
    ) -> Vec<Instruction> {
        let Some(probability) = self.expect_probability(instruction) else {
            return Vec::new();
        };
        let qubit_targets = self.expect_qubit_targets(instruction, false);
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::Noise(Noise::SingleQubitNoise {
                    kind,
                    probability,
                    qubit,
                }),
            })
            .collect()
    }

    fn broadcast_heralded_erase(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let Some(probability) = self.expect_probability(instruction) else {
            return Vec::new();
        };
        let qubit_targets = self.expect_qubit_targets(instruction, false);
        self.increase_record_count(qubit_targets.len());
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::Noise(Noise::HeraldedErase { probability, qubit }),
            })
            .collect()
    }

    fn broadcast_depolarize2(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let Some(probability) = self.expect_probability(instruction) else {
            return Vec::new();
        };
        let qubit_pairs = self.expect_qubit_pairs(instruction, false);
        qubit_pairs
            .into_iter()
            .map(|[(q0, _), (q1, _)]| Instruction {
                span: instruction.span,
                kind: InstructionKind::Noise(Noise::Depolarize2 {
                    probability,
                    q0,
                    q1,
                }),
            })
            .collect()
    }

    fn broadcast_heralded_pauli_channel_1(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Vec<Instruction> {
        let Some(probabilities): Option<[Probability; 4]> =
            self.expect_n_probabilities(instruction)
        else {
            return Vec::new();
        };
        let qubit_targets = self.expect_qubit_targets(instruction, false);
        self.increase_record_count(qubit_targets.len());
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::Noise(Noise::HeraldedPauliChannel1 {
                    probabilities,
                    qubit,
                }),
            })
            .collect()
    }

    fn broadcast_pauli_channel_1(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let Some(probabilities): Option<[Probability; 3]> =
            self.expect_n_probabilities(instruction)
        else {
            return Vec::new();
        };
        let qubit_targets = self.expect_qubit_targets(instruction, false);
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::Noise(Noise::PauliChannel1 {
                    probabilities,
                    qubit,
                }),
            })
            .collect()
    }

    fn broadcast_pauli_channel_2(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let Some(probabilities): Option<[Probability; 15]> =
            self.expect_n_probabilities(instruction)
        else {
            return Vec::new();
        };
        let qubit_target_pairs = self.expect_qubit_pairs(instruction, false);
        qubit_target_pairs
            .into_iter()
            .map(|[(q0, _), (q1, _)]| Instruction {
                span: instruction.span,
                kind: InstructionKind::Noise(Noise::PauliChannel2 {
                    probabilities,
                    q0,
                    q1,
                }),
            })
            .collect()
    }

    fn broadcast_peek_loss(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let Some(readout_noise) = self.expect_optional_readout_probability(instruction) else {
            return Vec::new();
        };
        let qubit_targets = self.expect_qubit_targets(instruction, false);
        self.increase_record_count(qubit_targets.len());
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::PeekLoss {
                    readout_noise,
                    qubit,
                },
            })
            .collect()
    }

    fn lower_detector(&mut self, instruction: &parser::Instruction) -> Option<Instruction> {
        let coordinates = self.expect_coordinates(instruction)?;
        let records = self.expect_measurement_records(instruction);

        Some(Instruction {
            span: instruction.span,
            kind: InstructionKind::Annotation(Annotation::Detector {
                coordinates,
                records,
            }),
        })
    }

    fn broadcast_mpad(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let Some(readout_noise) = self.expect_optional_readout_probability(instruction) else {
            return Vec::new();
        };

        let mut instructions = Vec::with_capacity(instruction.targets.len());
        for target in &instruction.targets {
            let Some((value, _)) = self.expect_qubit(instruction, target, false) else {
                continue;
            };
            let value = match value {
                0 => false,
                1 => true,
                _ => {
                    self.push_error(Error::UnsupportedTarget {
                        instruction: instruction.name.clone(),
                        span: target.span,
                    });
                    continue;
                }
            };

            instructions.push(Instruction {
                span: instruction.span,
                kind: InstructionKind::Annotation(Annotation::MeasurementPadding {
                    readout_noise,
                    value,
                }),
            });
        }
        self.increase_record_count(instructions.len());
        instructions
    }

    fn lower_observable_include(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<Instruction> {
        let logical_observable = self.expect_logical_observable_index(instruction)?;
        let targets = self.expect_observable_targets(instruction)?;

        Some(Instruction {
            span: instruction.span,
            kind: InstructionKind::Annotation(Annotation::ObservableInclude {
                logical_observable,
                targets,
            }),
        })
    }

    fn broadcast_qubit_coords(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let Some(coordinates) = self.expect_coordinates(instruction) else {
            return Vec::new();
        };

        let qubit_targets = self.expect_qubit_targets(instruction, false);
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::Annotation(Annotation::QubitCoordinates {
                    coordinates: coordinates.clone(),
                    qubit,
                }),
            })
            .collect()
    }

    fn lower_shift_coords(&mut self, instruction: &parser::Instruction) -> Option<Instruction> {
        if instruction.args.is_empty() {
            self.push_error(Error::MissingArg {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }

        let coordinates = self.expect_coordinates(instruction)?;
        self.unsupported_targets(instruction);

        Some(Instruction {
            span: instruction.span,
            kind: InstructionKind::Annotation(Annotation::ShiftCoordinates {
                offsets: coordinates,
            }),
        })
    }

    fn lower_tick(&mut self, instruction: &parser::Instruction) -> Option<Instruction> {
        self.unsupported_args(instruction);
        self.unsupported_targets(instruction);
        Some(Instruction {
            span: instruction.span,
            kind: InstructionKind::Annotation(Annotation::Tick),
        })
    }

    fn broadcast_single_qubit_rotation(
        &mut self,
        instruction: &parser::Instruction,
        axis: Pauli,
    ) -> Vec<Instruction> {
        let Some(angle) = self.expect_angle(instruction) else {
            return Vec::new();
        };

        let qubit_targets = self.expect_qubit_targets(instruction, false);
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::SingleQubitRotation { axis, angle, qubit },
            })
            .collect()
    }

    fn broadcast_two_qubit_rotation(
        &mut self,
        instruction: &parser::Instruction,
        axis: PauliPair,
    ) -> Vec<Instruction> {
        let Some(angle) = self.expect_angle(instruction) else {
            return Vec::new();
        };

        let qubit_pairs = self.expect_qubit_pairs(instruction, false);
        qubit_pairs
            .into_iter()
            .map(|[(q0, _), (q1, _)]| Instruction {
                span: instruction.span,
                kind: InstructionKind::TwoQubitRotation {
                    axis,
                    angle,
                    q0,
                    q1,
                },
            })
            .collect()
    }

    fn broadcast_u3(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let angles: Option<[Radians; 3]> = self.expect_n_angles(instruction);
        let Some([theta, phi, lambda]) = angles else {
            return Vec::new();
        };

        let qubit_targets = self.expect_qubit_targets(instruction, false);
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::U3 {
                    qubit,
                    theta,
                    phi,
                    lambda,
                },
            })
            .collect()
    }

    fn broadcast_pauli_product_rotation(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Vec<Instruction> {
        let Some(angle) = self.expect_angle(instruction) else {
            return Vec::new();
        };

        let pauli_products = self.expect_pauli_products(instruction);
        pauli_products
            .into_iter()
            .map(|product| Instruction {
                span: instruction.span,
                kind: InstructionKind::PauliProductRotation { angle, product },
            })
            .collect()
    }

    fn lower_correlated_error(
        &mut self,
        instruction: &parser::Instruction,
        kind: CorrelatedErrorKind,
    ) -> Option<Instruction> {
        let probability = self.expect_probability(instruction)?;

        let mut faults = Vec::with_capacity(instruction.targets.len());
        for target in &instruction.targets {
            let Some(fault) = self.expect_fault(instruction, target) else {
                continue;
            };

            faults.push(fault);
        }
        Some(Instruction {
            span: instruction.span,
            kind: InstructionKind::Noise(Noise::CorrelatedError {
                kind,
                probability,
                faults,
            }),
        })
    }

    fn lower_require(&mut self, instruction: &parser::Instruction) -> Option<Instruction> {
        self.unsupported_args(instruction);
        if instruction.targets.is_empty() {
            self.push_error(Error::MissingTarget {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }

        let records = self.expect_negatable_measurement_records(instruction);
        if records.is_empty() {
            return None;
        }

        self.validate_instruction_in_select_block(instruction)?;
        self.validate_any_measurement_record_in_scope(
            instruction,
            &records
                .iter()
                .map(|record| record.record)
                .collect::<Vec<_>>(),
        )?;

        Some(Instruction {
            span: instruction.span,
            kind: InstructionKind::Require { records },
        })
    }

    fn lower_not_leaked(&mut self, instruction: &parser::Instruction) -> Option<Instruction> {
        self.unsupported_args(instruction);
        if instruction.targets.is_empty() {
            self.push_error(Error::MissingTarget {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }

        let records = self.expect_measurement_records(instruction);
        if records.is_empty() {
            return None;
        }

        self.validate_instruction_in_select_block(instruction)?;
        self.validate_any_measurement_record_in_scope(instruction, &records)?;

        Some(Instruction {
            span: instruction.span,
            kind: InstructionKind::NotLeaked { records },
        })
    }

    fn expect_qubit_targets(
        &mut self,
        instruction: &parser::Instruction,
        allow_negated: bool,
    ) -> Vec<(StimQubitId, bool)> {
        let mut qubit_targets = Vec::new();
        for target in &instruction.targets {
            let Some(qubit_target) = self.expect_qubit(instruction, target, allow_negated) else {
                continue;
            };
            qubit_targets.push(qubit_target);
        }
        qubit_targets
    }

    fn expect_qubit_pairs(
        &mut self,
        instruction: &parser::Instruction,
        allow_negated: bool,
    ) -> Vec<[(StimQubitId, bool); 2]> {
        let Some(pairs) = self.expect_target_pairs(instruction) else {
            return Vec::new();
        };

        let mut qubit_target_pairs = Vec::new();
        for pair in pairs {
            let Some(qubit_target_pair) = self.expect_qubit_pair(instruction, pair, allow_negated)
            else {
                continue;
            };
            qubit_target_pairs.push(qubit_target_pair);
        }
        qubit_target_pairs
    }

    fn expect_qubit_triples(&mut self, instruction: &parser::Instruction) -> Vec<[StimQubitId; 3]> {
        let Some(triples) = self.expect_target_triples(instruction) else {
            return Vec::new();
        };

        let mut qubit_triples = Vec::new();
        for triple in triples {
            let Some(qubit_triple) = self.expect_qubit_triple(instruction, triple) else {
                continue;
            };
            qubit_triples.push(qubit_triple);
        }
        qubit_triples
    }

    fn expect_pauli_products(&mut self, instruction: &parser::Instruction) -> Vec<PauliProduct> {
        let mut pauli_products = Vec::new();
        for target in &instruction.targets {
            let factors = self.expect_pauli_product_factors(instruction, target);
            if factors.is_empty() {
                continue;
            }

            let Some(product) = self.canonicalize_pauli_product(instruction, target, factors)
            else {
                continue;
            };
            pauli_products.push(product);
        }
        pauli_products
    }

    fn expect_measurement_records(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Vec<MeasurementRecord> {
        let mut measurement_records = Vec::new();
        for target in &instruction.targets {
            let Some(measurement_record) = self.expect_measurement_record(instruction, target)
            else {
                continue;
            };
            measurement_records.push(measurement_record);
        }
        measurement_records
    }

    fn expect_negatable_measurement_records(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Vec<NegatableMeasurementRecord> {
        let mut measurement_records = Vec::new();
        for target in &instruction.targets {
            let Some(measurement_record) =
                self.expect_negatable_measurement_record(instruction, target)
            else {
                continue;
            };
            measurement_records.push(measurement_record);
        }
        measurement_records
    }

    fn validate_instruction_in_select_block(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<()> {
        if !self.record_tracker.has_active_select_scope() {
            self.push_error(Error::InstructionOutsideSelectBlock {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }
        Some(())
    }

    fn validate_any_measurement_record_in_scope(
        &mut self,
        instruction: &parser::Instruction,
        records: &[MeasurementRecord],
    ) -> Option<()> {
        if records.iter().all(|record| {
            self.record_tracker
                .is_offset_outside_current_select_scope(record.offset)
        }) {
            self.push_error(Error::AllMeasurementRecordsOutOfScope {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }
        Some(())
    }

    fn expect_observable_targets(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<Vec<ObservableTarget>> {
        let mut observable_targets = Vec::with_capacity(instruction.targets.len());
        let mut has_invalid_target = false;
        for target in &instruction.targets {
            match &target.kind {
                parser::TargetKind::MeasurementRecord { .. } => {
                    let Some(record) = self.expect_measurement_record(instruction, target) else {
                        has_invalid_target = true;
                        continue;
                    };
                    observable_targets.push(ObservableTarget::Record(record));
                }
                parser::TargetKind::Pauli(_) => {
                    let Some(pauli) = self.expect_pauli_target(instruction, target, false) else {
                        has_invalid_target = true;
                        continue;
                    };
                    observable_targets.push(ObservableTarget::Pauli(PauliFactor {
                        pauli: pauli.pauli,
                        qubit: pauli.qubit,
                    }));
                }
                _ => {
                    self.push_error(Error::UnsupportedTarget {
                        instruction: instruction.name.clone(),
                        span: target.span,
                    });
                    has_invalid_target = true;
                }
            }
        }

        if has_invalid_target {
            None
        } else {
            Some(observable_targets)
        }
    }

    /// Converts a Pauli product to a canonical form: one factor per qubit, sorted by
    /// qubit index, with identity factors removed. Rejects anti-Hermitian products and
    /// represents an overall phase of -1 as a negation.
    fn canonicalize_pauli_product(
        &mut self,
        instruction: &parser::Instruction,
        target: &parser::Target,
        mut factors: Vec<PauliTarget>,
    ) -> Option<PauliProduct> {
        let mut phase = 0;
        // must be stable so that same-qubit factors keep their relative order
        factors.sort_by_key(|factor| factor.qubit);

        let mut canonical_factors = Vec::new();
        for same_qubit_factors in factors.chunk_by(|a, b| a.qubit == b.qubit) {
            let mut accumulated = None;
            for factor in same_qubit_factors {
                if factor.negated {
                    phase = (phase + 2) % 4;
                }
                accumulated = match accumulated {
                    None => Some(factor.pauli),
                    Some(pauli) => {
                        let (product, product_phase) = pauli.multiply(factor.pauli);
                        phase = (phase + product_phase) % 4;
                        product
                    }
                };
            }
            if let Some(pauli) = accumulated {
                canonical_factors.push(PauliFactor {
                    pauli,
                    qubit: same_qubit_factors[0].qubit,
                });
            }
        }

        if phase % 2 != 0 {
            // a phase of i or -i makes the product anti-Hermitian, so it has no measurable eigenvalues
            self.push_error(Error::AntiHermitianPauliProduct { span: target.span });
            return None;
        }
        if canonical_factors.is_empty() && instruction.name == "MPP" {
            // TODO: an empty product measures the identity, which needs support for appending to measurement records
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        }

        // a phase of i^2 = -1 flips the measurement result
        let negated = phase == 2;
        Some(PauliProduct {
            factors: canonical_factors,
            negated,
        })
    }

    fn expect_qubit(
        &mut self,
        instruction: &parser::Instruction,
        target: &parser::Target,
        allow_negated: bool,
    ) -> Option<(StimQubitId, bool)> {
        let parser::TargetKind::Qubit { value, negated } = target.kind else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        };

        if negated && !allow_negated {
            self.push_error(Error::NegatedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        }
        Some((value, negated))
    }

    fn expect_qubit_pair(
        &mut self,
        instruction: &parser::Instruction,
        pair: &[parser::Target],
        allow_negated: bool,
    ) -> Option<[(StimQubitId, bool); 2]> {
        let (q0, neg0) = self.expect_qubit(instruction, &pair[0], allow_negated)?;
        let (q1, neg1) = self.expect_qubit(instruction, &pair[1], allow_negated)?;

        if q0 == q1 {
            self.push_error(Error::RepeatedQubit {
                instruction: instruction.name.clone(),
                qubit: q1,
                span: pair[1].span,
            });
            return None;
        }

        Some([(q0, neg0), (q1, neg1)])
    }

    fn expect_qubit_triple(
        &mut self,
        instruction: &parser::Instruction,
        triple: &[parser::Target],
    ) -> Option<[StimQubitId; 3]> {
        let (q0, _) = self.expect_qubit(instruction, &triple[0], false)?;
        let (q1, _) = self.expect_qubit(instruction, &triple[1], false)?;
        let (q2, _) = self.expect_qubit(instruction, &triple[2], false)?;

        let (repeated_qubit_value, repeated_qubit_span) = if q0 == q1 {
            (q1, triple[1].span)
        } else if q0 == q2 || q1 == q2 {
            (q2, triple[2].span)
        } else {
            return Some([q0, q1, q2]);
        };

        self.push_error(Error::RepeatedQubit {
            instruction: instruction.name.clone(),
            qubit: repeated_qubit_value,
            span: repeated_qubit_span,
        });
        None
    }

    fn expect_pauli_product_factors(
        &mut self,
        instruction: &parser::Instruction,
        target: &parser::Target,
    ) -> Vec<PauliTarget> {
        match &target.kind {
            parser::TargetKind::PauliProduct { factors } => factors.clone(),
            _ => self
                .expect_pauli_target(instruction, target, true)
                .into_iter()
                .collect(),
        }
    }

    fn expect_pauli_target(
        &mut self,
        instruction: &parser::Instruction,
        target: &parser::Target,
        allow_negated: bool,
    ) -> Option<PauliTarget> {
        let parser::TargetKind::Pauli(pauli_target) = target.kind else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        };

        if pauli_target.negated && !allow_negated {
            self.push_error(Error::NegatedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        }

        Some(pauli_target)
    }

    fn expect_target_pairs<'a>(
        &mut self,
        instruction: &'a parser::Instruction,
    ) -> Option<Chunks<'a, parser::Target>> {
        if !instruction.targets.len().is_multiple_of(2) {
            self.push_error(Error::OddTargetCount {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }
        Some(instruction.targets.chunks(2))
    }

    fn expect_target_triples<'a>(
        &mut self,
        instruction: &'a parser::Instruction,
    ) -> Option<Chunks<'a, parser::Target>> {
        if !instruction.targets.len().is_multiple_of(3) {
            self.push_error(Error::TargetCountNotMultipleOfThree {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }
        Some(instruction.targets.chunks(3))
    }

    fn expect_measurement_record(
        &mut self,
        instruction: &parser::Instruction,
        target: &parser::Target,
    ) -> Option<MeasurementRecord> {
        let measurement_record = self.expect_negatable_measurement_record(instruction, target)?;

        if measurement_record.negated {
            self.push_error(Error::NegatedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        }

        Some(measurement_record.record)
    }

    fn expect_negatable_measurement_record(
        &mut self,
        instruction: &parser::Instruction,
        target: &parser::Target,
    ) -> Option<NegatableMeasurementRecord> {
        let parser::TargetKind::MeasurementRecord {
            negated,
            value: offset,
        } = target.kind
        else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        };

        if self.record_tracker.is_offset_out_of_bounds(offset) {
            self.push_error(Error::MeasurementRecordOutOfBounds { span: target.span });
            return None;
        };

        Some(NegatableMeasurementRecord {
            record: MeasurementRecord {
                offset,
                span: target.span,
            },
            negated,
        })
    }

    fn expect_fault(
        &mut self,
        instruction: &parser::Instruction,
        target: &parser::Target,
    ) -> Option<Fault> {
        if let parser::TargetKind::Loss { value } = target.kind {
            return Some(Fault {
                kind: FaultKind::Loss,
                qubit: value,
            });
        }

        let pauli_target = self.expect_pauli_target(instruction, target, false)?;

        Some(Fault {
            kind: FaultKind::from_pauli(pauli_target.pauli),
            qubit: pauli_target.qubit,
        })
    }

    fn expect_angle(&mut self, instruction: &parser::Instruction) -> Option<Radians> {
        let [angle]: [Radians; 1] = self.expect_n_angles(instruction)?;
        Some(angle)
    }

    fn expect_n_angles<const N: usize>(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<[Radians; N]> {
        let args: &[parser::Arg; N] = self.expect_n_args(instruction)?;
        let mut radians = [0.0; N];
        let mut has_invalid_angle = false;

        for (index, arg) in args.iter().enumerate() {
            let angle_in_radians = match arg.value {
                ArgValue::Default(half_turns) => half_turns * PI,
                ArgValue::Radians(radians) => radians,
            };

            if angle_in_radians.is_finite() {
                radians[index] = angle_in_radians;
            } else {
                self.push_error(Error::InvalidAngle {
                    instruction: instruction.name.clone(),
                    span: arg.span,
                });
                has_invalid_angle = true;
            }
        }

        if has_invalid_angle {
            None
        } else {
            Some(radians)
        }
    }

    fn expect_default_arg_value(
        &mut self,
        instruction: &parser::Instruction,
        arg: &parser::Arg,
    ) -> Option<f64> {
        let ArgValue::Default(value) = arg.value else {
            self.push_error(Error::UnexpectedRadians {
                instruction: instruction.name.clone(),
                span: arg.span,
            });
            return None;
        };
        Some(value)
    }

    fn expect_coordinates(&mut self, instruction: &parser::Instruction) -> Option<Vec<f64>> {
        if instruction.args.len() > MAX_COORDINATES {
            self.push_error(Error::TooManyArgs {
                instruction: instruction.name.clone(),
                expected: MAX_COORDINATES,
                found: instruction.args.len(),
                span: args_span(&instruction.args[MAX_COORDINATES..]),
            });
            return None;
        }

        let mut coordinates = Vec::with_capacity(instruction.args.len());
        let mut has_invalid_coordinate = false;
        for arg in &instruction.args {
            let Some(value) = self.expect_default_arg_value(instruction, arg) else {
                has_invalid_coordinate = true;
                continue;
            };
            coordinates.push(value);
        }

        if has_invalid_coordinate {
            None
        } else {
            Some(coordinates)
        }
    }

    fn expect_logical_observable_index(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<u32> {
        let args: &[parser::Arg; 1] = self.expect_n_args(instruction)?;
        let value = self.expect_default_arg_value(instruction, &args[0])?;

        // value is parsed as f64 but represents an unsigned integer
        let logical_observable = value as u32;
        if f64::from(logical_observable) != value {
            self.push_error(Error::InvalidLogicalObservableIndex { span: args[0].span });
            return None;
        }

        Some(logical_observable)
    }

    fn expect_optional_readout_probability(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<Probability> {
        if instruction.args.is_empty() {
            return Some(0.0);
        }
        self.expect_probability(instruction)
    }

    fn expect_probability(&mut self, instruction: &parser::Instruction) -> Option<Probability> {
        let [probability]: [Probability; 1] = self.expect_n_probabilities(instruction)?;
        Some(probability)
    }

    fn expect_n_probabilities<const N: usize>(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<[Probability; N]> {
        self.expect_n_args::<N>(instruction)?;
        self.expect_probabilities(instruction)?.try_into().ok()
    }

    fn expect_probabilities(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<Vec<Probability>> {
        let mut probabilities = Vec::with_capacity(instruction.args.len());
        let mut has_invalid_probability = false;

        for arg in &instruction.args {
            let Some(value) = self.expect_default_arg_value(instruction, arg) else {
                has_invalid_probability = true;
                continue;
            };
            if self
                .validate_probability(instruction, arg.span, value)
                .is_none()
            {
                has_invalid_probability = true;
                continue;
            }

            probabilities.push(value);
        }

        if has_invalid_probability {
            return None;
        }

        self.validate_probability_sum(instruction, probabilities.iter().sum())?;
        Some(probabilities)
    }

    fn validate_probability(
        &mut self,
        instruction: &parser::Instruction,
        arg_span: Span,
        probability: f64,
    ) -> Option<()> {
        if (0.0..=1.0).contains(&probability) {
            Some(())
        } else {
            self.push_error(Error::InvalidProbability {
                instruction: instruction.name.clone(),
                probability,
                span: arg_span,
            });
            None
        }
    }

    fn validate_probability_sum(
        &mut self,
        instruction: &parser::Instruction,
        total: f64,
    ) -> Option<()> {
        if total > 1.0 {
            self.push_error(Error::InvalidProbabilitySum {
                instruction: instruction.name.clone(),
                total,
                span: args_span(&instruction.args),
            });
            None
        } else {
            Some(())
        }
    }

    fn expect_n_args<'a, const N: usize>(
        &mut self,
        instruction: &'a parser::Instruction,
    ) -> Option<&'a [parser::Arg; N]> {
        let args = &instruction.args;
        if args.is_empty() {
            self.push_error(Error::MissingArg {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }

        if args.len() > N {
            self.push_error(Error::TooManyArgs {
                instruction: instruction.name.clone(),
                expected: N,
                found: args.len(),
                span: args_span(&args[N..]),
            });
            return None;
        } else if args.len() < N {
            self.push_error(Error::TooFewArgs {
                instruction: instruction.name.clone(),
                expected: N,
                found: args.len(),
                span: args_span(args),
            });
            return None;
        }
        args.as_slice().try_into().ok()
    }

    fn unsupported_args(&mut self, instruction: &parser::Instruction) {
        if !instruction.args.is_empty() {
            self.push_error(Error::UnsupportedArgument {
                instruction: instruction.name.clone(),
                span: args_span(&instruction.args),
            });
        }
    }

    fn unsupported_targets(&mut self, instruction: &parser::Instruction) {
        let Some((first, rest)) = instruction.targets.split_first() else {
            return;
        };
        let last = rest.last().unwrap_or(first);
        self.push_error(Error::UnsupportedTargets {
            instruction: instruction.name.clone(),
            span: Span {
                lo: first.span.lo,
                hi: last.span.hi,
            },
        });
    }

    fn unknown(&mut self, instruction: &parser::Instruction) {
        self.push_error(Error::UnknownInstruction {
            name: instruction.name.clone(),
            span: instruction.span,
        });
    }

    fn push_error(&mut self, error: Error) {
        self.errors.push(error);
    }
}

pub fn lower(input: parser::Circuit) -> (Circuit, Vec<Error>) {
    let mut lowerer = Lowerer::new();
    let circuit = lowerer.lower_circuit(&input);
    (circuit, lowerer.errors)
}
