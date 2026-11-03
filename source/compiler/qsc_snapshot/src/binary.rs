// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use capnp::{
    message::{Builder, HeapAllocator, Reader, ReaderOptions},
    serialize::OwnedSegments,
    traits::{Owned, SetterInput},
};

use crate::{Budget, Error, Format, Kind, MAX_GRAPH_WORDS, MAX_WIRE_DEPTH, check_kind, header};

pub type Message = Builder<HeapAllocator>;

/// A canonical single-segment message. Hash its output segment directly;
/// packing and the stream's segment table are not part of a payload digest.
pub fn canonical<'a, T: Owned>(
    root: impl SetterInput<T> + Copy + Into<capnp::dynamic_value::Reader<'a>>,
) -> Result<Message, Error> {
    let capnp::dynamic_value::Reader::Struct(structure) = root.into() else {
        return Err(Error::malformed(
            "canonical payload root must be a typed record",
        ));
    };
    let words = structure
        .total_size()?
        .word_count
        .checked_add(1)
        .ok_or(Error::ExpandedLimit)?;
    let capacity = u32::try_from(words).map_err(Error::malformed)?;
    let mut canonical = Message::new(HeapAllocator::new().first_segment_words(capacity));
    canonical.set_root_canonical(root)?;
    Ok(canonical)
}

pub fn encode(kind: Kind, message: &Message) -> Result<Vec<u8>, Error> {
    let root = message.get_root_as_reader::<crate::snapshot_capnp::document::Reader>()?;
    check_kind(root, kind)?;
    let normalized = canonical::<crate::snapshot_capnp::document::Owned>(root)?;
    let mut bytes = header(kind, Format::Binary);
    capnp::serialize_packed::write_message(&mut bytes, &normalized)?;
    Ok(bytes)
}

/// Reads one packed message, bounding segment allocation before materializing
/// it, and then installs traversal/item budgets based on actual decoded words.
pub fn decode(kind: Kind, bytes: &[u8]) -> Result<(Reader<OwnedSegments>, Budget), Error> {
    // A two-byte zero run expands to at most 256 eight-byte words.
    let maximum_words = if kind == Kind::Graph {
        MAX_GRAPH_WORDS
    } else {
        bytes.len().checked_mul(128).ok_or(Error::ExpandedLimit)?
    };
    let options = ReaderOptions {
        traversal_limit_in_words: Some(maximum_words),
        nesting_limit: i32::try_from(MAX_WIRE_DEPTH).expect("constant fits i32"),
    };
    let mut input = bytes;
    let decoded = capnp::serialize_packed::read_message(&mut input, options)?;
    if !input.is_empty() {
        return Err(Error::malformed("trailing binary data"));
    }
    let words = decoded.size_in_words();
    if words > maximum_words {
        return Err(Error::ExpandedLimit);
    }
    // Covers domain conversion, canonical projection checks and digest passes.
    // A logical zero-size element still consumes at least one item credit.
    let budget = words.checked_mul(16).ok_or(Error::ItemLimit)?;
    let reader = Reader::new(
        decoded.into_segments(),
        ReaderOptions {
            traversal_limit_in_words: Some(budget),
            ..options
        },
    );
    check_kind(reader.get_root()?, kind)?;
    Ok((reader, Budget::new(budget)))
}

/// Detects non-default fields that a current typed projection would discard.
/// Both messages may have different physical segment/allocation layouts.
pub fn check_projection<'a, 'b, T: Owned>(
    source: impl SetterInput<T> + Copy + Into<capnp::dynamic_value::Reader<'a>>,
    projected: impl SetterInput<T> + Copy + Into<capnp::dynamic_value::Reader<'b>>,
) -> Result<(), Error> {
    let source = canonical::<T>(source)?;
    let projected = canonical::<T>(projected)?;
    if source.get_segments_for_output()[0] != projected.get_segments_for_output()[0] {
        return Err(Error::malformed(
            "unsupported or noncanonical semantic fields",
        ));
    }
    Ok(())
}
