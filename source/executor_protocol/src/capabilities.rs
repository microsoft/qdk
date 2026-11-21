// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part requires of the part below it, or provides the part above
//! it: its [`Capabilities`], one type for both sides of a boundary, which
//! [`fits`](BoundaryCompatible::fits) checks against each other, giving
//! the [`Boundary`] where they meet, or every [`CapabilitiesMismatch`].
//!
//! Each holds three kinds of statement: the instructions sent or accepted
//! ([`InstructionSet`]), amounts ([`Resources`]) and yes-or-no features
//! ([`Features`]). The last two follow one rule of reach, which this
//! folder alone applies: a requirement reaches each part below down to
//! the first that serves it. This file is the one place that lists those
//! kinds ([`Kinds`]): every other use of a kind goes through its own
//! methods. What a driver provides a run states the same two kinds
//! ([`RunProvision`]).

mod boundary;
mod features;
mod resources;
mod run_provision;
mod statements;

pub use boundary::{BatchFault, Boundary};
pub use features::{Feature, FeatureMismatch, Features};
pub use resources::{Resource, ResourceMismatch, Resources};
pub use run_provision::RunProvision;
pub use statements::{StatementMismatch, Statements};

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::{BoundaryCompatible, InstructionMismatch, InstructionSet};

/// What a part requires of the part below it, or provides the part above
/// it: the instructions it sends or accepts, amounts, and yes-or-no
/// features.
///
/// A driver states what it requires
/// ([`Driver::requires`](crate::Driver::requires)), an executor what it
/// provides ([`Executor::provides`](crate::Executor::provides)), and an
/// adapter both. The joins check, at each boundary, that what the part
/// above requires [`fits`](BoundaryCompatible::fits) what the part below
/// provides. A resource or a feature a part does not state, it neither
/// needs nor limits; a requirement of one reaches every part below that
/// states it, down to the first that serves it
/// ([`Statements::serving`]), as the crate's `docs/capabilities.md` says.
///
/// Built by [`new`](Self::new), from all three: a part that states no
/// resource and no feature passes [`Resources::unconstrained`] and
/// [`Features::unconstrained`], and composes with any part on them.
#[derive(Clone, Debug)]
pub struct Capabilities {
    instructions: Arc<InstructionSet>,
    kinds: Kinds,
}

impl Capabilities {
    /// `instructions`, sent or accepted, with `resources` and `features`.
    #[must_use]
    pub fn new(instructions: InstructionSet, resources: Resources, features: Features) -> Self {
        Self {
            instructions: Arc::new(instructions),
            kinds: Kinds {
                resources,
                features,
            },
        }
    }

    /// The instructions sent, for a requirement, or accepted, for a
    /// provision.
    #[must_use]
    pub fn instructions(&self) -> &InstructionSet {
        &self.instructions
    }

    /// The amounts stated.
    #[must_use]
    pub fn resources(&self) -> &Resources {
        &self.kinds.resources
    }

    /// The features stated.
    #[must_use]
    pub fn features(&self) -> &Features {
        &self.kinds.features
    }

    /// Every resource and feature `self`, a requirement, states above what
    /// `provided` states; the instructions are not compared.
    pub(crate) fn kinds_fit(&self, provided: &Self) -> Result<(), CapabilitiesMismatch> {
        self.kinds.fit(&provided.kinds)
    }

    /// What `self`, a part's provision, provides over `below`, the
    /// provision of the parts below it, with `self`'s instructions.
    // Once per run: kept out of line, so that it does not grow the
    // per-call code of the run it would be inlined into.
    #[inline(never)]
    pub(crate) fn over(&self, below: &Self) -> Self {
        self.with_kinds(self.kinds.over(&below.kinds))
    }

    /// What reaches below a part that requires `requires` and provides
    /// `provides`, when `self` is required of it: its own requirement,
    /// with its instructions, and each field of `self` it does not serve,
    /// the larger where both state it.
    pub(crate) fn past(&self, requires: &Self, provides: &Self) -> Self {
        let reaching = self.kinds.not_served_by(&provides.kinds);
        requires.with_kinds(requires.kinds.larger(&reaching))
    }

    /// What of `self`, a requirement, reaches past `provision`: each field
    /// it does not serve.
    pub(crate) fn not_served_by(&self, provision: &Self) -> Self {
        self.with_kinds(self.kinds.not_served_by(&provision.kinds))
    }

