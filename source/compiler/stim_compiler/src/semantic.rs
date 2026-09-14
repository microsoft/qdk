// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser;
use crate::parser::{ArgValue, PauliTarget, args_span};
use miette::Diagnostic;
use parser::Pauli;
use qsc_data_structures::span::Span;
use std::f64::consts::PI;
use std::slice::Chunks;
use thiserror::Error;

pub type StimQubitId = u32;
pub type Radians = f64;
pub type Probability = f64;

const MAX_COORDINATES: usize = 16;

pub struct MeasurementRecord {
    pub offset: u32,
}

pub struct NegatableMeasurementRecord {
    pub record: MeasurementRecord,
    pub negated: bool,
}

pub struct Circuit {
    pub span: Span,
    pub items: Vec<Item>,
}

pub enum Item {
    Block(Block),
    Instruction(Instruction),
}

pub enum Block {
    RepeatBlock { count: u32, body: Vec<Item> },
    SelectBlock { body: Vec<Item> },
}

pub struct Instruction {
    pub span: Span,
    pub kind: InstructionKind,
}

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

#[derive(Clone, Copy)]
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

#[derive(Clone, Copy)]
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

#[derive(Clone, Copy)]
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

#[derive(Clone, Copy)]
pub enum PauliPair {
    XX,
    YY,
    ZZ,
}

pub struct PauliProduct {
    pub factors: Vec<PauliFactor>,
    pub negated: bool,
}

pub struct PauliFactor {
    pub pauli: Pauli,
    pub qubit: StimQubitId,
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
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

#[derive(Clone, Copy)]
pub enum CorrelatedErrorKind {
    Initial,
    Else,
}

#[derive(Clone, Copy)]
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
}

pub struct Fault {
    pub kind: FaultKind,
    pub qubit: StimQubitId,
}

#[derive(Clone, Copy)]
pub enum SingleQubitNoiseKind {
    Depolarize,
    HeraldedErase,
    Fault(FaultKind),
}
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

pub enum ObservableTarget {
    Record(MeasurementRecord),
    Pauli(PauliFactor),
}

