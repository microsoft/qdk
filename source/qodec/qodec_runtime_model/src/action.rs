// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Instruction actions, read once: Paulis that may name a `pauli` parameter,
//! and the steps of an action. And facts that follow from an action alone,
//! such as the Clifford it implements.

use super::{
    pauli::{Clifford, DensePauli, Pauli},
    protocol::OwnedParam,
};
use rustc_hash::FxHashMap;
use std::collections::BTreeMap;

/// The images of the generators listed by a `clifford` step, keyed by
/// `(qubit, is_x)`. Generators the step does not list map to themselves and
/// have no entry.
pub type CliffordMap = FxHashMap<(usize, bool), Pauli>;

/// A Pauli of an action: a literal on flat indices, or the name of a `pauli`
/// parameter, whose value each call supplies.
#[derive(Clone, Debug, PartialEq)]
pub enum PauliExpr {
    /// A literal Pauli.
    Literal(Pauli),
    /// The value of this `pauli` parameter.
    Param(String),
}

/// A [`PauliExpr`] as written, or why it could not be read. The error only
/// matters if a call needs the Pauli.
pub type ParsedPauli = Result<PauliExpr, String>;

impl PauliExpr {
    /// The Pauli for a call with arguments `args`: the literal itself, or the
    /// parsed argument of the parameter.
    pub fn resolve(&self, args: &BTreeMap<String, OwnedParam>) -> Result<Pauli, String> {
        match self {
            PauliExpr::Literal(p) => Ok(p.clone()),
            PauliExpr::Param(name) => match args.get(name) {
                Some(OwnedParam::Text(text)) => Pauli::parse(text),
                _ => Err(format!("missing `pauli` argument `{name}`")),
            },
        }
    }
}

/// The angle of a rotation.
#[derive(Clone, Debug, PartialEq)]
pub enum Angle {
    /// A literal angle.
    Literal(f64),
    /// The value of the parameter with this index in the instruction's
    /// parameters.
    Param(usize),
}

/// One step of an instruction's action.
#[derive(Clone, Debug)]
pub struct ActionStep {
    /// Whether the step only applies under a condition.
    pub guarded: bool,
    /// What the step does.
    pub action: StepAction,
}

/// What an [`ActionStep`] does.
#[derive(Clone, Debug)]
pub enum StepAction {
    /// A Clifford, given by the images of some generators.
    Clifford {
        /// Each listed (generator, image).
        images: Vec<(ParsedPauli, ParsedPauli)>,
        /// The Clifford the images define, or why they don't define one.
        map: Result<CliffordMap, String>,
    },
    /// A Pauli.
    Pauli(ParsedPauli),
    /// A rotation about a Pauli.
    Rotate {
        /// The rotation's axis.
        pauli: ParsedPauli,
        /// The rotation's angle, or why it could not be read.
        angle: Result<Angle, String>,
    },
    /// Measurements of these Paulis.
    Observe(Vec<ParsedPauli>),
    /// A preparation of a state these Paulis stabilize.
    Stabilize(Vec<ParsedPauli>),
}

/// The largest flat index `action` addresses through literal Paulis, plus
/// one; 0 if there is none. Pauli parameters are skipped, because their
/// operators are only known per call.
#[must_use]
pub fn action_extent(action: &[ActionStep]) -> usize {
    let mut extent = 0;
    let mut visit = |p: &ParsedPauli| {
        if let Ok(PauliExpr::Literal(p)) = p
            && let Some(q) = p.max_qubit()
        {
            extent = extent.max(q + 1);
        }
    };
    for step in action {
        match &step.action {
            StepAction::Clifford { images, .. } => images.iter().for_each(|(g, i)| {
                visit(g);
                visit(i);
            }),
            StepAction::Pauli(p) | StepAction::Rotate { pauli: p, .. } => visit(p),
            StepAction::Observe(paulis) | StepAction::Stabilize(paulis) => {
                paulis.iter().for_each(&mut visit);
            }
        }
    }
    extent
}

/// The Clifford implemented by an action made only of unguarded `clifford` and
/// literal `pauli` steps, on `width` flat indices. The identity if the action is
/// empty; `None` for any other action, or if a step addresses an index outside
/// `width`.
#[must_use]
pub fn action_clifford(action: &[ActionStep], width: usize) -> Option<Clifford> {
    let mut c = Clifford::identity(width);
    for step in action {
        if step.guarded {
            return None;
        }
        let next = match &step.action {
            StepAction::Clifford { map, .. } => clifford_of(map.as_ref().ok()?, width)?,
            StepAction::Pauli(Ok(PauliExpr::Literal(p))) => Clifford::from_pauli(p, width).ok()?,
            _ => return None,
        };
        c = c.then(&next);
    }
    Some(c)
}

/// The Clifford of a `clifford` step on `width` flat indices, or `None` if a
/// generator or image is outside them.
fn clifford_of(map: &CliffordMap, width: usize) -> Option<Clifford> {
    let mut c = Clifford::identity(width);
    for (&(q, is_x), image) in map {
        if q >= width {
            return None;
        }
        let image = DensePauli::from_pauli(image, width).ok()?;
        if is_x {
            c.x_images[q] = image;
        } else {
            c.z_images[q] = image;
        }
    }
    Some(c)
}
