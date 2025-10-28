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
    pub scope: usize,
    /// Index into the `dbg_locations` vector in the `Program`
    pub inlined_at: Option<usize>,
}

impl Display for DbgLocation {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, " scope={}", self.scope)?;
        write!(f, "location=({})", self.location)?;
        if let Some(inlined_at) = self.inlined_at {
            write!(f, " inlined_at={inlined_at}")?;
        }
        Ok(())
    }
}

#[derive(Default, Clone)]
pub struct DbgInfo {
    pub dbg_metadata_scopes: Vec<DbgMetadataScope>,
    pub dbg_locations: Vec<DbgLocation>,
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
    pub dbg_location: Option<usize>,
}

impl Display for InstructionMetadata {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "!dbg")?;

        if let Some(dbg_location) = self.dbg_location {
            write!(f, " dbg_location={dbg_location}")?;
        }
        Ok(())
    }
}