#[derive(Clone, Debug, Error, Diagnostic)]
pub enum Error {
    #[error("unsupported instruction: {name}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnsupportedInstruction"))]
    UnsupportedInstruction {
        name: String,
        #[label]
        span: Span,
    },
    #[error("unknown instruction: {name}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnknownInstruction"))]
    UnknownInstruction {
        name: String,
        #[label]
        span: Span,
    },
    #[error("{instruction} must appear inside a SELECT block")]
    #[diagnostic(code("Qdk.Stim.Compiler.InstructionOutsideSelectBlock"))]
    InstructionOutsideSelectBlock {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("{instruction} instruction must start a block")]
    #[diagnostic(code("Qdk.Stim.Compiler.InstructionWithoutBlock"))]
    InstructionWithoutBlock {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("unsupported argument in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnsupportedArgument"))]
    UnsupportedArgument {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("argument for {instruction} cannot be specified in radians")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnexpectedRadians"))]
    UnexpectedRadians {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("missing argument in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.MissingArg"))]
    MissingArg {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("too few arguments for instruction {instruction}; expected {expected}, found {found}")]
    #[diagnostic(code("Qdk.Stim.Compiler.TooFewArgs"))]
    TooFewArgs {
        instruction: String,
        expected: usize,
        found: usize,
        #[label]
        span: Span,
    },
    #[error("too many arguments for instruction {instruction}; expected {expected}, found {found}")]
    #[diagnostic(code("Qdk.Stim.Compiler.TooManyArgs"))]
    TooManyArgs {
        instruction: String,
        expected: usize,
        found: usize,
        #[label]
        span: Span,
    },
    #[error("angle for {instruction} must be finite and representable in radians")]
    #[diagnostic(code("Qdk.Stim.Compiler.InvalidAngle"))]
    InvalidAngle {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("logical observable index must be a non-negative 32-bit integer")]
    #[diagnostic(code("Qdk.Stim.Compiler.InvalidLogicalObservableIndex"))]
    InvalidLogicalObservableIndex {
        #[label]
        span: Span,
    },
    #[error("probability for {instruction} must be between 0 and 1; found {probability}")]
    #[diagnostic(code("Qdk.Stim.Compiler.InvalidProbability"))]
    InvalidProbability {
        instruction: String,
        probability: f64,
        #[label]
        span: Span,
    },
    #[error("probabilities for {instruction} must sum to at most 1.0, but they sum to {total}")]
    #[diagnostic(code("Qdk.Stim.Compiler.InvalidProbabilitySum"))]
    InvalidProbabilitySum {
        instruction: String,
        total: f64,
        #[label]
        span: Span,
    },
    #[error("NOTLEAKED cannot reference a record produced by PEEK_LOSS")]
    #[diagnostic(code("Qdk.Stim.Compiler.NotLeakedOnPeekLoss"))]
    NotLeakedOnPeekLoss {
        #[label]
        span: Span,
    },
    #[error("unsupported target in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnsupportedTarget"))]
    UnsupportedTarget {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("unsupported targets in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnsupportedTargets"))]
    UnsupportedTargets {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("missing target in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.MissingTarget"))]
    MissingTarget {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("target cannot be negated in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.NegatedTarget"))]
    NegatedTarget {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("instruction {instruction} requires an even number of targets")]
    #[diagnostic(code("Qdk.Stim.Compiler.OddTargetCount"))]
    OddTargetCount {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("instruction {instruction} requires a multiple of three targets")]
    #[diagnostic(code("Qdk.Stim.Compiler.TargetCountNotMultipleOfThree"))]
    TargetCountNotMultipleOfThree {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("qubit {qubit} is repeated in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.RepeatedQubit"))]
    RepeatedQubit {
        instruction: String,
        qubit: StimQubitId,
        #[label]
        span: Span,
    },
    #[error("measurement record target in an unsupported position in instruction: {instruction}")]
    #[diagnostic(code("Qdk.Stim.Compiler.MisplacedMeasurementRecord"))]
    MisplacedMeasurementRecord {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error(
        "controlled instruction {instruction} requires a qubit target, but both targets are measurement records"
    )]
    #[diagnostic(code("Qdk.Stim.Compiler.BothTargetsAreMeasurementRecords"))]
    BothTargetsAreMeasurementRecords {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error("measurement record is out of bounds")]
    #[diagnostic(code("Qdk.Stim.Compiler.MeasurementRecordOutOfBounds"))]
    MeasurementRecordOutOfBounds {
        #[label]
        span: Span,
    },
    #[error("all measurement records referenced by {instruction} are out of scope")]
    #[diagnostic(code("Qdk.Stim.Compiler.AllMeasurementRecordsOutOfScope"))]
    AllMeasurementRecordsOutOfScope {
        instruction: String,
        #[label]
        span: Span,
    },
    #[error(
        "else_correlated_error must be preceded by a correlated_error or else_correlated_error instruction"
    )]
    #[diagnostic(code("Qdk.Stim.Compiler.OrphanedElseCorrelatedError"))]
    OrphanedElseCorrelatedError {
        #[label]
        span: Span,
    },
    #[error("a REPEAT count of zero is not supported")]
    #[diagnostic(code("Qdk.Stim.Compiler.ZeroRepeatCount"))]
    ZeroRepeatCount {
        #[label]
        span: Span,
    },
    #[error("Pauli product must be Hermitian")]
    #[diagnostic(code("Qdk.Stim.Compiler.AntiHermitianPauliProduct"))]
    AntiHermitianPauliProduct {
        #[label]
        span: Span,
    },
}
struct Lowerer {
    errors: Vec<Error>,
}

impl Lowerer {
    fn new() -> Self {
        Self { errors: Vec::new() }
    }

    fn lower_circuit(&mut self, circuit: &parser::Circuit) -> Circuit {
        Circuit {
            span: circuit.span,
            items: self.lower_items(&circuit.items),
        }
    }

