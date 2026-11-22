// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The parts of a chain as they declare themselves, top to bottom
//! ([`DeclaredParts`]): what the [`on`](crate::DriverJoins::on) that makes
//! a runner checks the whole chain with, and the runner each run.

use std::any::Any;
use std::collections::BTreeMap;

use super::{BoundaryMismatch, CompositionFault};
use crate::{
    BoundaryCompatible, Capabilities, CapabilitiesMismatch, FeatureMismatch, Features,
    InstructionSet, PartId, ResourceMismatch, Resources,
};

/// The parts of a chain, top to bottom, as the roles' hidden `declare`
/// adds them, each given what arrives at it: whether each provides what
/// arrives, and, when it lists faults, each fault.
///
/// What arrives at a part is what the part above requires given what
/// arrived there, starting from what the run requires. A value a part
/// passes below unchanged is the requirement of the first part that
/// required it, which a fault names; a lowering, whose set differs from
/// the one above it, is the first to require what it sends.
///
/// It is public, because the roles' hidden hook names it, but no public
/// path reaches it, so that no other crate can call or override the hook.
/// It holds parts of different instruction sets, so it keeps each sent
/// set and requirement as `Any`, read back as the receiver's set.
pub struct DeclaredParts<'a> {
    parts: usize,
    fits: bool,
    /// What a walk that lists faults keeps; `None` for one that only
    /// checks, as each run's does.
    faults: Option<Faults<'a>>,
}

/// What a walk that lists faults keeps of each part.
struct Faults<'a> {
    names: Vec<&'a str>,
    /// What each part sends, by its position: all but the bottom part.
    sent: Vec<Sent<'a>>,
    /// Each fault, by its requirer's then its provider's position.
    found: BTreeMap<(usize, usize), BoundaryMismatch>,
}

/// What one part sends and requires of the part below, its set erased.
struct Sent<'a> {
    /// The set it sends.
    sends: &'a dyn Any,
    resources: Resources,
    features: Features,
    /// The set's capabilities it requires.
    instructions: Box<dyn Any>,
}

impl<'a> DeclaredParts<'a> {
    /// No part yet, for a walk that lists every fault, by the parts'
    /// names: the one `on` checks a chain with.
    pub(crate) fn listing_faults() -> Self {
        Self {
            parts: 0,
            fits: true,
            faults: Some(Faults {
                names: Vec::new(),
                sent: Vec::new(),
                found: BTreeMap::new(),
            }),
        }
    }

    /// No part yet, for a walk that only checks whether each part provides
    /// what arrives at it: the one each run starts with.
    pub(crate) fn checking() -> Self {
        Self {
            parts: 0,
            fits: true,
            faults: None,
        }
    }

    /// Adds the part below those added, by its name.
    pub(crate) fn push(&mut self, name: &'a str) {
        self.parts += 1;
        if let Some(faults) = &mut self.faults {
            faults.names.push(name);
        }
    }

    /// The part last added accepts `accepts` and provides `provides`, and
    /// `arriving` arrives at it: checks the set the part above sends
    /// against `accepts`, when listing faults, and `arriving` against
    /// `provides`.
    pub(crate) fn receive<S: InstructionSet>(
        &mut self,
        accepts: &S,
        provides: &Capabilities<S>,
        arriving: &Capabilities<S>,
    ) {
        let fit = arriving.fits(provides);
        self.fits &= fit.is_ok();
        let Some(faults) = &mut self.faults else {
            return;
        };
        let receiver = self.parts - 1;
        let sends = (faults.sent[receiver - 1].sends.downcast_ref::<S>())
            .expect("the joins check at compile time that a part sends the set below it accepts");
        if let Err(mismatch) = sends.fits(accepts) {
            self.fits = false;
            let fault = faults.found.entry((receiver - 1, receiver));
            fault.or_default().instructions = Some(Box::new(mismatch));
        }
        let Err(mismatch) = fit else {
            return;
        };
        let sent = &faults.sent;
        // The first part above `receiver` whose requirement `alike` reads
        // as the one arriving at `receiver`, each part between passing it
        // on unchanged.
        let origin = |alike: &dyn Fn(&Sent, &Sent) -> bool| {
            let mut origin = receiver - 1;
            while origin > 0 && alike(&sent[origin - 1], &sent[origin]) {
                origin -= 1;
            }
            origin
        };
        let CapabilitiesMismatch {
            resources,
            features,
            instructions,
        } = mismatch;
        let mut split: BTreeMap<usize, CapabilitiesMismatch> = BTreeMap::new();
        for mismatch in resources {
            let at = origin(&|a, b| match mismatch {
                ResourceMismatch::Shots { .. } => a.resources.shots == b.resources.shots,
                ResourceMismatch::InstructionsPerBatch { .. } => {
                    a.resources.instructions_per_batch == b.resources.instructions_per_batch
                }
            });
            split.entry(at).or_default().resources.push(mismatch);
        }
        for mismatch in features {
            let at = origin(&|a, b| match mismatch {
                FeatureMismatch::Replay => a.features.replay == b.features.replay,
                FeatureMismatch::SeededRuns => a.features.seeded_runs == b.features.seeded_runs,
            });
            split.entry(at).or_default().features.push(mismatch);
        }
        if let Some(mismatch) = instructions {
            let at = origin(&|a, b| {
                let a = a.instructions.downcast_ref::<S::Capabilities>();
                a.is_some() && a == b.instructions.downcast_ref::<S::Capabilities>()
            });
            split.entry(at).or_default().instructions = Some(mismatch);
        }
        for (origin, mismatch) in split {
            let capabilities = &mut (faults.found.entry((origin, receiver)))
                .or_default()
                .capabilities;
            capabilities.resources.extend(mismatch.resources);
            capabilities.features.extend(mismatch.features);
            capabilities.instructions = mismatch.instructions;
        }
    }

    /// The part last added sends `sends`, and requires `required` of the
    /// part below.
    pub(crate) fn send<S: InstructionSet>(&mut self, sends: &'a S, required: &Capabilities<S>) {
        if let Some(faults) = &mut self.faults {
            faults.sent.push(Sent {
                sends,
                resources: *required.resources(),
                features: *required.features(),
                instructions: Box::new(required.instructions().clone()),
            });
        }
    }

    /// How many parts were added.
    pub(crate) fn len(&self) -> usize {
        self.parts
    }

    /// Whether every part provides what arrives at it, and, when listing
    /// faults, accepts what the part above sends.
    pub(crate) fn fit(&self) -> bool {
        self.fits
    }

    /// Each part, by its position and the name it declared.
    pub(crate) fn ids(&self) -> Vec<PartId> {
        let names = self.faults.as_ref().map_or(&[][..], |faults| &faults.names);
        (names.iter().enumerate())
            .map(|(at, name)| PartId::new(at, name))
            .collect()
    }

    /// Every fault, by its requirer then its provider, top to bottom, each
    /// part named as `parts` names it, by its position.
    pub(crate) fn faults(self, parts: &[PartId]) -> Vec<CompositionFault> {
        let found = self.faults.map(|faults| faults.found).unwrap_or_default();
        (found.into_iter())
            .map(|((requirer, provider), mismatch)| CompositionFault {
                requirer: parts[requirer].clone(),
                provider: parts[provider].clone(),
                mismatch,
            })
            .collect()
    }
}
