// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! What flows down a chain with the instructions: [`Capabilities`],
//! amounts ([`Resources`]) and yes-or-no features ([`Features`]), which
//! [`fits`](BoundaryCompatible::fits) checks what arrives at a part
//! against what the part provides, giving every [`CapabilitiesMismatch`].
//!
//! This file is the one place that lists those kinds: every other use of a
//! kind goes through its own methods, so making a kind optional, or adding
//! one, changes this file alone.

mod boundary;
mod features;
mod resources;
mod statements;

pub use boundary::BatchFault;
pub(crate) use boundary::Boundary;
pub use features::{Feature, FeatureMismatch, Features};
pub use resources::{Resource, ResourceMismatch, Resources};
pub use statements::{StatementMismatch, Statements};

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;

use crate::BoundaryCompatible;

/// What a run, or a part, requires of the parts below it, or what a part
/// provides the parts above it: amounts, such as a shot's qubits, and
/// yes-or-no features, such as seeded runs.
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
/// Built by [`new`](Self::new), or from
/// [`UNCONSTRAINED`](Self::UNCONSTRAINED), which states nothing, with
/// [`map_resources`](Self::map_resources) and
/// [`map_features`](Self::map_features).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capabilities {
    resources: Resources,
    features: Features,
}

/// [`Capabilities::UNCONSTRAINED`] as a `'static` place, which the roles'
/// default `provides` lends: a constant holding a map is not promoted to
/// one, as a map has drop glue.
pub(crate) static UNCONSTRAINED: Capabilities = Capabilities::UNCONSTRAINED;

impl Capabilities {
    /// No resource and no feature stated: required, no need; provided, no
    /// limit. What it leaves unstated, composition cannot refuse a chain
    /// on.
    pub const UNCONSTRAINED: Self =
        Self::new(Resources::unconstrained(), Features::unconstrained());

    /// `resources` and `features`.
    #[must_use]
    pub const fn new(resources: Resources, features: Features) -> Self {
        Self {
            resources,
            features,
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

impl BoundaryCompatible for Capabilities {
    type Fit = ();
    type Mismatch = CapabilitiesMismatch;

    /// Whether `self`, what arrives at a part, fits `provided`, the part's
    /// limits: every resource and feature `self` states above what
    /// `provided` states, at once.
    fn fits(&self, provided: &Self) -> Result<(), CapabilitiesMismatch> {
        let short = CapabilitiesMismatch {
            resources: self.resources.mismatches(&provided.resources),
            features: self.features.mismatches(&provided.features),
        };
        if short.is_empty() { Ok(()) } else { Err(short) }
    }
}

/// Every way a provision falls short of what arrives: each resource
/// provided short, and each feature required and not provided. In the
/// mismatch a failed fit returns, at least one list is not empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapabilitiesMismatch {
    /// Each resource provided short.
    pub resources: Vec<ResourceMismatch>,
    /// Each feature required and not provided.
    pub features: Vec<FeatureMismatch>,
}

impl CapabilitiesMismatch {
    pub(crate) fn is_empty(&self) -> bool {
        self.resources.is_empty() && self.features.is_empty()
    }

    /// Each mismatch, grouped by `origin`, which is given, for one
    /// mismatch, whether two capabilities state its key alike.
    pub(crate) fn split_by<O: Ord>(
        self,
        mut origin: impl FnMut(&dyn Fn(&Capabilities, &Capabilities) -> bool) -> O,
    ) -> BTreeMap<O, Self> {
        let mut split: BTreeMap<O, Self> = BTreeMap::new();
        for mismatch in self.resources {
            let key = &mismatch.key;
            let at = origin(&|a, b| a.resources.get(key) == b.resources.get(key));
            split.entry(at).or_default().resources.push(mismatch);
        }
        for mismatch in self.features {
            let key = &mismatch.key;
            let at = origin(&|a, b| a.features.get(key) == b.features.get(key));
            split.entry(at).or_default().features.push(mismatch);
        }
        split
    }
}

/// Each mismatch, joined by `; `.
impl fmt::Display for CapabilitiesMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let resources = self.resources.iter().map(|m| m as &dyn fmt::Display);
        let features = self.features.iter().map(|m| m as &dyn fmt::Display);
        for (i, mismatch) in resources.chain(features).enumerate() {
            if i > 0 {
                f.write_str("; ")?;
            }
            write!(f, "{mismatch}")?;
        }
        Ok(())
    }
}

impl Error for CapabilitiesMismatch {}
