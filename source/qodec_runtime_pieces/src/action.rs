// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Instruction actions, precompiled for the pieces that interpret them per
//! call: Paulis that may name a `pauli` parameter, and the steps of an
//! action.

use super::{pauli::Pauli, protocol::OwnedParam};
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
    Rotate(ParsedPauli),
    /// Measurements of these Paulis.
    Observe(Vec<ParsedPauli>),
    /// A preparation of a state these Paulis stabilize.
    Stabilize(Vec<ParsedPauli>),
}
