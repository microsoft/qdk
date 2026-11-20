// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A meaning's stable form, with the `serde` feature: what an instruction
//! does, written to cross a process boundary with its set. `null` for an
//! undeclared meaning, else its steps in order, each its operation and
//! its guard, in Qodec's words: `{"operation": {"pauli": "X_0"}, "guard":
//! {"if": [{"parameter": 0}, {"outcome": 0}]}}`.
//!
//! It is read back through the builder, step by step, as a loader of
//! Qodec's YAML would build it, so a decoded meaning is canonical, and
//! the slips of a form written by hand (a Clifford mapping a generator
//! twice) are refused when its set is built, as a built one's are.

use super::{Angle, Condition, Guard, Meaning, Observable, Operation, Pauli, Step};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// One step: what it does, and its guard, if any.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StepForm {
    operation: OperationForm,
    guard: Option<GuardForm>,
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

/// A step's guard: Qodec's `if` (an odd parity) or `unless` (an even one).
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GuardForm {
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
                Some(GuardForm::If(terms)) => meaning.only_if(terms),
                Some(GuardForm::Unless(terms)) => meaning.unless(terms),
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
                GuardForm::If(terms.clone())
            } else {
                GuardForm::Unless(terms.clone())
            }
        });
        Self { operation, guard }
    }
}
