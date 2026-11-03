// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Standard gate identity and its mapping onto the resource estimator's
//! instruction identifier space.

use serde::de::{Error as DeError, Unexpected};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A gate the store encodes inline, with no heap allocation.
///
/// The discriminant is the store's own compact identity. It is deliberately
/// separate from the estimator's identifier so that the store controls its own
/// layout, with [`StandardGate::instruction_id`] providing the total mapping
/// between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum StandardGate {
    I,
    X,
    Y,
    Z,
    H,
    S,
    SDag,
    SqrtX,
    SqrtXDag,
    T,
    TDag,
    Cx,
    Cy,
    Cz,
    Swap,
    Ccx,
    Cswap,
    Rx,
    Ry,
    Rz,
    Rxx,
    Ryy,
    Rzz,
    Crx,
    Cry,
    Crz,
    MeasureX,
    MeasureY,
    MeasureZ,
    PrepareX,
    PrepareY,
    PrepareZ,
}

/// Number of variants in [`StandardGate`], for sizing static tables.
pub const STANDARD_GATE_COUNT: usize = 32;

/// Estimator instruction identifier per standard gate, indexed by discriminant.
const INSTRUCTION_IDS: [u16; STANDARD_GATE_COUNT] = [
    0,    // I
    1,    // X
    2,    // Y
    3,    // Z
    16,   // H
    23,   // S
    24,   // SDag
    19,   // SqrtX
    20,   // SqrtXDag
    1028, // T
    1029, // TDag
    25,   // Cx
    26,   // Cy
    27,   // Cz
    28,   // Swap
    1030, // Ccx
    1033, // Cswap
    1036, // Rx
    1037, // Ry
    1038, // Rz
    1042, // Rxx
    1043, // Ryy
    1044, // Rzz
    1039, // Crx
    1040, // Cry
    1041, // Crz
    256,  // MeasureX
    257,  // MeasureY
    258,  // MeasureZ
    48,   // PrepareX
    49,   // PrepareY
    50,   // PrepareZ
];

/// Qubit count per standard gate, indexed by discriminant.
const QUBIT_ARITY: [u8; STANDARD_GATE_COUNT] = [
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // I through TDag
    2, 2, 2, 2, // Cx, Cy, Cz, Swap
    3, 3, // Ccx, Cswap
    1, 1, 1, // Rx, Ry, Rz
    2, 2, 2, // Rxx, Ryy, Rzz
    2, 2, 2, // Crx, Cry, Crz
    1, 1, 1, // measurements
    1, 1, 1, // preparations
];

/// Angle-parameter count per standard gate, indexed by discriminant.
const PARAM_ARITY: [u8; STANDARD_GATE_COUNT] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // I through TDag
    0, 0, 0, 0, // Cx, Cy, Cz, Swap
    0, 0, // Ccx, Cswap
    1, 1, 1, // Rx, Ry, Rz
    1, 1, 1, // Rxx, Ryy, Rzz
    1, 1, 1, // Crx, Cry, Crz
    0, 0, 0, // measurements
    0, 0, 0, // preparations
];

/// Lower-case name per standard gate, indexed by discriminant.
const NAMES: [&str; STANDARD_GATE_COUNT] = [
    "i",
    "x",
    "y",
    "z",
    "h",
    "s",
    "sdg",
    "sx",
    "sxdg",
    "t",
    "tdg",
    "cx",
    "cy",
    "cz",
    "swap",
    "ccx",
    "cswap",
    "rx",
    "ry",
    "rz",
    "rxx",
    "ryy",
    "rzz",
    "crx",
    "cry",
    "crz",
    "measure_x",
    "measure_y",
    "measure_z",
    "prepare_x",
    "prepare_y",
    "prepare_z",
];

impl StandardGate {
    /// Estimator instruction identifier for this gate.
    ///
    /// Total in this direction by construction. The reverse direction is
    /// partial, because the estimator space contains instructions the circuit
    /// surface does not express.
    #[must_use]
    pub const fn instruction_id(self) -> u16 {
        INSTRUCTION_IDS[self as usize]
    }

    /// Number of qubits this gate acts on.
    #[must_use]
    pub const fn qubit_arity(self) -> u8 {
        QUBIT_ARITY[self as usize]
    }

    /// Number of angle parameters this gate takes.
    #[must_use]
    pub const fn param_arity(self) -> u8 {
        PARAM_ARITY[self as usize]
    }

    /// Lower-case name for this gate.
    #[must_use]
    pub const fn name(self) -> &'static str {
        NAMES[self as usize]
    }

    /// Every standard gate, in discriminant order.
    #[must_use]
    pub const fn all() -> [Self; STANDARD_GATE_COUNT] {
        use StandardGate::{
            Ccx, Crx, Cry, Crz, Cswap, Cx, Cy, Cz, H, I, MeasureX, MeasureY, MeasureZ, PrepareX,
            PrepareY, PrepareZ, Rx, Rxx, Ry, Ryy, Rz, Rzz, S, SDag, SqrtX, SqrtXDag, Swap, T, TDag,
            X, Y, Z,
        };
        [
            I, X, Y, Z, H, S, SDag, SqrtX, SqrtXDag, T, TDag, Cx, Cy, Cz, Swap, Ccx, Cswap, Rx, Ry,
            Rz, Rxx, Ryy, Rzz, Crx, Cry, Crz, MeasureX, MeasureY, MeasureZ, PrepareX, PrepareY,
            PrepareZ,
        ]
    }
}

/// Serialized by name rather than by discriminant, so that reordering the enum
/// does not silently change the meaning of a stored circuit. This also keeps
/// one gate vocabulary shared between the API and the stored form.
impl Serialize for StandardGate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.name())
    }
}

impl<'de> Deserialize<'de> for StandardGate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Self::all()
            .into_iter()
            .find(|gate| gate.name() == name)
            .ok_or_else(|| D::Error::invalid_value(Unexpected::Str(&name), &"a standard gate name"))
    }
}
