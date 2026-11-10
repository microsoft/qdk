// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Reads `qodec` instruction actions, whose Paulis are text, into the
//! pieces' [`ActionStep`]s and [`PauliExpr`]s.

use super::{
    action::{ActionStep, CliffordMap, ParsedPauli, PauliExpr, StepAction},
    pauli::{Clifford, Pauli, PauliOp},
};
use qodec::{Action, Instruction, ParameterKind, PauliString};
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

/// The Clifford of a `clifford` step on `width` flat indices.
pub fn clifford(
    images: &BTreeMap<PauliString, PauliString>,
    width: usize,
) -> Result<Clifford, String> {
    Clifford::from_images(
        images.iter().map(|(g, i)| (g.0.as_str(), i.0.as_str())),
        width,
    )
}

/// The largest qubit index an instruction's action addresses through literal
/// Paulis, plus one; 0 if there is none. Names of `pauli` parameters are
/// skipped, because their operators are only known per call.
#[must_use]
pub fn action_extent(instruction: &Instruction) -> usize {
    let mut extent = 0;
    let mut visit = |text: &PauliString| {
        if let Ok(p) = Pauli::parse(&text.0)
            && let Some(q) = p.max_qubit()
        {
            extent = extent.max(q + 1);
        }
    };
    for step in &instruction.action {
        match &step.action {
            Action::Clifford(images) => images.iter().for_each(|(g, i)| {
                visit(g);
                visit(i);
            }),
            Action::Pauli(p) | Action::Rotate { pauli: p, .. } => visit(p),
            Action::Observe(observables) => observables.iter().for_each(|o| visit(&o.pauli)),
            Action::Stabilize(paulis) => paulis.iter().for_each(&mut visit),
        }
    }
    extent
}

/// The Clifford implemented by an action made only of unguarded `clifford` and
/// literal `pauli` steps, on `width` flat indices. The identity if the action is
/// empty; `None` for any other action, or if a step addresses an index outside
/// `width`.
#[must_use]
pub fn action_clifford(instruction: &Instruction, width: usize) -> Option<Clifford> {
    let mut c = Clifford::identity(width);
    for step in &instruction.action {
        if step.condition.is_some() {
            return None;
        }
        let next = match &step.action {
            Action::Clifford(images) => clifford(images, width).ok()?,
            Action::Pauli(p) => match parse_pauli(instruction, p).ok()? {
                PauliExpr::Literal(p) => Clifford::from_pauli(&p, width).ok()?,
                PauliExpr::Param(_) => return None,
            },
            _ => return None,
        };
        c = c.then(&next);
    }
    Some(c)
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

/// The `pauli` steps of `instruction`'s action, in order.
#[must_use]
pub fn pauli_steps(instruction: &Instruction) -> Vec<ParsedPauli> {
    instruction
        .action
        .iter()
        .filter_map(|step| match &step.action {
            Action::Pauli(text) => Some(parse_pauli(instruction, text)),
            _ => None,
        })
        .collect()
}

/// `instruction`'s action, read once. A Pauli that cannot be read stays an
/// error in the step, for whoever interprets it.
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
                Action::Rotate { pauli, .. } => StepAction::Rotate(parse(pauli)),
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