    fn lower_items(&mut self, items: &Vec<parser::Item>) -> Vec<Item> {
        let mut lowered_items = Vec::new();

        for item in items {
            match item {
                parser::Item::Block(block) => {
                    if let Some(block) = self.lower_block(block) {
                        lowered_items.push(Item::Block(block));
                    }
                }
                parser::Item::Instruction(instruction) => {
                    lowered_items.extend(
                        self.lower_instruction(instruction)
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

        Some(Block::RepeatBlock {
            count: num_repeats,
            body: self.lower_items(items),
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
        Some(Block::SelectBlock {
            body: self.lower_items(items),
        })
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
            // TODO: DEAL WITH CLASSICALLY CONTROLLED
            // TODO: ARE WE DEALING WITH REPEATED QUBITS CORRECTLY?
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
                .collect(), // TODO: SHOULDN'T BE ABLE TO BE SPLIT THROUGH WITH SELECT BLOCK
            "DEPOLARIZE1" => {
                self.broadcast_single_qubit_noise(instruction, SingleQubitNoiseKind::Depolarize)
            }
            "DEPOLARIZE2" => self.broadcast_depolarize2(instruction),
            "HERALDED_ERASE" => {
                self.broadcast_single_qubit_noise(instruction, SingleQubitNoiseKind::HeraldedErase)
            }
            "HERALDED_PAULI_CHANNEL_1" => self.broadcast_heralded_pauli_channel_1(instruction),
            // TODO: Add tests for I_ERROR and II_ERROR probability and target validation.
            "II_ERROR" => {
                let _ = self.validate_probability_list(instruction);
                let _ = self.validate_qubit_pairs(instruction, false);
                Vec::new()
            }
            "I_ERROR" => {
                let _ = self.validate_probability_list(instruction);
                let _ = self.validate_qubit_targets(instruction, false);
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
            "SHIFT_COORDS" => self
                .lower_shift_coords(instruction)
                .into_iter()
                .collect(),
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
        let qubit_targets = self.validate_qubit_targets(instruction, false);
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
        let qubit_targets = self.validate_qubit_targets(instruction, false);
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
        let qubit_target_pairs = self.validate_qubit_pairs(instruction, false);
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
        let qubit_triples = self.validate_qubit_triples(instruction);
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
            match (&pair[0].kind, &pair[1].kind) {
                (parser::TargetKind::Qubit { .. }, parser::TargetKind::Qubit { .. }) => {
                    let Some([(q0, _), (q1, _)]) = self.expect_qubit_pair(instruction, pair, false)
                    else {
                        continue;
                    };
                    instructions.push(Instruction {
                        span: instruction.span,
                        kind: InstructionKind::TwoQubitGate { gate, q0, q1 },
                    });
                }
                (
                    parser::TargetKind::MeasurementRecord { .. },
                    parser::TargetKind::Qubit { .. },
                ) if allowed_rec_position.allows_first() => {
                    let Some(control) = self.expect_measurement_record(instruction, &pair[0])
                    else {
                        continue;
                    };
                    let Some((target, _)) = self.expect_qubit(instruction, &pair[1], false) else {
                        continue;
                    };
                    instructions.push(Instruction {
                        span: instruction.span,
                        kind: InstructionKind::ClassicallyControlledPauli {
                            control,
                            target,
                            pauli: classically_controlled_pauli,
                        },
                    });
                }
                (
                    parser::TargetKind::Qubit { .. },
                    parser::TargetKind::MeasurementRecord { .. },
                ) if allowed_rec_position.allows_second() => {
                    let Some(control) = self.expect_measurement_record(instruction, &pair[1])
                    else {
                        continue;
                    };
                    let Some((target, _)) = self.expect_qubit(instruction, &pair[0], false) else {
                        continue;
                    };
                    instructions.push(Instruction {
                        span: instruction.span,
                        kind: InstructionKind::ClassicallyControlledPauli {
                            control,
                            target,
                            pauli: classically_controlled_pauli,
                        },
                    });
                }
                (
                    parser::TargetKind::MeasurementRecord { .. },
                    parser::TargetKind::MeasurementRecord { .. },
                ) => {
                    self.push_error(Error::BothTargetsAreMeasurementRecords {
                        instruction: instruction.name.clone(),
                        span: Span {
                            lo: pair[0].span.lo,
                            hi: pair[1].span.hi,
                        },
                    });
                }
                // A `rec` that reached here sits on a side this gate doesn't allow
                (parser::TargetKind::MeasurementRecord { .. }, _) => {
                    self.push_error(Error::MisplacedMeasurementRecord {
                        instruction: instruction.name.clone(),
                        span: pair[0].span,
                    });
                }
                (_, parser::TargetKind::MeasurementRecord { .. }) => {
                    self.push_error(Error::MisplacedMeasurementRecord {
                        instruction: instruction.name.clone(),
                        span: pair[1].span,
                    });
                }
                _ => self.push_error(Error::UnsupportedTarget {
                    instruction: instruction.name.clone(),
                    span: pair[0].span,
                }),
            }
        }
        instructions
    }

    fn broadcast_pauli_product_gate(
        &mut self,
        instruction: &parser::Instruction,
        gate: PauliProductGateKind,
    ) -> Vec<Instruction> {
        self.unsupported_args(instruction);
        let pauli_products = self.validate_pauli_products(instruction);
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
        let Some(readout_noise) = self.expect_probability_or_zero(instruction) else {
            return Vec::new();
        };
        let qubit_targets = self.validate_qubit_targets(instruction, true);
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
        let Some(readout_noise) = self.expect_probability_or_zero(instruction) else {
            return Vec::new();
        };
        let qubit_target_pairs = self.validate_qubit_pairs(instruction, true);
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
        let Some(readout_noise) = self.expect_probability_or_zero(instruction) else {
            return Vec::new();
        };
        let pauli_products = self.validate_pauli_products(instruction);
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
        let qubit_targets = self.validate_qubit_targets(instruction, false);
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

    fn broadcast_depolarize2(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let Some(probability) = self.expect_probability(instruction) else {
            return Vec::new();
        };
        let qubit_pairs = self.validate_qubit_pairs(instruction, false);
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
        let Some(probabilities): Option<[Probability; 4]> = self.expect_probabilities(instruction)
        else {
            return Vec::new();
        };
        let qubit_targets = self.validate_qubit_targets(instruction, false);
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
        let Some(probabilities): Option<[Probability; 3]> = self.expect_probabilities(instruction)
        else {
            return Vec::new();
        };
        let qubit_targets = self.validate_qubit_targets(instruction, false);
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
        let Some(probabilities): Option<[Probability; 15]> = self.expect_probabilities(instruction)
        else {
            return Vec::new();
        };
        let qubit_target_pairs = self.validate_qubit_pairs(instruction, false);
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
        let Some(readout_noise) = self.expect_probability_or_zero(instruction) else {
            return Vec::new();
        };
        let qubit_targets = self.validate_qubit_targets(instruction, false);
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
        let records = self.validate_records(instruction);

        Some(Instruction {
            span: instruction.span,
            kind: InstructionKind::Annotation(Annotation::Detector {
                coordinates,
                records,
            }),
        })
    }

    fn broadcast_mpad(&mut self, instruction: &parser::Instruction) -> Vec<Instruction> {
        let Some(readout_noise) = self.expect_probability_or_zero(instruction) else {
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
        instructions
    }

    fn lower_observable_include(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<Instruction> {
        let logical_observable = self.expect_logical_observable_index(instruction)?;
        let targets = self.validate_observable_targets(instruction)?;

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

        let qubit_targets = self.validate_qubit_targets(instruction, false);
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

        let qubit_targets = self.validate_qubit_targets(instruction, false);
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

        let qubit_pairs = self.validate_qubit_pairs(instruction, false);
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
        let Some(angles) = self.expect_angles(instruction, 3) else {
            return Vec::new();
        };

        let qubit_targets = self.validate_qubit_targets(instruction, false);
        qubit_targets
            .into_iter()
            .map(|(qubit, _)| Instruction {
                span: instruction.span,
                kind: InstructionKind::U3 {
                    qubit,
                    theta: angles[0],
                    phi: angles[1],
                    lambda: angles[2],
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

        let pauli_products = self.validate_pauli_products(instruction);
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

        let records = self.validate_negatable_records(instruction);
        if records.is_empty() {
            return None;
        }

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

        let records = self.validate_records(instruction);
        if records.is_empty() {
            return None;
        }

        Some(Instruction {
            span: instruction.span,
            kind: InstructionKind::NotLeaked { records },
        })
    }

    fn validate_qubit_targets(
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

    fn validate_qubit_pairs(
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

    fn validate_qubit_triples(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Vec<[StimQubitId; 3]> {
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

    fn validate_pauli_products(&mut self, instruction: &parser::Instruction) -> Vec<PauliProduct> {
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

    fn validate_records(&mut self, instruction: &parser::Instruction) -> Vec<MeasurementRecord> {
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

    fn validate_negatable_records(
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

    fn validate_observable_targets(
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
        let parser::TargetKind::MeasurementRecord { negated, value } = target.kind else {
            self.push_error(Error::UnsupportedTarget {
                instruction: instruction.name.clone(),
                span: target.span,
            });
            return None;
        };
        Some(NegatableMeasurementRecord {
            record: MeasurementRecord { offset: value },
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
        self.expect_angles(instruction, 1)?.pop()
    }

    fn expect_angles(
        &mut self,
        instruction: &parser::Instruction,
        expected: usize,
    ) -> Option<Vec<Radians>> {
        let args = self.expect_arg_count(instruction, expected)?;
        let mut radians = Vec::with_capacity(args.len());
        let mut has_invalid_angle = false;

        for arg in args {
            let angle_in_radians = match arg.value {
                ArgValue::Default(half_turns) => half_turns * PI,
                ArgValue::Radians(radians) => radians,
            };

            if angle_in_radians.is_finite() {
                radians.push(angle_in_radians);
            } else {
                self.push_error(Error::InvalidAngle {
                    instruction: instruction.name.clone(),
                    span: arg.span,
                });
                has_invalid_angle = true;
            }
        }

        if !has_invalid_angle {
            Some(radians)
        } else {
            None
        }
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
            match arg.value {
                ArgValue::Default(value) => coordinates.push(value),
                ArgValue::Radians(_) => {
                    self.push_error(Error::UnexpectedRadians {
                        instruction: instruction.name.clone(),
                        span: arg.span,
                    });
                    has_invalid_coordinate = true;
                }
            }
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
        let args = self.expect_arg_count(instruction, 1)?;
        let arg = args[0];
        let ArgValue::Default(value) = arg.value else {
            self.push_error(Error::UnexpectedRadians {
                instruction: instruction.name.clone(),
                span: arg.span,
            });
            return None;
        };

        let logical_observable = value as u32;
        if f64::from(logical_observable) != value {
            self.push_error(Error::InvalidLogicalObservableIndex { span: arg.span });
            return None;
        }

        Some(logical_observable)
    }

    fn expect_probability_or_zero(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<Probability> {
        if instruction.args.is_empty() {
            return Some(0.0);
        }
        self.expect_probability(instruction)
    }

    fn expect_probability(&mut self, instruction: &parser::Instruction) -> Option<Probability> {
        let [probability]: [Probability; 1] = self.expect_probabilities(instruction)?;
        Some(probability)
    }

    fn expect_probabilities<const N: usize>(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<[Probability; N]> {
        self.expect_arg_count(instruction, N)?;
        let probabilities = self.validate_probability_list(instruction)?;
        let mut result = [0.0; N];
        result.copy_from_slice(&probabilities);
        Some(result)
    }

    // TODO: does this make sense?
    fn validate_probability_list(
        &mut self,
        instruction: &parser::Instruction,
    ) -> Option<Vec<Probability>> {
        let mut probabilities = Vec::with_capacity(instruction.args.len());
        let mut has_invalid_probability = false;
        for arg in &instruction.args {
            let value = match arg.value {
                ArgValue::Default(value) => value,
                ArgValue::Radians(value) => {
                    self.push_error(Error::UnexpectedRadians {
                        instruction: instruction.name.clone(),
                        span: arg.span,
                    });
                    has_invalid_probability = true;
                    value
                }
            };

            if (0.0..=1.0).contains(&value) {
                probabilities.push(value);
            } else {
                self.push_error(Error::InvalidProbability {
                    instruction: instruction.name.clone(),
                    probability: value,
                    span: arg.span,
                });
                has_invalid_probability = true;
            }
        }
        if has_invalid_probability {
            return None;
        }

        let total: f64 = probabilities.iter().sum();
        if total > 1.0 {
            self.push_error(Error::InvalidProbabilitySum {
                instruction: instruction.name.clone(),
                total,
                span: args_span(&instruction.args),
            });
            return None;
        }

        Some(probabilities)
    }

    fn expect_arg_count(
        &mut self,
        instruction: &parser::Instruction,
        expected: usize,
    ) -> Option<Vec<parser::Arg>> {
        let args = &instruction.args;
        if args.is_empty() {
            self.push_error(Error::MissingArg {
                instruction: instruction.name.clone(),
                span: instruction.span,
            });
            return None;
        }

        if args.len() > expected {
            self.push_error(Error::TooManyArgs {
                instruction: instruction.name.clone(),
                expected,
                found: args.len(),
                span: args_span(&args[expected..]),
            });
            return None;
        } else if args.len() < expected {
            self.push_error(Error::TooFewArgs {
                instruction: instruction.name.clone(),
                expected,
                found: args.len(),
                span: args_span(args),
            });
            return None;
        }
        Some(args.clone())
    }

    fn unsupported(&mut self, instruction: &parser::Instruction) {
        self.push_error(Error::UnsupportedInstruction {
            name: instruction.name.clone(),
            span: instruction.span,
        });
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
