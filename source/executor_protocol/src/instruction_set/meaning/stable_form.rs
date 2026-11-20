// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A meaning's stable form, with the `serde` feature, as part of its
//! set's: `null` for an undeclared meaning, else its steps in order, each
//! its operation and its guard. Its shape is in
//! `docs/instruction-sets.md`; how it is read back is [`Meaning`]'s to
//! say.

use super::{Angle, Condition, Guard, Meaning, Observable, Operation, Pauli, Step};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One step: what it does, and its guard, if any.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StepForm {
    operation: OperationForm,
    guard: Option<ConditionForm>,
}

/// What a step does, by Qodec's step names.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum OperationForm {
    Stabilize(Vec<Pauli>),
    /// Each generator and its image, as pairs: a generator given twice is
    /// read as given, and refused as a slip.
    Clifford(Vec<(Pauli, Pauli)>),
    Pauli(Pauli),
    Observe(Vec<Observable>),
    Rotate {
        pauli: Pauli,
        angle: Angle,
    },
}

/// A step's guard, a [`Condition`]: Qodec's `if` (an odd parity) or
/// `unless` (an even one), and its terms.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConditionForm {
    If(Vec<Guard>),
    Unless(Vec<Guard>),
}

/// Writes its steps; a meaning with a slip, whose set could not be built,
/// has no stable form and is refused.
impl Serialize for Meaning {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(slip) = self.slips.first() {
            return Err(serde::ser::Error::custom(format_args!(
                "a meaning written with a slip has no stable form: {slip}"
            )));
        }
        let steps: Option<Vec<StepForm>> = self
            .steps
            .as_ref()
            .map(|steps| steps.iter().map(StepForm::from).collect());
        steps.serialize(serializer)
    }
}

/// Builds the meaning its steps write, through the builder.
impl<'de> Deserialize<'de> for Meaning {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let Some(steps) = Option::<Vec<StepForm>>::deserialize(deserializer)? else {
            return Ok(Meaning::undeclared());
        };
        let mut meaning = Meaning::declared();
        for StepForm { operation, guard } in steps {
            meaning = match operation {
                OperationForm::Stabilize(generators) => meaning.stabilize(generators),
                OperationForm::Clifford(images) => meaning.clifford(images),
                OperationForm::Pauli(pauli) => meaning.pauli(pauli),
                OperationForm::Observe(observables) => meaning.observe(observables),
                OperationForm::Rotate { pauli, angle } => meaning.rotate(pauli, angle),
            };
            meaning = match guard {
                None => meaning,
                Some(ConditionForm::If(terms)) => meaning.only_if(terms),
                Some(ConditionForm::Unless(terms)) => meaning.unless(terms),
            };
        }
        Ok(meaning)
    }
}

impl From<&Step> for StepForm {
    fn from(Step { operation, guard }: &Step) -> Self {
        let operation = match operation {
            Operation::Stabilize(generators) => OperationForm::Stabilize(generators.clone()),
            Operation::Clifford(images) => OperationForm::Clifford(
                images
                    .iter()
                    .map(|(generator, image)| (generator.clone(), image.clone()))
                    .collect(),
            ),
            Operation::Pauli(pauli) => OperationForm::Pauli(pauli.clone()),
            Operation::Observe(observables) => OperationForm::Observe(observables.clone()),
            Operation::Rotate { pauli, angle } => OperationForm::Rotate {
                pauli: pauli.clone(),
                angle: *angle,
            },
        };
        let guard = guard.as_ref().map(|Condition { odd, terms }| {
            if *odd {
                ConditionForm::If(terms.clone())
            } else {
                ConditionForm::Unless(terms.clone())
            }
        });
        Self { operation, guard }
    }
}
