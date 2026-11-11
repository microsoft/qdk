// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Reads `qodec` instruction actions, whose Paulis are text, into the model's
//! [`ActionStep`]s and [`PauliExpr`]s.

use qodec::{Action, Instruction, ParameterKind, PauliString, Scalar};
use qodec_runtime_model::{
    action::{ActionStep, Angle, CliffordMap, ParsedPauli, PauliExpr, StepAction},
    pauli::{Pauli, PauliOp},
};
use std::collections::BTreeMap;

/// The Clifford map of a `clifford` step: the image of each listed generator.
pub fn clifford_map(images: &BTreeMap<PauliString, PauliString>) -> Result<CliffordMap, String> {
    images
        .iter()
        .map(|(generator, image)| {
            let generator = Pauli::parse(&generator.0)?;
            let key = match generator.terms() {
                [(q, PauliOp::X)] if !generator.negative => (*q, true),
                [(q, PauliOp::Z)] if !generator.negative => (*q, false),
                _ => return Err(format!("`{generator}` is not a Clifford generator")),
            };
            Ok((key, Pauli::parse(&image.0)?))
        })
        .collect()
}

/// Reads `text`, a Pauli of `instruction`'s action: the name of one of its
/// `pauli` parameters, or a literal.
pub fn parse_pauli(instruction: &Instruction, text: &PauliString) -> ParsedPauli {
    let is_param = instruction
        .parameters
        .iter()
        .any(|p| p.kind == ParameterKind::Pauli && p.name == text.0);
    if is_param {
        Ok(PauliExpr::Param(text.0.clone()))
    } else {
        Pauli::parse(&text.0).map(PauliExpr::Literal)
    }
}

/// `instruction`'s action, read once. A Pauli or angle that cannot be read
/// stays an error in the step, for whoever interprets it.
#[must_use]
pub fn compile_action(instruction: &Instruction) -> Vec<ActionStep> {
    let parse = |text: &PauliString| parse_pauli(instruction, text);
    instruction
        .action
        .iter()
        .map(|step| ActionStep {
            guarded: step.condition.is_some(),
            action: match &step.action {
                Action::Clifford(images) => StepAction::Clifford {
                    images: images.iter().map(|(g, i)| (parse(g), parse(i))).collect(),
                    map: clifford_map(images),
                },
                Action::Pauli(p) => StepAction::Pauli(parse(p)),
                Action::Rotate { pauli, angle } => StepAction::Rotate {
                    pauli: parse(pauli),
                    angle: match angle {
                        Scalar::Literal(a) => Ok(Angle::Literal(*a)),
                        Scalar::Parameter(name) => instruction
                            .parameters
                            .iter()
                            .position(|p| &p.name == name)
                            .map(Angle::Param)
                            .ok_or_else(|| format!("unknown angle parameter `{name}`")),
                    },
                },
                Action::Observe(observables) => {
                    StepAction::Observe(observables.iter().map(|o| parse(&o.pauli)).collect())
                }
                Action::Stabilize(paulis) => {
                    StepAction::Stabilize(paulis.iter().map(parse).collect())
                }
            },
        })
        .collect()
}
