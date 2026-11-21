// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The parts of a chain as they declare themselves, top to bottom
//! ([`DeclaredParts`]): what the [`on`](crate::DriverJoins::on) that makes
//! a runner checks the whole chain with, what bounds each run, and what
//! names each part.

use super::CompositionFault;
use crate::PartId;
use crate::capabilities::Forwarding;
use crate::{BoundaryCompatible, Capabilities};

/// The parts of a chain, top to bottom, each with its name and what it
/// requires and provides, as the roles' hidden `declare` adds them.
///
/// It is public, because the roles' hidden hook names it, but no public
/// path reaches it, so that no other crate can call or override the hook.
pub struct DeclaredParts<'a> {
    parts: Vec<DeclaredPart<'a>>,
}

/// Why a run's provision is found: the parts it is asked of start with a
/// driver.
const CHAIN: &str = "a run's parts start with a driver";

/// One part, as it declares itself: a driver requires, an executor
/// provides, and an adapter does both.
struct DeclaredPart<'a> {
    name: &'a str,
    requires: Option<&'a Capabilities>,
    provides: Option<&'a Capabilities>,
}

impl<'a> DeclaredParts<'a> {
    /// No part yet.
    pub(crate) fn new() -> Self {
        Self { parts: Vec::new() }
    }

    /// Adds the part below those added: its name, what it requires and
    /// what it provides.
    pub(crate) fn push(
        &mut self,
        name: &'a str,
        requires: Option<&'a Capabilities>,
        provides: Option<&'a Capabilities>,
    ) {
        self.parts.push(DeclaredPart {
            name,
            requires,
            provides,
        });
    }

    /// How many parts were added.
    pub(crate) fn len(&self) -> usize {
        self.parts.len()
    }

    /// Every fault of the chain, by its requirer top to bottom: what each
    /// part requires against what the part below it provides, all of it;
    /// then each resource and feature the part below does not require
    /// again, which it forwards, against each part further below, until a
    /// part requires it again.
    pub(crate) fn faults(&self) -> Vec<CompositionFault> {
        let mut faults = Vec::new();
        for (at, requirer) in self.parts.iter().enumerate() {
            let Some(mut forwarded) = requirer.requires.cloned() else {
                continue;
            };
            for (below, provider) in self.parts.iter().enumerate().skip(at + 1) {
                let provided = (provider.provides).expect("each part below the driver provides");
                let fit = if below == at + 1 {
                    forwarded.fits(provided).map(drop)
                } else {
                    forwarded.kinds_fit(provided)
                };
                if let Err(mismatch) = fit {
                    faults.push(CompositionFault {
                        requirer: self.id(at),
                        provider: self.id(below),
                        mismatch,
                    });
                }
                let Some(answered) = provider.requires else {
                    break;
                };
                forwarded = forwarded.unanswered(answered);
            }
        }
        faults
    }

    /// What a run of these parts, a driver above its adapters, is
    /// bounded by, over `below`, the provision of the parts below them:
    /// `below`, then each adapter's provision above it, the stricter where
    /// two state a resource or a feature, through each adapter that does
    /// not require it again; less each field the driver requires, which it
    /// serves itself, as an adapter does. The runner checks each run
    /// against it, and so does a joined driver, against its own parts, so
    /// that both give the same verdict.
    pub(crate) fn run_provision(&self, below: &Capabilities) -> Capabilities {
        let (driver, adapters) = self.parts.split_first().expect(CHAIN);
        let provision = adapters.iter().rev().fold(below.clone(), |below, adapter| {
            let declared = adapter.provides.zip(adapter.requires);
            let (provided, required) = declared.expect("an adapter declares both sides");
            provided.stricter_through(required, &below)
        });
        provision.unanswered(driver.requires.expect("a driver requires"))
    }

    /// Each part, by its position and name, top to bottom.
    pub(crate) fn ids(&self) -> Vec<PartId> {
        (0..self.parts.len()).map(|at| self.id(at)).collect()
    }

    /// The part at `at`, by its position and name.
    pub(crate) fn id(&self, at: usize) -> PartId {
        PartId::new(at, self.parts[at].name)
    }
}
