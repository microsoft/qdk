// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Reads `qodec` instruction actions, whose Paulis are text, into the model's
//! [`ActionStep`]s and [`PauliExpr`]s. An action that cannot be read is an
//! error of the qodec, found when it is read rather than when a call needs it.

use qodec::{Action, Instruction, ParameterKind, PauliString, Scalar};
use qodec_runtime_model::{
    action::{ActionStep, Angle, CliffordMap, PauliExpr, StepAction},
    pauli::{Pauli, PauliOp},
};
use std::collections::BTreeMap;

/// The Clifford map of a `clifford` step: the image of each listed generator.
/// Generators and images are literal Paulis: the spec's tableau maps input
/// generators to their images, and names no parameter.
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
pub fn parse_pauli(instruction: &Instruction, text: &PauliString) -> Result<PauliExpr, String> {
    if is_pauli_param(instruction, &text.0) {
        Ok(PauliExpr::Param(text.0.clone()))
    } else {
        Pauli::parse(&text.0).map(PauliExpr::Literal)
    }
}

fn is_pauli_param(instruction: &Instruction, name: &str) -> bool {
    instruction
        .parameters
        .iter()
        .any(|p| p.kind == ParameterKind::Pauli && p.name == name)
}

/// `instruction`'s action, read once. Fails, naming the step, if a Pauli, a
/// Clifford tableau or an angle cannot be read. A rotation angle must be a
/// literal, or name a `number` or `integer` parameter, as the spec's
/// [validation rules](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/validation.md#instruction-sets)
/// require.
pub fn compile_action(instruction: &Instruction) -> Result<Vec<ActionStep>, String> {
    let parse = |text: &PauliString| parse_pauli(instruction, text);
    let parse_all = |paulis: &mut dyn Iterator<Item = &PauliString>| {
        paulis.map(parse).collect::<Result<Vec<_>, _>>()
    };
    instruction
        .action
        .iter()
        .enumerate()
        .map(|(k, step)| {
            let action = match &step.action {
                Action::Clifford(images) => match images
                    .iter()
                    .flat_map(|(g, i)| [g, i])
                    .find(|p| is_pauli_param(instruction, &p.0))
                {
                    Some(name) => Err(format!(
                        "a `clifford` step maps literal Paulis, not the parameter `{}`",
                        name.0
                    )),
                    None => clifford_map(images).map(StepAction::Clifford),
                },
                Action::Pauli(p) => parse(p).map(StepAction::Pauli),
                Action::Rotate { pauli, angle } => {
                    let angle = match angle {
                        Scalar::Literal(a) => Ok(Angle::Literal(*a)),
                        Scalar::Parameter(name) => instruction
                            .parameters
                            .iter()
                            .position(|p| {
                                &p.name == name
                                    && matches!(
                                        p.kind,
                                        ParameterKind::Number | ParameterKind::Integer
                                    )
                            })
                            .map(Angle::Param)
                            .ok_or_else(|| {
                                format!(
                                    "the angle names no `number` or `integer` parameter `{name}`"
                                )
                            }),
                    };
                    angle.and_then(|angle| {
                        Ok(StepAction::Rotate {
                            pauli: parse(pauli)?,
                            angle,
                        })
                    })
                }
                Action::Observe(observables) => {
                    parse_all(&mut observables.iter().map(|o| &o.pauli)).map(StepAction::Observe)
                }
                Action::Stabilize(paulis) => {
                    parse_all(&mut paulis.iter()).map(StepAction::Stabilize)
                }
            };
            action
                .map(|action| ActionStep {
                    guarded: step.condition.is_some(),
                    action,
                })
                .map_err(|e| format!("`{}`, action step {k}: {e}", instruction.mnemonic))
        })
        .collect()
}
