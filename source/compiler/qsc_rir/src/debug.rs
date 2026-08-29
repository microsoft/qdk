// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Debug information metadata for RIR.
//!
//! These structures are based on LLVM's source level debugging features, which provide
//! metadata for mapping compiled code back to source locations and lexical scopes.
//! See: <https://llvm.org/docs/SourceLevelDebugging.html>

use indenter::indented;
use qsc_data_structures::{
    display::core::set_indentation, functors::FunctorApp, index_map::IndexMap,
};
use std::{
    fmt::{self, Display, Formatter, Write},
    rc::Rc,
};

/// A source code offset in the compilation.
/// This should be resolvable into a real source code location (file, line, column).
#[derive(Clone, Debug, Copy)]
pub struct DbgPackageOffset {
    /// The FIR package that owns the source code offset.
    pub package_id: usize,
    /// The source code offset within the package.
    pub offset: u32,
}

/// The concrete FIR callable and specialization represented by a debug scope.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DbgCallableId {
    /// The package containing the callable in FIR storage.
    pub package_id: usize,
    /// The callable item ID within the FIR package.
    pub item_id: usize,
    /// The functor application selecting the callable specialization.
    pub functor_app: FunctorApp,
}

/// The concrete FIR loop represented by a debug scope.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DbgLoopId {
    /// The package containing the loop expression in FIR storage.
    pub package_id: usize,
    /// The loop expression ID within the FIR package.
    pub expr_id: usize,
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
pub enum DbgScope {
    /// Corresponds to a callable in the source code, `DISubprogram` in LLVM.
    SubProgram {
        /// Callable name.
        name: Rc<str>,
        /// Concrete FIR callable and specialization identity.
        callable_id: DbgCallableId,
        /// Source code location of the callable implementation. Corresponds to `DIScope`'s `line` and `column` fields.
        location: DbgPackageOffset,
    },
    /// Corresponds to a block, such as a loop body. `DILexicalBlockFile` in LLVM.
    LexicalBlockFile {
        /// Used to distinguish different scopes have the same source code location, such as different iterations of a loop body.
        discriminator: usize,
        /// Concrete FIR loop identity.
        loop_id: DbgLoopId,
        /// Source code location of the block. Corresponds to `DIScope`'s `line` and `column` fields.
        location: DbgPackageOffset,
    },
}

impl Display for DbgScope {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            DbgScope::SubProgram {
                name,
                callable_id,
                location,
            } => {
                write!(
                    f,
                    "SubProgram name={name} location=({}-{}) callable=({}-{} adjoint={} controlled={})",
                    location.package_id,
                    location.offset,
                    callable_id.package_id,
                    callable_id.item_id,
                    callable_id.functor_app.adjoint,
                    callable_id.functor_app.controlled
                )?;
            }
            DbgScope::LexicalBlockFile {
                discriminator,
                loop_id,
                location,
            } => {
                write!(
                    f,
                    "LexicalBlockFile location=({}-{}) loop=({}-{}) discriminator={}",
                    location.package_id,
                    location.offset,
                    loop_id.package_id,
                    loop_id.expr_id,
                    discriminator
                )?;
            }
        }
        Ok(())
    }
}

/// Program debug metadata, including all debug locations and scopes.
#[derive(Default, Clone)]
pub struct DbgInfo {
    pub dbg_scopes: IndexMap<DbgScopeId, (DbgScope, bool)>,
    pub dbg_locations: IndexMap<DbgLocationId, (DbgLocation, bool)>,
    next_dbg_scope_id: usize,
    next_dbg_location_id: usize,
}

impl DbgInfo {
    #[must_use]
    pub fn get_location(&self, id: DbgLocationId) -> &DbgLocation {
        &self
            .dbg_locations
            .get(id)
            .expect("dbg location id should be in dbg info")
            .0
    }

    pub fn add_location(&mut self, location: DbgLocation) -> DbgLocationId {
        let id = DbgLocationId(self.next_dbg_location_id);
        self.next_dbg_location_id += 1;
        self.dbg_locations.insert(id, (location, false));
        id
    }

    #[must_use]
    pub fn get_scope(&self, id: DbgScopeId) -> &DbgScope {
        &self
            .dbg_scopes
            .get(id)
            .expect("dbg scope id should be in dbg info")
            .0
    }

    pub fn add_scope(&mut self, scope: DbgScope) -> DbgScopeId {
        let id = DbgScopeId(self.next_dbg_scope_id);
        self.next_dbg_scope_id += 1;
        self.dbg_scopes.insert(id, (scope, false));
        id
    }

    pub fn mark_location_used(&mut self, dbg_location: DbgLocationId) {
        let mut data = self
            .dbg_locations
            .get_mut(dbg_location)
            .expect("dbg location ID must exist");

        while !data.1 {
            data.1 = true;
            self.dbg_scopes
                .get_mut(data.0.scope)
                .expect("dbg scope id must exist")
                .1 = true;
            if let Some(inlined_at) = data.0.inlined_at {
                data = self
                    .dbg_locations
                    .get_mut(inlined_at)
                    .expect("dbg location id must exist");
            } else {
                break;
            }
        }
    }

    pub fn remove_unused_dbg_metadata(&mut self) {
        self.dbg_locations.retain(|_, (_, used)| *used);
        self.dbg_scopes.retain(|_, (_, used)| *used);
    }
}

impl Display for DbgInfo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if !self.dbg_scopes.is_empty() || !self.dbg_locations.is_empty() {
            let mut indent = set_indentation(indented(f), 0);
            write!(indent, "\ndbg_scopes:")?;
            indent = set_indentation(indent, 1);
            for (index, (scope, _used)) in self.dbg_scopes.iter() {
                write!(indent, "\n{index} = {scope}", index = index.0)?;
            }
            indent = set_indentation(indent, 0);
            write!(indent, "\ndbg_locations:")?;
            indent = set_indentation(indent, 1);
            for (index, (location, _used)) in self.dbg_locations.iter() {
                write!(indent, "\n[{index}]: {location}", index = index.0)?;
            }
        }
        Ok(())
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

impl From<DbgLocationId> for usize {
    fn from(id: DbgLocationId) -> Self {
        id.0
    }
}

impl From<usize> for DbgLocationId {
    fn from(value: usize) -> Self {
        DbgLocationId(value)
    }
}

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
/// Index into the `dbg_scopes` vector in `DbgInfo`.
pub struct DbgScopeId(usize);

impl From<DbgScopeId> for usize {
    fn from(id: DbgScopeId) -> Self {
        id.0
    }
}

impl From<usize> for DbgScopeId {
    fn from(value: usize) -> Self {
        DbgScopeId(value)
    }
}