    /// These capabilities' instructions, with the resources and features
    /// `other` states: a boundary's statement of a run, whose instructions
    /// are those its fit matched.
    pub(crate) fn with_kinds_of(&self, other: &Self) -> Self {
        self.with_kinds(other.kinds.clone())
    }

    /// These capabilities' instructions, with `kinds`.
    fn with_kinds(&self, kinds: Kinds) -> Self {
        Self {
            instructions: Arc::clone(&self.instructions),
            kinds,
        }
    }
}

impl BoundaryCompatible for Capabilities {
    type Fit = Boundary;
    type Mismatch = CapabilitiesMismatch;

    /// The boundary where `self`, required, meets `provided`: the opcode
    /// map of the instructions, as [`InstructionSet`]'s `fits` gives it;
    /// or every mismatch of the instructions, the resources and the
    /// features at once.
    fn fits(&self, provided: &Self) -> Result<Boundary, CapabilitiesMismatch> {
        let opcodes = self.instructions.fits(&provided.instructions);
        match (opcodes, self.kinds_fit(provided)) {
            (Ok(opcode_map), Ok(())) => Ok(Boundary::new(opcode_map, self, provided)),
            (opcodes, kinds) => Err(CapabilitiesMismatch {
                instructions: opcodes.err().unwrap_or_default(),
                ..kinds.err().unwrap_or_default()
            }),
        }
    }
}

/// The resources and features a part states: the kinds that follow the
/// rule of reach, as [`Statements`] says. Making a kind optional, or
/// adding one, changes this type, its methods and the arguments of
/// [`Capabilities::new`] alone.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Kinds {
    resources: Resources,
    features: Features,
}

impl Kinds {
    /// Nothing stated.
    const fn new() -> Self {
        Self {
            resources: Resources::unconstrained(),
            features: Features::unconstrained(),
        }
    }

    /// Every field `self`, a requirement, states above what `provided`
    /// states.
    fn fit(&self, provided: &Self) -> Result<(), CapabilitiesMismatch> {
        let short = CapabilitiesMismatch {
            instructions: Vec::new(),
            resources: self.resources.mismatches(&provided.resources),
            features: self.features.mismatches(&provided.features),
        };
        if short.is_empty() { Ok(()) } else { Err(short) }
    }

    /// As [`Statements`]' `over`, of each kind, with no replay where
    /// seeded runs are not provided, as [`Features::with_replay`] keeps
    /// for one part.
    fn over(&self, below: &Self) -> Self {
        let features = self.features.over(&below.features);
        Self {
            resources: self.resources.over(&below.resources),
            features: features.replay_only_if_seeded(),
        }
    }

    /// As [`Statements`]' `not_served_by`, of each kind.
    fn not_served_by(&self, provision: &Self) -> Self {
        Self {
            resources: self.resources.not_served_by(&provision.resources),
            features: self.features.not_served_by(&provision.features),
        }
    }

    /// As [`Statements`]' `larger`, of each kind.
    fn larger(&self, other: &Self) -> Self {
        Self {
            resources: self.resources.larger(&other.resources),
            features: self.features.larger(&other.features),
        }
    }
}

/// Every way a provision falls short of a requirement: each instruction
/// that does not fit, each resource provided short, and each feature
/// required and not provided. In the mismatch a failed fit returns, at
/// least one list is not empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapabilitiesMismatch {
    /// Each instruction that does not fit, as [`InstructionSet`]'s `fits`
    /// finds them.
    pub instructions: Vec<InstructionMismatch>,
    /// Each resource provided short.
    pub resources: Vec<ResourceMismatch>,
    /// Each feature required and not provided.
    pub features: Vec<FeatureMismatch>,
}

impl CapabilitiesMismatch {
    fn is_empty(&self) -> bool {
        self.instructions.is_empty() && self.resources.is_empty() && self.features.is_empty()
    }
}

/// Each mismatch, joined by `; `.
impl fmt::Display for CapabilitiesMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let instructions = self.instructions.iter().map(|m| m as &dyn fmt::Display);
        let resources = self.resources.iter().map(|m| m as &dyn fmt::Display);
        let features = self.features.iter().map(|m| m as &dyn fmt::Display);
        for (i, mismatch) in instructions.chain(resources).chain(features).enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{mismatch}")?;
        }
        Ok(())
    }
}

impl Error for CapabilitiesMismatch {}
