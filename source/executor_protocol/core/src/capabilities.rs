// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What flows down a chain with the instructions: [`Capabilities`], the
//! amounts every chain states ([`Resources`]), its yes-or-no features
//! ([`Features`]), and the instruction set's own, which
//! [`fits`](BoundaryCompatible::fits) checks what arrives at a part
//! against what the part provides, giving every [`CapabilitiesMismatch`].

mod boundary;
mod features;
mod resources;

pub use boundary::BatchFault;
pub(crate) use boundary::Boundary;
pub use features::{FeatureMismatch, Features};
pub use resources::{ResourceMismatch, Resources};

use std::error::Error;
use std::fmt;

use crate::{BoundaryCompatible, InstructionSet};

/// What a run, or a part, requires of the parts below it, or a part's
/// fixed limits on what arrives at it: amounts, such as a run's shots;
/// yes-or-no features, such as seeded runs; and the capabilities of the
/// instruction set `S`, such as the qubits a shot holds.
///
/// A requirement flows down the chain: the runner states a run's
/// ([`Driver::requires`](crate::Driver::requires) gets it), and each part
/// passes below what it requires given what arrives
/// ([`Adapter::requires`](crate::Adapter::requires)), by default what
/// arrives. A provision is a part's fixed limits on what arrives at it
/// ([`Adapter::provides`](crate::Adapter::provides),
/// [`Executor::provides`](crate::Executor::provides)), by default none.
/// What arrives at a part must [`fit`](BoundaryCompatible::fits) what it
/// provides, as the crate's `docs/capabilities.md` says.
///
/// One requirement *requires no more than* another when each of its parts
/// does: for each resource and feature it states, the other's value needs
/// at least as much (an amount no smaller, a feature used where it is
/// used; a field left unstated requires nothing, and so does a feature
/// stated `false`), and its set's capabilities fit the other's. It
/// compares two requirements, unlike [`fits`](BoundaryCompatible::fits),
/// which compares a requirement with limits, and reads a field left
/// unstated in the limits as no limit.
///
/// Built by [`new`](Self::new), or from the set's
/// [`unconstrained`](InstructionSet::unconstrained), which states nothing,
/// with [`map_resources`](Self::map_resources) and
/// [`map_features`](Self::map_features).
pub struct Capabilities<S: InstructionSet> {
    resources: Resources,
    features: Features,
    set_capabilities: S::Capabilities,
}

impl<S: InstructionSet> Capabilities<S> {
    /// `resources`, `features`, and `set_capabilities`, the set's own.
    #[must_use]
    pub const fn new(
        resources: Resources,
        features: Features,
        set_capabilities: S::Capabilities,
    ) -> Self {
        Self {
            resources,
            features,
            set_capabilities,
        }
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

    /// The instruction set's own capabilities stated.
    #[must_use]
    pub fn set_capabilities(&self) -> &S::Capabilities {
        &self.set_capabilities
    }

    /// These capabilities, with what `change` makes of their resources,
    /// such as `above.clone().map_resources(|r| r.with_shots(100))`.
    #[must_use]
    pub fn map_resources(self, change: impl FnOnce(Resources) -> Resources) -> Self {
        Self {
            resources: change(self.resources),
            ..self
        }
    }

    /// These capabilities, with what `change` makes of their features.
    #[must_use]
    pub fn map_features(self, change: impl FnOnce(Features) -> Features) -> Self {
        Self {
            features: change(self.features),
            ..self
        }
    }
}

impl<S: InstructionSet> Clone for Capabilities<S> {
    fn clone(&self) -> Self {
        Self::new(self.resources, self.features, self.set_capabilities.clone())
    }
}

impl<S: InstructionSet> PartialEq for Capabilities<S> {
    fn eq(&self, other: &Self) -> bool {
        self.resources == other.resources
            && self.features == other.features
            && self.set_capabilities == other.set_capabilities
    }
}

impl<S: InstructionSet> fmt::Debug for Capabilities<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Capabilities")
            .field("resources", &self.resources)
            .field("features", &self.features)
            .field("set_capabilities", &self.set_capabilities)
            .finish()
    }
}

impl<S: InstructionSet> BoundaryCompatible for Capabilities<S> {
    type Fit = ();
    type Mismatch = CapabilitiesMismatch;

    /// Whether `self`, what arrives at a part, fits `provided`, the part's
    /// limits: every resource and feature `self` states above what
    /// `provided` states, and the set's capabilities' mismatch, at once.
    fn fits(&self, provided: &Self) -> Result<(), CapabilitiesMismatch> {
        let short = CapabilitiesMismatch {
            resources: self.resources.mismatches(&provided.resources),
            features: self.features.mismatches(provided.features),
            set_capabilities: (self.set_capabilities.fits(&provided.set_capabilities).err())
                .map(|mismatch| Box::new(mismatch) as Box<dyn Error + Send + Sync>),
        };
        if short.is_empty() { Ok(()) } else { Err(short) }
    }
}

/// Every way a provision falls short of what arrives: each resource
/// provided short, each feature required and not provided, and the
/// instruction set's capabilities' mismatch. In the mismatch a failed fit
/// returns, at least one is there.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct CapabilitiesMismatch {
    /// Each resource provided short.
    pub resources: Vec<ResourceMismatch>,
    /// Each feature required and not provided.
    pub features: Vec<FeatureMismatch>,
    /// How the set's capabilities fall short: the set's own mismatch,
    /// which `downcast_ref` reads as its type.
    pub set_capabilities: Option<Box<dyn Error + Send + Sync>>,
}

impl CapabilitiesMismatch {
    pub(crate) fn is_empty(&self) -> bool {
        self.resources.is_empty() && self.features.is_empty() && self.set_capabilities.is_none()
    }
}

/// Each mismatch, the set's last, joined by `; `.
impl fmt::Display for CapabilitiesMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let resources = self.resources.iter().map(|m| m as &dyn fmt::Display);
        let features = self.features.iter().map(|m| m as &dyn fmt::Display);
        let set_capabilities = (self.set_capabilities.iter()).map(|m| m as &dyn fmt::Display);
        for (i, mismatch) in resources
            .chain(features)
            .chain(set_capabilities)
            .enumerate()
        {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{mismatch}")?;
        }
        Ok(())
    }
}

impl Error for CapabilitiesMismatch {}
