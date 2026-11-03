// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{ResourceId, alias::AliasDecision, cleanup::CleanupState};

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case", tag = "selector", content = "value")]
pub enum PathSelector {
    Field(String),
    Index(i64),
    DynamicIndex { symbol: String, predicate: String },
    Slice { start: i64, end: i64, step: i64 },
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct OwnerPath {
    pub root: u64,
    pub selectors: Vec<PathSelector>,
}

impl OwnerPath {
    #[must_use]
    pub fn root(root: u64) -> Self {
        Self {
            root,
            selectors: Vec::new(),
        }
    }

    #[must_use]
    pub fn field(mut self, field: impl Into<String>) -> Self {
        self.selectors.push(PathSelector::Field(field.into()));
        self
    }

    pub fn index(mut self, index: i64, length: Option<u64>) -> Result<Self, OwnershipError> {
        let normalized = if index < 0 {
            let length = i64::try_from(length.ok_or(OwnershipError::UnknownNegativeIndexLength)?)
                .map_err(|_| OwnershipError::IndexOutOfBounds)?;
            length
                .checked_add(index)
                .ok_or(OwnershipError::IndexOutOfBounds)?
        } else {
            index
        };
        if normalized < 0 || length.is_some_and(|length| normalized.cast_unsigned() >= length) {
            return Err(OwnershipError::IndexOutOfBounds);
        }
        self.selectors.push(PathSelector::Index(normalized));
        Ok(self)
    }

    #[must_use]
    pub fn dynamic_index(
        mut self,
        symbol: impl Into<String>,
        predicate: impl Into<String>,
    ) -> Self {
        self.selectors.push(PathSelector::DynamicIndex {
            symbol: symbol.into(),
            predicate: predicate.into(),
        });
        self
    }

