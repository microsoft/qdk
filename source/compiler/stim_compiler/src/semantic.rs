// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::parser::*;
use Pauli::{X, Y, Z};
use qsc_data_structures::{
    display::{
        write_field, write_list_field, writeln_field, writeln_header,
        writeln_header_with_span, writeln_list_field,
    },
    span::Span,
};
use std::fmt::{self, Display, Formatter};
use thiserror::Error;

pub type StimQubitId = u32;
pub type Radians = f64;
pub type Probability = f64;

pub struct MeasurementRecord {
    pub span: Span,
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
    Instruction(Instruction),
    RepeatBlock {
        span: Span,
        count: u32,
        body: Vec<Item>,
    },
    SelectBlock {
        span: Span,
        body: Vec<Item>,
    },
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
    PairMeasurement {
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
        qubit: StimQubitId,
        readout_noise: Probability,
    },
    SelectControl(SelectControl),
    Annotation(Annotation),
    SingleRotation {
        qubit: StimQubitId,
        axis: Pauli,
        angle: Radians,
    },
    PairRotation {
        q0: StimQubitId,
        q1: StimQubitId,
        axis: PauliPair,
        angle: Radians,
    },
    U3 {
        qubit: StimQubitId,
        theta: Radians,
        phi: Radians,
        lambda: Radians,
    },
    PauliProductRotation {
        product: PauliProduct,
        angle: Radians,
    },
}

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

pub enum ThreeQubitGateKind {
    CCZ,
    CCX,
}

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

pub enum PauliProductGateKind {
    S,
    S_DAG,
    T,
    T_DAG,
}

pub enum Noise {
    Correlated {
        branches: Vec<CorrelatedError>,
    },
    Depolarize1 {
        probability: Probability,
        qubit: StimQubitId,
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
    SingleQubitError {
        kind: FaultKind,
        probability: Probability,
        qubit: StimQubitId,
    },
}

pub enum FaultKind {
    X,
    Y,
    Z,
    Loss,
}

pub struct Fault {
    pub kind: FaultKind,
    pub qubit: StimQubitId,
}

pub struct CorrelatedError {
    pub span: Span,
    pub probability: Probability,
    pub faults: Vec<Fault>,
}

pub enum SelectControl {
    Require {
        records: Vec<NegatableMeasurementRecord>,
    },
    NotLeaked {
        records: Vec<MeasurementRecord>,
    },
}

pub enum Annotation {
    Detector {
        coordinates: Vec<f64>,
        records: Vec<MeasurementRecord>,
    },
    MeasurementPadding {
        readout_noise: Probability,
        values: Vec<bool>,
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
