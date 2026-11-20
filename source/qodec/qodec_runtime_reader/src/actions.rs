// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Reads `qodec` instruction actions, whose Paulis are text, into the model's
//! [`ActionStep`]s and [`PauliExpr`]s. An action that cannot be read is an
//! error of the qodec, found when it is read rather than when a call needs it.

use qodec::{Action, Instruction, ParameterKind, PauliString, Scalar};
use qodec_runtime_model::{
    action::{ActionStep, Angle, CliffordMap, Condition, PauliExpr, Predicate, StepAction},
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

/// The condition of a step, given the outcomes `measured` by earlier steps.
/// A predicate is a `bit` parameter's name or `outcomes[i]`, and an outcome
/// must be measured before a step reads it.
fn read_condition(
    instruction: &Instruction,
    condition: &qodec::Condition,
    measured: usize,
) -> Result<Condition, String> {
    let predicates = condition
        .predicates
        .iter()
        .map(|p| {
            if let Some(i) = p
                .strip_prefix("outcomes[")
                .and_then(|r| r.strip_suffix(']'))
            {
                let i: usize = i
                    .trim()
                    .parse()
                    .map_err(|_| format!("`{p}` is not an outcome reference"))?;
                return if i < measured {
                    Ok(Predicate::Outcome(i))
                } else {
                    Err(format!("`{p}` is read before it is measured"))
                };
            }
            if instruction
                .parameters
                .iter()
                .any(|q| q.name == *p && q.kind == ParameterKind::Bit)
            {
                Ok(Predicate::Param(p.clone()))
            } else {
                Err(format!("`{p}` names no `bit` parameter"))
            }
        })
        .collect::<Result<_, _>>()?;
    Ok(Condition {
        predicates,
        invert: condition.invert,
    })
}

/// `instruction`'s action, read once. Fails, naming the step, if a Pauli, a
/// Clifford tableau, an angle or a condition cannot be read, or if an
/// `observe` step is guarded: a call's outcomes must not depend on its
/// arguments or outcomes. A rotation angle must be a
/// literal, or name a `number` or `integer` parameter, as the spec's
/// [validation rules](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/validation.md#instruction-sets)
/// require.
pub fn compile_action(instruction: &Instruction) -> Result<Vec<ActionStep>, String> {
    let parse = |text: &PauliString| parse_pauli(instruction, text);
    let parse_all = |paulis: &mut dyn Iterator<Item = &PauliString>| {
        paulis.map(parse).collect::<Result<Vec<_>, _>>()
    };
    let mut measured = 0;
    instruction
        .action
        .iter()
        .enumerate()
        .map(|(k, step)| {
            let condition = step
                .condition
                .as_ref()
                .map(|c| read_condition(instruction, c, measured))
                .transpose();
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
                .and_then(|action| {
                    if let StepAction::Observe(paulis) = &action {
                        if step.condition.is_some() {
                            return Err(
                                "an `observe` step can't be guarded: the number of outcomes must not vary"
                                    .to_string(),
                            );
                        }
                        measured += paulis.len();
                    }
                    Ok(ActionStep {
                        condition: condition?,
                        action,
                    })
                })
                .map_err(|e| format!("`{}`, action step {k}: {e}", instruction.mnemonic))
        })
        .collect()
}

/// Requires every flat index past `boundary` that `action`'s literal Paulis
/// name to be a temporary an earlier unconditional `stabilize` introduces,
/// as the spec's [instruction structure](https://github.com/microsoft/qdk-ec/blob/fabb5bd8b24615caff42163a2db9f14124cae925/qodec/docs/concepts/instruction-set.md#instruction-structure)
/// says: only such a step introduces one, and a conditional one can't.
pub fn check_temporaries(
    instruction: &Instruction,
    action: &[ActionStep],
    boundary: usize,
) -> Result<(), String> {
    let mut introduced = Vec::new();
    for (k, step) in action.iter().enumerate() {
        let literal = |p: &PauliExpr| match p {
            PauliExpr::Literal(p) => p.support().collect(),
            PauliExpr::Param(_) => Vec::new(),
        };
        let indices: Vec<usize> = match &step.action {
            StepAction::Clifford(map) => map
                .iter()
                .flat_map(|(&(q, _), image)| std::iter::once(q).chain(image.support()))
                .collect(),
            StepAction::Pauli(p) | StepAction::Rotate { pauli: p, .. } => literal(p),
            StepAction::Observe(paulis) | StepAction::Stabilize(paulis) => {
                paulis.iter().flat_map(literal).collect()
            }
        };
        let unconditional_stabilize =
            matches!(step.action, StepAction::Stabilize(_)) && !step.guarded();
        for i in indices {
            if i < boundary || introduced.contains(&i) {
                continue;
            }
            if unconditional_stabilize {
                introduced.push(i);
            } else {
                return Err(format!(
                    "`{}`, action step {k}: flat index {i} is past the boundary, and no earlier unconditional `stabilize` introduces it",
                    instruction.mnemonic
                ));
            }
        }
    }
    Ok(())
}