    pub fn slice(mut self, start: i64, end: i64, step: i64) -> Result<Self, OwnershipError> {
        if step == 0 {
            return Err(OwnershipError::ZeroSliceStep);
        }
        self.selectors
            .push(PathSelector::Slice { start, end, step });
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct Epoch(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum BorrowKind {
    Shared,
    Exclusive,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Authority {
    pub path: OwnerPath,
    pub resources: BTreeSet<ResourceId>,
    pub epoch: Epoch,
    pub cleanup: CleanupState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Borrow {
    pub id: u64,
    pub kind: BorrowKind,
    pub path: OwnerPath,
    pub resources: BTreeSet<ResourceId>,
    pub epoch: Epoch,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SplitCertificate {
    pub parent: OwnerPath,
    pub parts: Vec<(OwnerPath, BTreeSet<ResourceId>)>,
}

impl SplitCertificate {
    pub fn checked(
        authority: &Authority,
        mut parts: Vec<(OwnerPath, BTreeSet<ResourceId>)>,
    ) -> Result<Self, OwnershipError> {
        parts.sort_by(|left, right| left.0.cmp(&right.0));
        let mut covered = BTreeSet::new();
        for (_, resources) in &parts {
            if !covered.is_disjoint(resources) {
                return Err(OwnershipError::InvalidSplitCertificate);
            }
            covered.extend(resources);
        }
        if covered != authority.resources || parts.is_empty() {
            return Err(OwnershipError::InvalidSplitCertificate);
        }
        Ok(Self {
            parent: authority.path.clone(),
            parts,
        })
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum OwnershipError {
    #[error("negative index requires a known aggregate length")]
    UnknownNegativeIndexLength,
    #[error("owner path index is out of bounds")]
    IndexOutOfBounds,
    #[error("owner path slice step cannot be zero")]
    ZeroSliceStep,
    #[error("split certificate does not partition the parent resource domain")]
    InvalidSplitCertificate,
    #[error("borrow conflicts with live authority or another borrow")]
    BorrowConflict,
    #[error("borrow or authority epoch is stale")]
    StaleEpoch,
    #[error("branch or loop exits have incompatible ownership domains")]
    JoinMismatch,
    #[error("QHDG-OWNERSHIP-DYNAMIC-ALIAS-001")]
    RuntimeDisjointnessDisabled,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct OwnershipState {
    pub authorities: Vec<Authority>,
    pub borrows: Vec<Borrow>,
    next_borrow: u64,
}

impl OwnershipState {
    pub(crate) const fn snapshot_next_borrow(&self) -> u64 {
        self.next_borrow
    }

    pub(crate) fn from_snapshot(
        authorities: Vec<Authority>,
        borrows: Vec<Borrow>,
        next_borrow: u64,
    ) -> Result<Self, qsc_snapshot::Error> {
        let mut ids = BTreeSet::new();
        let valid_path = |path: &OwnerPath| {
            path.selectors.iter().all(|selector| {
                !matches!(selector, PathSelector::Index(index) if *index < 0)
                    && !matches!(selector, PathSelector::Slice { step: 0, .. })
            })
        };
        if authorities
            .iter()
            .any(|authority| !valid_path(&authority.path))
            || borrows.iter().any(|borrow| {
                !ids.insert(borrow.id)
                    || borrow.id >= next_borrow
                    || !valid_path(&borrow.path)
                    || !authorities.iter().any(|authority| {
                        authority.path == borrow.path
                            && authority.cleanup.is_live()
                            && authority.resources == borrow.resources
                            && authority.epoch == borrow.epoch
                    })
            })
        {
            return Err(qsc_snapshot::Error::malformed(
                "inconsistent ownership checkpoint",
            ));
        }
        Ok(Self {
            authorities,
            borrows,
            next_borrow,
        })
    }

    #[cfg(test)]
    pub(crate) fn from_unchecked(authorities: Vec<Authority>, borrows: Vec<Borrow>) -> Self {
        Self {
            authorities,
            borrows,
            next_borrow: 0,
        }
    }

    pub fn add_authority(&mut self, authority: Authority) -> Result<(), OwnershipError> {
        if self.authorities.iter().any(|existing| {
            !existing.resources.is_disjoint(&authority.resources)
                && existing.cleanup.is_live()
                && authority.cleanup.is_live()
        }) {
            return Err(OwnershipError::BorrowConflict);
        }
        self.authorities.push(authority);
        self.authorities
            .sort_by(|left, right| left.path.cmp(&right.path));
        Ok(())
    }

    pub fn borrow(&mut self, path: &OwnerPath, kind: BorrowKind) -> Result<Borrow, OwnershipError> {
        let authority = self
            .authorities
            .iter()
            .find(|authority| &authority.path == path && authority.cleanup.is_live())
            .ok_or(OwnershipError::BorrowConflict)?;
        let conflicts = self.borrows.iter().any(|borrow| {
            !borrow.resources.is_disjoint(&authority.resources)
                && (kind == BorrowKind::Exclusive || borrow.kind == BorrowKind::Exclusive)
        });
        if conflicts {
            return Err(OwnershipError::BorrowConflict);
        }
        let borrow = Borrow {
            id: self.next_borrow,
            kind,
            path: authority.path.clone(),
            resources: authority.resources.clone(),
            epoch: authority.epoch,
        };
        self.next_borrow += 1;
        self.borrows.push(borrow.clone());
        Ok(borrow)
    }

    pub fn end_borrow(&mut self, borrow: &Borrow) -> Result<(), OwnershipError> {
        let position = self
            .borrows
            .iter()
            .position(|live| live.id == borrow.id && live == borrow)
            .ok_or(OwnershipError::StaleEpoch)?;
        self.borrows.remove(position);
        Ok(())
    }

    pub fn split(
        &mut self,
        path: &OwnerPath,
        certificate: &SplitCertificate,
    ) -> Result<(), OwnershipError> {
        let mut candidate = self.clone();
        candidate.split_in_place(path, certificate)?;
        *self = candidate;
        Ok(())
    }

    fn split_in_place(
        &mut self,
        path: &OwnerPath,
        certificate: &SplitCertificate,
    ) -> Result<(), OwnershipError> {
        let position = self
            .authorities
            .iter()
            .position(|authority| &authority.path == path)
            .ok_or(OwnershipError::InvalidSplitCertificate)?;
        let parent = self.authorities.remove(position);
        if certificate.parent != parent.path
            || SplitCertificate::checked(&parent, certificate.parts.clone())? != *certificate
            || self
                .borrows
                .iter()
                .any(|borrow| !borrow.resources.is_disjoint(&parent.resources))
        {
            return Err(OwnershipError::InvalidSplitCertificate);
        }
        let fragment_count = certificate.parts.len();
        for (fragment, resources) in &certificate.parts {
            self.authorities.push(Authority {
                path: fragment.clone(),
                resources: resources.clone(),
                epoch: parent.epoch,
                cleanup: parent.cleanup.split(fragment_count),
            });
        }
        self.authorities
            .sort_by(|left, right| left.path.cmp(&right.path));
        Ok(())
    }

    pub fn join(&mut self, certificate: &SplitCertificate) -> Result<(), OwnershipError> {
        let mut candidate = self.clone();
        candidate.join_in_place(certificate)?;
        *self = candidate;
        Ok(())
    }

    fn join_in_place(&mut self, certificate: &SplitCertificate) -> Result<(), OwnershipError> {
        let mut children = Vec::new();
        for (path, resources) in &certificate.parts {
            let position = self
                .authorities
                .iter()
                .position(|authority| &authority.path == path && &authority.resources == resources)
                .ok_or(OwnershipError::JoinMismatch)?;
            children.push(self.authorities.remove(position));
        }
        if children.iter().any(|child| {
            self.borrows
                .iter()
                .any(|borrow| !borrow.resources.is_disjoint(&child.resources))
        }) {
            return Err(OwnershipError::BorrowConflict);
        }
        let resources = children
            .iter()
            .flat_map(|child| child.resources.iter().copied())
            .collect();
        let epoch = Epoch(
            children
                .iter()
                .map(|child| child.epoch.0)
                .max()
                .unwrap_or(0)
                + 1,
        );
        let cleanup = CleanupState::join_fragments(
            children.iter().map(|child| &child.cleanup),
            certificate.parts.len(),
        );
        self.add_authority(Authority {
            path: certificate.parent.clone(),
            resources,
            epoch,
            cleanup,
        })
    }

    pub fn join_exits(
        kind: crate::cleanup::ExitKind,
        exits: &[Authority],
    ) -> Result<Authority, OwnershipError> {
        let Some(first) = exits.first() else {
            return Err(OwnershipError::JoinMismatch);
        };
        if exits.iter().any(|exit| {
            exit.path != first.path
                || exit.resources != first.resources
                || matches!(exit.cleanup, CleanupState::Released)
                    != matches!(first.cleanup, CleanupState::Released)
        }) {
            return Err(OwnershipError::JoinMismatch);
        }
        let cleanup = crate::cleanup::CleanupJoin::checked(
            kind,
            &exits
                .iter()
                .map(|exit| exit.cleanup.clone())
                .collect::<Vec<_>>(),
        )
        .map_err(|_| OwnershipError::JoinMismatch)?
        .state;
        Ok(Authority {
            path: first.path.clone(),
            resources: first.resources.clone(),
            epoch: Epoch(exits.iter().map(|exit| exit.epoch.0).max().unwrap_or(0) + 1),
            cleanup,
        })
    }

    pub fn classify_alias(decision: AliasDecision) -> Result<AliasDecision, OwnershipError> {
        match decision {
            AliasDecision::RuntimeCheck => Err(OwnershipError::RuntimeDisjointnessDisabled),
            outcome => Ok(outcome),
        }
    }
}

impl Authority {
    #[must_use]
    pub fn quantum_transition(mut self, operation: impl Into<String>) -> Self {
        self.epoch.0 += 1;
        self.cleanup = self.cleanup.quantum_transition(operation);
        self
    }

    #[must_use]
    pub fn measure(mut self, standard_basis_zero: bool) -> Self {
        self.epoch.0 += 1;
        self.cleanup = self.cleanup.measure(standard_basis_zero);
        self
    }

    #[must_use]
    pub fn reset(mut self, fresh_origin: bool) -> Self {
        self.epoch.0 += 1;
        self.cleanup = self.cleanup.reset(fresh_origin);
        self
    }
}
