// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::super::{
    constants::{
        INSTRUCTION_SET, ONE_QUBIT_GATE_ERROR_RATE, ONE_QUBIT_GATE_TIME,
        ONE_QUBIT_MEASUREMENT_ERROR_RATE, ONE_QUBIT_MEASUREMENT_TIME, T_GATE_ERROR_RATE,
    },
    serialization::{f64_nan, time},
};
use serde::{Deserialize, Serialize, de::Error};

/// Physical qubit classification.
///
/// The physical qubit can be either `gate_based` or `Majorana` (use these
/// values in the serialized file formats).
#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum PhysicalInstructionSet {
    GateBased,
    Majorana,
}

/// This checks whether in a list of required fields some of them are not
/// specified and then generates an error with all undefined fields.
///
/// The first element in each tuple is `true` if and only if the field is not
/// specified.
fn check_required_fields(fields: &[(bool, &str)]) -> Result<(), serde_json::error::Error> {
    let missing_fields: Vec<_> = fields
        .iter()
        .filter(|(is_none, _)| *is_none)
        .map(|(_, name)| format!("`{name}`"))
        .collect();

    if missing_fields.is_empty() {
        Ok(())
    } else {
        let joined = missing_fields.join(", ");
        Err(serde::de::Error::custom(format!("missing fields {joined}")))
    }
}

/// Physical qubit model.
///
/// This struct models a physical qubit.
///
/// # Qubit types
///
/// We can model two different qubit types.  These come with different fields;
/// some of them require values to be specified, while some others can be
/// derived.  See `input_params.md` file in docs folder for more details.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(
    tag = "instructionSet",
    try_from = "serde_json::Map<String, serde_json::Value>"
)]
pub enum PhysicalQubit {
    GateBased(GateBasedPhysicalQubit),
    Majorana(MajoranaQubit),
}

impl PhysicalQubit {
    #[must_use]
    pub fn qubit_gate_ns_e3() -> Self {
        Self::GateBased(GateBasedPhysicalQubit::qubit_gate_ns_e3())
    }

    #[must_use]
    pub fn qubit_gate_ns_e4() -> Self {
        Self::GateBased(GateBasedPhysicalQubit::qubit_gate_ns_e4())
    }

    #[must_use]
    pub fn qubit_gate_us_e3() -> Self {
        Self::GateBased(GateBasedPhysicalQubit::qubit_gate_us_e3())
    }

    #[must_use]
    pub fn qubit_gate_us_e4() -> Self {
        Self::GateBased(GateBasedPhysicalQubit::qubit_gate_us_e4())
    }

    #[must_use]
    pub fn qubit_maj_ns_e4() -> Self {
        Self::Majorana(MajoranaQubit::qubit_maj_ns_e4())
    }

    #[must_use]
    pub fn qubit_maj_ns_e6() -> Self {
        Self::Majorana(MajoranaQubit::qubit_maj_ns_e6())
    }

    #[must_use]
    pub fn instruction_set(&self) -> super::PhysicalInstructionSet {
        match self {
            Self::GateBased(_) => super::PhysicalInstructionSet::GateBased,
            Self::Majorana(_) => super::PhysicalInstructionSet::Majorana,
        }
    }

    #[must_use]
    pub fn t_gate_time(&self) -> u64 {
        match self {
            Self::GateBased(gate_based) => {
                gate_based.t_gate_time.expect("T gate time should be set")
            }
            Self::Majorana(majorana) => majorana.t_gate_time.expect("T gate time should be set"),
        }
    }

    #[must_use]
    pub fn t_gate_error_rate(&self) -> f64 {
        match self {
            Self::GateBased(gate_based) => gate_based.t_gate_error_rate,
            Self::Majorana(majorana) => majorana.t_gate_error_rate,
        }
    }

    #[must_use]
    pub fn one_qubit_measurement_time(&self) -> u64 {
        match self {
            Self::GateBased(gate_based) => gate_based
                .one_qubit_measurement_time
                .expect("measurement time should be set"),
            Self::Majorana(majorana) => majorana
                .one_qubit_measurement_time
                .expect("measurement time should"),
        }
    }

