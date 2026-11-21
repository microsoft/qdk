// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What a part requires of the part below it, or provides the part above
//! it: its [`Capabilities`], one type for both sides of a boundary, which
//! [`fits`](BoundaryCompatible::fits) checks against each other, giving
//! the [`Boundary`] where they meet, or every [`CapabilitiesMismatch`].
//!
//! Each holds three kinds of statement: the instructions sent or accepted
//! ([`InstructionSet`]), amounts ([`Resources`]) and yes-or-no features
//! ([`Features`]). This file is the one place that lists the kinds: every
//! other use of a kind goes through its own methods.

mod boundary;
mod features;
mod resources;
mod statements;

pub use boundary::{BatchFault, Boundary};
pub use features::{Feature, FeatureMismatch, Features};
pub use resources::{Resource, ResourceMismatch, Resources};
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
/// provides. A resource or a feature a part does not state is forwarded:
/// across an adapter that does not state it, the requirement of the part
/// above meets the provision of the part below, as the crate's
/// `docs/capabilities.md` says.
///
/// Built from [`new`](Self::new), with the builders below: a part that
/// states no resource and no feature writes only its instruction set.
#[derive(Clone, Debug)]
pub struct Capabilities {
    instructions: Arc<InstructionSet>,
    resources: Resources,
    features: Features,
}

impl Capabilities {
    /// `instructions`, sent or accepted, with no resource and no feature
    /// stated.
    #[must_use]
    pub fn new(instructions: InstructionSet) -> Self {
        Self {
            instructions: Arc::new(instructions),
            resources: Resources::new(),
            features: Features::new(),
        }
    }

    /// These capabilities, with `resources` in place of their own.
    #[must_use]
    pub fn with_resources(mut self, resources: Resources) -> Self {
        self.resources = resources;
        self
    }

    /// These capabilities, with `features` in place of their own.
    #[must_use]
    pub fn with_features(mut self, features: Features) -> Self {
        self.features = features;
        self
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
        &self.resources
    }

    /// The features stated.
    #[must_use]
    pub fn features(&self) -> &Features {
        &self.features
    }

    /// Every resource and feature `self`, a requirement, states above what
    /// `provided` states; the instructions are not compared.
    pub(crate) fn kinds_fit(&self, provided: &Self) -> Result<(), CapabilitiesMismatch> {
        let short = CapabilitiesMismatch {
            instructions: Vec::new(),
            resources: self.resources.mismatches(&provided.resources),
            features: self.features.mismatches(&provided.features),
        };
        if short.is_empty() { Ok(()) } else { Err(short) }
    }

    /// Whether `self`, a provision, serves a run of `shots` shots, whose
    /// seed is fixed when `seeded`: what a run states only as it starts.
    // Once per run: kept out of line, so that it does not grow the
    // per-call code of the run it would be inlined into.
    #[inline(never)]
    pub(crate) fn admits_run(
        &self,
        shots: usize,
        seeded: bool,
    ) -> Result<(), CapabilitiesMismatch> {
        let shots = u32::try_from(shots).unwrap_or(u32::MAX);
        let run = self.with_kinds(
            Resources::new().with_shots(shots),
            Features::new().with_seeded_runs(seeded),
        );
        run.kinds_fit(self)
    }

    /// These capabilities' instructions, with the resources and features
    /// `other` states: a boundary's statement of a run, whose instructions
    /// are those its fit matched.
    pub(crate) fn with_kinds_of(&self, other: &Self) -> Self {
        self.with_kinds(other.resources.clone(), other.features.clone())
    }

    /// These capabilities' instructions, never forwarded, with `resources`
    /// and `features`.
    fn with_kinds(&self, resources: Resources, features: Features) -> Self {
        Self {
            instructions: Arc::clone(&self.instructions),
            resources,
            features,
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

/// How a forwarded statement combines across parts: implemented by each
/// kind that a part may leave unstated, and by [`Capabilities`], which
/// combines each kind and keeps its own instructions, never forwarded.
pub(crate) trait Forwarding {
    /// `self`'s statement of each field, else `other`'s.
    fn or(&self, other: &Self) -> Self;
    /// `self`'s statement of each field `answered` does not state.
    fn unanswered(&self, answered: &Self) -> Self;
    /// The stricter of `self`'s and `other`'s provisions of each field,
    /// where both state it, else the one that does.
    fn stricter(&self, other: &Self) -> Self;

    /// What `self`, a part's provision, provides together with `below`,
    /// the provision of the parts below it: `self`'s, and the stricter of
    /// it and `below`'s where `requires`, the part's requirement, does not
    /// state it again.
    // Once per run: kept out of line, so that it does not grow the
    // per-call code of the run it would be inlined into.
    #[inline(never)]
    fn stricter_through(&self, requires: &Self, below: &Self) -> Self
    where
        Self: Sized,
    {
        self.stricter(&below.unanswered(requires))
    }
}

impl Forwarding for Capabilities {
    fn or(&self, other: &Self) -> Self {
        let resources = self.resources.or(&other.resources);
        self.with_kinds(resources, self.features.or(&other.features))
    }

    fn unanswered(&self, answered: &Self) -> Self {
        let resources = self.resources.unanswered(&answered.resources);
        self.with_kinds(resources, self.features.unanswered(&answered.features))
    }

    fn stricter(&self, other: &Self) -> Self {
        let resources = self.resources.stricter(&other.resources);
        self.with_kinds(resources, self.features.stricter(&other.features))
    }
}

/// Every way a provision falls short of a requirement: each instruction
/// that does not fit, each resource provided short, and each feature
/// required and not provided. At least one is not empty.
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
