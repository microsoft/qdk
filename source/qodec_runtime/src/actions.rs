// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Interprets `qodec` instruction actions, whose Paulis are text.

use super::pauli::{Clifford, Pauli, PauliOp};
use qodec::{Action, Argument, Instruction, ParameterKind, PauliString};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

/// A Pauli literal on flat indices, or the name of a `pauli` parameter.
#[derive(Clone, Debug, PartialEq)]
pub enum PauliExpr {
    Literal(Pauli),
    Param(String),
}

impl PauliExpr {
    pub fn parse(instruction: &Instruction, text: &PauliString) -> Result<Self, String> {
        let is_param = instruction
            .parameters
            .iter()
            .any(|p| p.kind == ParameterKind::Pauli && p.name == text.0);
        if is_param {
            Ok(PauliExpr::Param(text.0.clone()))
        } else {
            Pauli::from_text(text).map(PauliExpr::Literal)
        }
    }

    /// Resolves a parameter from the call's arguments.
    pub fn resolve(&self, args: &BTreeMap<String, Argument>) -> Result<Pauli, String> {
        match self {
            PauliExpr::Literal(p) => Ok(p.clone()),
            PauliExpr::Param(name) => match args.get(name) {
                Some(Argument::Text(text)) => Pauli::parse(text),
                _ => Err(format!("missing `pauli` argument `{name}`")),
            },
        }
    }

    #[must_use]
    pub fn literal(&self) -> Option<&Pauli> {
        match self {
            PauliExpr::Literal(p) => Some(p),
            PauliExpr::Param(_) => None,
        }
    }
}

/// The images of the generators listed by a `clifford` step, keyed by
/// `(qubit, is_x)`.
pub type CliffordMap = FxHashMap<(usize, bool), Pauli>;

pub fn clifford_map(images: &BTreeMap<PauliString, PauliString>) -> Result<CliffordMap, String> {
    images
        .iter()
        .map(|(generator, image)| {
            let generator = Pauli::from_text(generator)?;
            let key = match generator.terms() {
                [(q, PauliOp::X)] if !generator.negative => (*q, true),
                [(q, PauliOp::Z)] if !generator.negative => (*q, false),
                _ => return Err(format!("`{generator}` is not a Clifford generator")),
            };
            Ok((key, Pauli::from_text(image)?))
        })
        .collect()
}

/// The largest qubit index an instruction's action addresses, plus one.
#[must_use]
pub fn action_extent(instruction: &Instruction) -> usize {
    let mut extent = 0;
    let mut visit = |text: &PauliString| {
        if let Ok(p) = Pauli::from_text(text)
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
/// empty; `None` for any other action.
#[must_use]
pub fn action_clifford(instruction: &Instruction, width: usize) -> Option<Clifford> {
    let mut c = Clifford::identity(width);
    for step in &instruction.action {
        if step.condition.is_some() {
            return None;
        }
        let next = match &step.action {
            Action::Clifford(images) => Clifford::from_images(images, width).ok()?,
            Action::Pauli(p) => match PauliExpr::parse(instruction, p).ok()? {
                PauliExpr::Literal(p) => Clifford::from_pauli(&p, width).ok()?,
                PauliExpr::Param(_) => return None,
            },
            _ => return None,
        };
        c = c.then(&next);
    }
    Some(c)
}