    #[must_use]
    pub fn clifford_error_rate(&self) -> f64 {
        match self {
            Self::GateBased(gate_based) => gate_based
                .one_qubit_gate_error_rate
                .max(gate_based.two_qubit_gate_error_rate)
                .max(gate_based.idle_error_rate),
            Self::Majorana(majorana) => majorana
                .idle_error_rate
                .max(majorana.one_qubit_measurement_error_rate.process())
                .max(majorana.two_qubit_joint_measurement_error_rate.process()),
        }
    }

    #[must_use]
    pub fn readout_error_rate(&self) -> f64 {
        match self {
            Self::GateBased(gate_based) => gate_based.one_qubit_measurement_error_rate,
            Self::Majorana(majorana) => majorana
                .one_qubit_measurement_error_rate
                .readout()
                .max(majorana.two_qubit_joint_measurement_error_rate.readout()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GateBasedPhysicalQubit {
    #[serde(default)]
    pub name: String,
    #[serde(default, with = "time")]
    pub one_qubit_measurement_time: Option<u64>,
    #[serde(default, with = "time")]
    pub one_qubit_gate_time: Option<u64>,
    #[serde(default, with = "time")]
    pub two_qubit_gate_time: Option<u64>,
    #[serde(default, with = "time")]
    pub t_gate_time: Option<u64>,
    #[serde(default = "f64_nan", deserialize_with = "deserialize_error_rate")]
    pub one_qubit_measurement_error_rate: f64,
    #[serde(default = "f64_nan", deserialize_with = "deserialize_error_rate")]
    pub one_qubit_gate_error_rate: f64,
    #[serde(default = "f64_nan", deserialize_with = "deserialize_error_rate")]
    pub two_qubit_gate_error_rate: f64,
    #[serde(default = "f64_nan", deserialize_with = "deserialize_error_rate")]
    pub t_gate_error_rate: f64,
    #[serde(default = "f64_nan", deserialize_with = "deserialize_error_rate")]
    pub idle_error_rate: f64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MajoranaQubit {
    #[serde(default)]
    pub name: String,
    #[serde(default, with = "time")]
    pub one_qubit_measurement_time: Option<u64>,
    #[serde(default, with = "time")]
    pub two_qubit_joint_measurement_time: Option<u64>,
    #[serde(default, with = "time")]
    pub t_gate_time: Option<u64>,
    #[serde(default)]
    pub one_qubit_measurement_error_rate: MeasurementErrorRate,
    #[serde(default)]
    pub two_qubit_joint_measurement_error_rate: MeasurementErrorRate,
    #[serde(default = "f64_nan", deserialize_with = "deserialize_error_rate")]
    pub t_gate_error_rate: f64,
    #[serde(default = "f64_nan", deserialize_with = "deserialize_error_rate")]
    pub idle_error_rate: f64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(untagged)]
pub enum MeasurementErrorRate {
    Simple(#[serde(deserialize_with = "deserialize_error_rate")] f64),
    Detailed {
        #[serde(deserialize_with = "deserialize_error_rate")]
        process: f64,
        #[serde(deserialize_with = "deserialize_error_rate")]
        readout: f64,
    },
}

impl Default for PhysicalQubit {
    fn default() -> Self {
        Self::GateBased(GateBasedPhysicalQubit::default())
    }
}

impl TryFrom<serde_json::Map<String, serde_json::Value>> for PhysicalQubit {
    type Error = serde_json::Error;

    fn try_from(mut map: serde_json::Map<String, serde_json::Value>) -> Result<Self, Self::Error> {
        use serde_json::Value;

        // check if there is an instruction set
        if let Some(Value::String(instruction_set)) = map.remove(INSTRUCTION_SET) {
            match instruction_set.as_str() {
                "gate_based" | "gateBased" | "gate-based" | "GateBased" => {
                    let qubit: GateBasedPhysicalQubit = serde_json::from_value(Value::Object(map))?;
                    Ok(Self::GateBased(qubit.normalized()?))
                }
                "Majorana" | "majorana" => {
                    let qubit: MajoranaQubit = serde_json::from_value(Value::Object(map))?;
                    Ok(Self::Majorana(qubit.normalized()?))
                }
                _ => Err(serde_json::Error::invalid_value(
                    serde::de::Unexpected::Str(&instruction_set),
                    &"expected \"GateBased\" or \"Majorana\" as value for instructionSet",
                )),
            }
        } else if let Some(Value::String(name)) = map.get("name") {
            match name.as_str() {
                "qubit_gate_ns_e3" | "qubit_gate_ns_e4" | "qubit_gate_us_e3"
                | "qubit_gate_us_e4" => {
                    let qubit: GateBasedPhysicalQubit = serde_json::from_value(Value::Object(map))?;
                    Ok(Self::GateBased(qubit.normalized()?))
                }
                "qubit_maj_ns_e4" | "qubit_maj_ns_e6" => {
                    let qubit: MajoranaQubit = serde_json::from_value(Value::Object(map))?;
                    Ok(Self::Majorana(qubit.normalized()?))
                }
                _ => Err(serde_json::Error::missing_field(INSTRUCTION_SET)),
            }
        } else {
            Err(serde_json::Error::missing_field("name or instructionSet"))
        }
    }
}

impl GateBasedPhysicalQubit {
    fn from_default_name(name: &str) -> Option<Self> {
        match name {
            "qubit_gate_ns_e3" => Some(Self::qubit_gate_ns_e3()),
            "qubit_gate_ns_e4" => Some(Self::qubit_gate_ns_e4()),
            "qubit_gate_us_e3" => Some(Self::qubit_gate_us_e3()),
            "qubit_gate_us_e4" => Some(Self::qubit_gate_us_e4()),
            _ => None,
        }
    }

    #[must_use]
    pub fn qubit_gate_ns_e3() -> Self {
        Self {
            name: "qubit_gate_ns_e3".into(),
            one_qubit_measurement_time: Some(100),
            one_qubit_gate_time: Some(50),
            two_qubit_gate_time: Some(50),
            t_gate_time: Some(50),
            one_qubit_measurement_error_rate: 1e-3,
            one_qubit_gate_error_rate: 1e-3,
            two_qubit_gate_error_rate: 1e-3,
            t_gate_error_rate: 1e-3,
            idle_error_rate: 1e-3,
        }
    }

    #[must_use]
    pub fn qubit_gate_ns_e4() -> Self {
        Self {
            name: "qubit_gate_ns_e4".into(),
            one_qubit_measurement_time: Some(100),
            one_qubit_gate_time: Some(50),
            two_qubit_gate_time: Some(50),
            t_gate_time: Some(50),
            one_qubit_measurement_error_rate: 1e-4,
            one_qubit_gate_error_rate: 1e-4,
            two_qubit_gate_error_rate: 1e-4,
            t_gate_error_rate: 1e-4,
            idle_error_rate: 1e-4,
        }
    }

    #[must_use]
    pub fn qubit_gate_us_e3() -> Self {
        Self {
            name: "qubit_gate_us_e3".into(),
            one_qubit_measurement_time: Some(100_000),
            one_qubit_gate_time: Some(100_000),
            two_qubit_gate_time: Some(100_000),
            t_gate_time: Some(100_000),
            one_qubit_measurement_error_rate: 1e-3,
            one_qubit_gate_error_rate: 1e-3,
            two_qubit_gate_error_rate: 1e-3,
            t_gate_error_rate: 1e-6,
            idle_error_rate: 1e-3,
        }
    }

    #[must_use]
    pub fn qubit_gate_us_e4() -> Self {
        Self {
            name: "qubit_gate_us_e4".into(),
            one_qubit_measurement_time: Some(100_000),
            one_qubit_gate_time: Some(100_000),
            two_qubit_gate_time: Some(100_000),
            t_gate_time: Some(100_000),
            one_qubit_measurement_error_rate: 1e-4,
            one_qubit_gate_error_rate: 1e-4,
            two_qubit_gate_error_rate: 1e-4,
            t_gate_error_rate: 1e-6,
            idle_error_rate: 1e-4,
        }
    }

    fn normalized(mut self) -> Result<Self, serde_json::error::Error> {
        if let Some(default_model) = Self::from_default_name(&self.name) {
            self.overwrite_from(&default_model);
        }

        // at this point we can assume that all values have been
        // pre-assigned in case of pre-defined models
        check_required_fields(&[
            (
                self.one_qubit_measurement_time.is_none(),
                ONE_QUBIT_MEASUREMENT_TIME,
            ),
            (self.t_gate_error_rate.is_nan(), T_GATE_ERROR_RATE),
            (
                self.one_qubit_measurement_error_rate.is_nan(),
                ONE_QUBIT_MEASUREMENT_ERROR_RATE,
            ),
            (self.one_qubit_gate_time.is_none(), ONE_QUBIT_GATE_TIME),
            (
                self.one_qubit_gate_error_rate.is_nan(),
                ONE_QUBIT_GATE_ERROR_RATE,
            ),
        ])?;

        // at this point we can assume that all required fields have been assigned
        update_if_none(&mut self.two_qubit_gate_time, self.one_qubit_gate_time);
        update_if_none(&mut self.t_gate_time, self.one_qubit_gate_time);
        update_if_nan(
            &mut self.two_qubit_gate_error_rate,
            self.one_qubit_gate_error_rate,
        );
        update_if_nan(&mut self.t_gate_error_rate, self.one_qubit_gate_error_rate);
        update_if_nan(
            &mut self.idle_error_rate,
            self.one_qubit_measurement_error_rate,
        );

        Ok(self)
    }

    fn overwrite_from(&mut self, base: &Self) {
        update_if_none(
            &mut self.one_qubit_measurement_time,
            base.one_qubit_measurement_time,
        );
        update_if_none(&mut self.one_qubit_gate_time, base.one_qubit_gate_time);
        update_if_none(&mut self.two_qubit_gate_time, base.two_qubit_gate_time);
        update_if_none(&mut self.t_gate_time, base.t_gate_time);
        update_if_nan(
            &mut self.one_qubit_measurement_error_rate,
            base.one_qubit_measurement_error_rate,
        );
        update_if_nan(
            &mut self.one_qubit_gate_error_rate,
            base.one_qubit_gate_error_rate,
        );
        update_if_nan(
            &mut self.two_qubit_gate_error_rate,
            base.two_qubit_gate_error_rate,
        );
        update_if_nan(&mut self.t_gate_error_rate, base.t_gate_error_rate);
        update_if_nan(&mut self.idle_error_rate, base.idle_error_rate);
    }
}

impl Default for GateBasedPhysicalQubit {
    fn default() -> Self {
        Self::qubit_gate_ns_e3()
    }
}

impl MajoranaQubit {
    fn from_default_name(name: &str) -> Option<Self> {
        match name {
            "qubit_maj_ns_e4" => Some(Self::qubit_maj_ns_e4()),
            "qubit_maj_ns_e6" => Some(Self::qubit_maj_ns_e6()),
            _ => None,
        }
    }

    #[must_use]
    pub fn qubit_maj_ns_e4() -> Self {
        Self {
            name: "qubit_maj_ns_e4".into(),
            one_qubit_measurement_time: Some(100),
            two_qubit_joint_measurement_time: Some(100),
            t_gate_time: Some(100),
            one_qubit_measurement_error_rate: MeasurementErrorRate::Detailed {
                process: 1e-4,
                readout: 1e-4,
            },
            two_qubit_joint_measurement_error_rate: MeasurementErrorRate::Detailed {
                process: 1e-4,
                readout: 1e-4,
            },
            t_gate_error_rate: 0.05,
            idle_error_rate: 1e-4,
        }
    }

    #[must_use]
    pub fn qubit_maj_ns_e6() -> Self {
        Self {
            name: "qubit_maj_ns_e6".into(),
            one_qubit_measurement_time: Some(100),
            two_qubit_joint_measurement_time: Some(100),
            t_gate_time: Some(100),
            one_qubit_measurement_error_rate: MeasurementErrorRate::Detailed {
                process: 1e-6,
                readout: 1e-6,
            },
            two_qubit_joint_measurement_error_rate: MeasurementErrorRate::Detailed {
                process: 1e-6,
                readout: 1e-6,
            },
            t_gate_error_rate: 0.01,
            idle_error_rate: 1e-6,
        }
    }

    fn normalized(mut self) -> Result<Self, serde_json::error::Error> {
        if let Some(default_model) = Self::from_default_name(&self.name) {
            self.overwrite_from(&default_model);
        }

        // at this point we can assume that all values have been
        // pre-assigned in case of pre-defined models
        check_required_fields(&[
            (
                self.one_qubit_measurement_time.is_none(),
                ONE_QUBIT_MEASUREMENT_TIME,
            ),
            (
                self.one_qubit_measurement_error_rate.is_nan(),
                ONE_QUBIT_MEASUREMENT_ERROR_RATE,
            ),
            (self.t_gate_error_rate.is_nan(), T_GATE_ERROR_RATE),
        ])?;

        // normalize error rates
        let (_, readout_one) = self.one_qubit_measurement_error_rate.normalize();
        let (process_two, readout_two) = self.two_qubit_joint_measurement_error_rate.normalize();

        // at this point we can assume that all required fields have been assigned
        update_if_none(
            &mut self.two_qubit_joint_measurement_time,
            self.one_qubit_measurement_time,
        );
        update_if_none(&mut self.t_gate_time, self.one_qubit_measurement_time);
        update_if_nan(process_two, *readout_one);
        update_if_nan(readout_two, *readout_one);
        update_if_nan(&mut self.idle_error_rate, *readout_one);

        Ok(self)
    }

    fn overwrite_from(&mut self, base: &Self) {
        update_if_none(
            &mut self.one_qubit_measurement_time,
            base.one_qubit_measurement_time,
        );
        update_if_none(
            &mut self.one_qubit_measurement_time,
            base.one_qubit_measurement_time,
        );
        update_if_none(
            &mut self.two_qubit_joint_measurement_time,
            base.two_qubit_joint_measurement_time,
        );
        update_if_none(&mut self.t_gate_time, base.t_gate_time);

        if self.one_qubit_measurement_error_rate.is_nan() {
            self.one_qubit_measurement_error_rate = base.one_qubit_measurement_error_rate;
        }
        if self.two_qubit_joint_measurement_error_rate.is_nan() {
            self.two_qubit_joint_measurement_error_rate =
                base.two_qubit_joint_measurement_error_rate;
        }

        update_if_nan(&mut self.t_gate_error_rate, base.t_gate_error_rate);
        update_if_nan(&mut self.idle_error_rate, base.idle_error_rate);
    }
}

impl Default for MajoranaQubit {
    fn default() -> Self {
        Self::qubit_maj_ns_e4()
    }
}

fn deserialize_error_rate<'de, D>(deserializer: D) -> Result<f64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value: f64 = Deserialize::deserialize(deserializer)?;

    if value <= 0.0 || value >= 1.0 {
        Err(D::Error::custom("expected value between 0.0 and 1.0"))
    } else {
        Ok(value)
    }
}

impl MeasurementErrorRate {
    /// Normalizes the measurement error rate and returns the values for
    /// process and readout
    pub fn normalize(&mut self) -> (&mut f64, &mut f64) {
        *self = match self {
            Self::Simple(v) => Self::Detailed {
                process: *v,
                readout: *v,
            },
            Self::Detailed { process, readout } => Self::Detailed {
                process: *process,
                readout: *readout,
            },
        };

        if let Self::Detailed { process, readout } = self {
            (process, readout)
        } else {
            unreachable!()
        }
    }

    pub fn is_nan(&self) -> bool {
        match self {
            Self::Simple(v) => v.is_nan(),
            Self::Detailed { process, readout } => process.is_nan() && readout.is_nan(),
        }
    }

    pub fn process(&self) -> f64 {
        match *self {
            Self::Simple(v) => v,
            Self::Detailed { process, .. } => process,
        }
    }

    pub fn readout(&self) -> f64 {
        match *self {
            Self::Simple(v) => v,
            Self::Detailed { readout, .. } => readout,
        }
    }
}

impl Default for MeasurementErrorRate {
    fn default() -> Self {
        Self::Simple(f64::NAN)
    }
}

#[inline]
fn update_if_nan(value: &mut f64, base_value: f64) {
    if value.is_nan() {
        *value = base_value;
    }
}

#[inline]
fn update_if_none(value: &mut Option<u64>, base_value: Option<u64>) {
    if value.is_none() {
        *value = base_value;
    }
}
