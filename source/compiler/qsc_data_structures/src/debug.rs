// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    fmt::{self, Display, Formatter},
    rc::Rc,
};

use crate::span::PackageSpan;

#[derive(Clone, Debug)]
pub struct DbgLocation {
    pub location: PackageSpan,
    /// Index into the `dbg_metadata_scopes` vector in the `Program`.
    pub scope: DbgScopeId,
    /// Index into the `dbg_locations` vector in the `Program`
    pub inlined_at: Option<DbgLocationId>,
}

impl Display for DbgLocation {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, " scope={}", self.scope.0)?;
        write!(f, "location=({})", self.location)?;
        if let Some(inlined_at) = self.inlined_at {
            write!(f, " inlined_at={}", inlined_at.0)?;
        }
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct DbgInfo {
    pub dbg_metadata_scopes: Vec<DbgMetadataScope>,
    pub dbg_locations: Vec<DbgLocation>,
}

impl DbgInfo {
    #[must_use]
    pub fn get_location(&self, id: DbgLocationId) -> &DbgLocation {
        &self.dbg_locations[id.0]
    }

    pub fn add_location(&mut self, location: DbgLocation) -> DbgLocationId {
        let id = DbgLocationId(self.dbg_locations.len());
        self.dbg_locations.push(location);
        id
    }

    #[must_use]
    pub fn get_scope(&self, id: DbgScopeId) -> &DbgMetadataScope {
        &self.dbg_metadata_scopes[id.0]
    }

    pub fn add_scope(&mut self, scope: DbgMetadataScope) -> DbgScopeId {
        let id = DbgScopeId(self.dbg_metadata_scopes.len());
        self.dbg_metadata_scopes.push(scope);
        id
    }
}

#[derive(Clone, Debug)]
pub enum DbgMetadataScope {
    /// Corresponds to a callable.
    SubProgram {
        name: Rc<str>,
        location: PackageSpan,
    },
    // TODO: LexicalBlockFile
}

impl Display for DbgMetadataScope {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DbgMetadataScope::SubProgram { name, location } => {
                write!(f, "SubProgram name={name} location=({location})")?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct InstructionMetadata {
    /// Index into the `dbg_locations` vector in the `Program`.
    pub dbg_location: Option<DbgLocationId>,
}

impl Display for InstructionMetadata {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "!dbg")?;

        if let Some(dbg_location) = self.dbg_location {
            write!(f, " dbg_location={}", dbg_location.0)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct DbgLocationId(usize);

impl From<usize> for DbgLocationId {
    fn from(value: usize) -> Self {
        DbgLocationId(value)
    }
}

impl From<DbgLocationId> for usize {
    fn from(value: DbgLocationId) -> Self {
        value.0
    }
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub struct DbgScopeId(usize);

impl From<usize> for DbgScopeId {
    fn from(value: usize) -> Self {
        DbgScopeId(value)
    }
}

impl From<DbgScopeId> for usize {
    fn from(value: DbgScopeId) -> Self {
        value.0
    }
}
