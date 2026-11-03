// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Typed QDK snapshot framing and bounded encoding primitives.
//!
//! Domain crates own model conversion and verification. This crate knows the
//! wire schema, not circuit or graph runtime types.

pub mod binary;
pub mod text;

#[rustfmt::skip]
#[allow(clippy::all, clippy::pedantic, clippy::unwrap_used)]
pub mod snapshot_capnp;

use thiserror::Error;

pub const MODEL_VERSION: u32 = 1;
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_GRAPH_WORDS: usize = 8 * 1024 * 1024;
pub const MAX_DEPTH: usize = 128;
pub const MAX_WIRE_DEPTH: usize = 4 * MAX_DEPTH + 32;
pub const HEADER_LEN: usize = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Circuit,
    CircuitStore,
    Graph,
}

impl Kind {
    const fn magic(self) -> &'static [u8; 8] {
        match self {
            Self::Circuit => b"QDKCIRCT",
            Self::CircuitStore => b"QDKSTORE",
            Self::Graph => b"QDKQHDG!",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    Binary,
    Text,
}

impl Format {
    const fn tag(self) -> u8 {
        match self {
            Self::Binary => 1,
            Self::Text => b'(',
        }
    }
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum Error {
    #[error("snapshot exceeds the encoded byte limit")]
    ByteLimit,
    #[error("snapshot exceeds the expanded message limit")]
    ExpandedLimit,
    #[error("snapshot exceeds the logical item limit")]
    ItemLimit,
    #[error("snapshot exceeds the nesting limit")]
    DepthLimit,
    #[error("snapshot envelope has invalid magic or the wrong document kind")]
    Magic,
    #[error("snapshot envelope format {0} is unsupported")]
    Format(u8),
    #[error("snapshot envelope flags {0} are unsupported")]
    Flags(u8),
    #[error("snapshot model version {0} is unsupported")]
    ModelVersion(u32),
    #[error("snapshot is malformed: {0}")]
    Malformed(String),
}

impl Error {
    pub fn malformed(error: impl std::fmt::Display) -> Self {
        Self::Malformed(error.to_string())
    }
}

impl From<capnp::Error> for Error {
    fn from(error: capnp::Error) -> Self {
        Self::malformed(error)
    }
}

impl From<capnp::NotInSchema> for Error {
    fn from(error: capnp::NotInSchema) -> Self {
        Self::malformed(error)
    }
}

/// Budget charged before constructing domain records or reserving list storage.
/// Binary readers base it on decoded words, never on packed bytes.
pub struct Budget {
    remaining: usize,
}

impl Budget {
    #[must_use]
    pub const fn new(items: usize) -> Self {
        Self { remaining: items }
    }

    pub fn charge(&mut self, count: usize) -> Result<(), Error> {
        self.remaining = self.remaining.checked_sub(count).ok_or(Error::ItemLimit)?;
        Ok(())
    }
}

pub fn depth(depth: usize) -> Result<(), Error> {
    if depth > MAX_DEPTH {
        Err(Error::DepthLimit)
    } else {
        Ok(())
    }
}

pub fn length(len: usize) -> Result<u32, Error> {
    u32::try_from(len).map_err(Error::malformed)
}

#[must_use]
pub fn header(kind: Kind, format: Format) -> Vec<u8> {
    let mut bytes = kind.magic().to_vec();
    bytes.extend([format.tag(), b'\n']);
    bytes
}

pub fn payload(kind: Kind, bytes: &[u8]) -> Result<(Format, &[u8]), Error> {
    if kind == Kind::Graph && bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(Error::ByteLimit);
    }
    if bytes.len() < HEADER_LEN || &bytes[..8] != kind.magic() {
        return Err(Error::Magic);
    }
    if bytes[9] != b'\n' {
        return Err(Error::Flags(bytes[9]));
    }
    let format = match bytes[8] {
        1 => Format::Binary,
        b'(' => Format::Text,
        other => return Err(Error::Format(other)),
    };
    Ok((format, &bytes[HEADER_LEN..]))
}

pub fn check_kind(root: snapshot_capnp::document::Reader<'_>, kind: Kind) -> Result<(), Error> {
    use snapshot_capnp::document::Which;

    if root.get_version() != MODEL_VERSION {
        return Err(Error::ModelVersion(root.get_version()));
    }
    if !matches!(
        (kind, root.which()?),
        (Kind::Circuit, Which::Circuit(_))
            | (Kind::CircuitStore, Which::Store(_))
            | (Kind::Graph, Which::Graph(_))
    ) {
        return Err(Error::malformed("document kind does not match envelope"));
    }
    Ok(())
}

pub fn string(value: capnp::text::Reader<'_>) -> Result<String, Error> {
    value.to_str().map(str::to_owned).map_err(Error::malformed)
}

#[cfg(test)]
mod tests;
