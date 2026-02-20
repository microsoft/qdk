// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Debug information metadata for RIR.
//!
//! These structures are based on LLVM's source level debugging features, which provide
//! metadata for mapping compiled code back to source locations and lexical scopes.
//! See: <https://llvm.org/docs/SourceLevelDebugging.html>

use indenter::{Indented, indented};
use std::{
    fmt::{self, Display, Formatter, Write},
    rc::Rc,
};

/// A source code offset in the compilation.
/// This should be resolvable into a real source code location (file, line, column).
#[derive(Clone, Debug, Copy)]
pub struct DbgPackageOffset {
    /// An FIR `PackageId`.
    pub package_id: usize,
    /// The source code offset within the package.
    pub offset: u32,
}

/// A source code location. This is an analogue of the `DILocation` metadata in LLVM.
/// <https://llvm.org/doxygen/classllvm_1_1DILocation.html>
#[derive(Clone, Debug)]
pub struct DbgLocation {
    /// Source code location. Corresponds to `DILocation`'s `line` and `column` fields.
    pub location: DbgPackageOffset,
    /// The lexical scope that contains this location.
    pub scope: DbgScopeId,
    /// The location that this location was inlined at, if any.
    pub inlined_at: Option<DbgLocationId>,
}

impl Display for DbgLocation {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "scope={}", self.scope.0)?;
        write!(
            f,
            " location=({}-{})",
            self.location.package_id, self.location.offset
        )?;
        if let Some(inlined_at) = self.inlined_at {
            write!(f, " inlined_at={}", inlined_at.0)?;
        }
        Ok(())
    }
}

/// A lexical scope. This is an analogue of the `DIScope` metadata in LLVM.
/// <https://llvm.org/doxygen/classllvm_1_1DIScope.html>
#[derive(Clone, Debug)]
pub enum DbgMetadataScope {
    /// Corresponds to a callable in the source code, `DISubprogram` in LLVM.
    SubProgram {
        /// Callable name.
        name: Rc<str>,
        /// Source code location of the callable implementation. Corresponds to `DIScope`'s `line` and `column` fields.
        location: DbgPackageOffset,
    },
    /// Corresponds to a block, such as a loop body. `DILexicalBlockFile` in LLVM.
    LexicalBlockFile {
        /// Used to distinguish different scopes have the same source code location, such as different iterations of a loop body.
        discriminator: usize,
        /// Source code location of the block. Corresponds to `DIScope`'s `line` and `column` fields.
        location: DbgPackageOffset,
    },
}

impl Display for DbgMetadataScope {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DbgMetadataScope::SubProgram { name, location } => {
                write!(
                    f,
                    "SubProgram name={name} location=({}-{})",
                    location.package_id, location.offset
                )?;
            }
            DbgMetadataScope::LexicalBlockFile {
                discriminator,
                location,
            } => {
                write!(
                    f,
                    "LexicalBlockFile location=({}-{}) discriminator={}",
                    location.package_id, location.offset, discriminator
                )?;
            }
        }
        Ok(())
    }
}

/// Program debug metadata, including all debug locations and scopes.
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

impl Display for DbgInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if !self.dbg_metadata_scopes.is_empty() || !self.dbg_locations.is_empty() {
            let mut indent = set_indentation(indented(f), 0);
            write!(indent, "\ndbg_metadata_scopes:")?;
            indent = set_indentation(indent, 1);
            for (index, scope) in self.dbg_metadata_scopes.iter().enumerate() {
                write!(indent, "\n{index} = {scope}")?;
            }
            indent = set_indentation(indent, 0);
            write!(indent, "\ndbg_locations:")?;
            indent = set_indentation(indent, 1);
            for (index, location) in self.dbg_locations.iter().enumerate() {
                write!(indent, "\n[{index}]: {location}")?;
            }
        }
        Ok(())
    }
}

fn set_indentation<'a, 'b>(
    indent: Indented<'a, Formatter<'b>>,
    level: usize,
) -> Indented<'a, Formatter<'b>> {
    match level {
        0 => indent.with_str(""),
        1 => indent.with_str("    "),
        2 => indent.with_str("        "),
        _ => unimplemented!("indentation level not supported"),
    }
}

/// Debug metadata attached to an instruction. This can be used to generate `!dbg` metadata in QIR.
#[derive(Clone, Debug, Copy)]
pub struct InstructionDbgMetadata {
    pub dbg_location: DbgLocationId,
}

impl Display for InstructionDbgMetadata {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "!dbg dbg_location={}", self.dbg_location.0)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
/// Index into the `dbg_locations` vector in `DbgInfo`.
pub struct DbgLocationId(usize);

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
/// Index into the `dbg_metadata_scopes` vector in `DbgInfo`.
pub struct DbgScopeId(usize);
