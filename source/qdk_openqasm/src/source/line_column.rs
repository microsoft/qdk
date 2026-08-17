// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::span::Span;
use thiserror::Error;

/// The encoding used to count columns within a line.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PositionEncoding {
    Utf8,
    CodePoint,
    Utf16,
}

/// A zero-based, encoding-aware position within one source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Position {
    pub line: u32,
    pub column: u32,
    pub encoding: PositionEncoding,
}

/// A half-open range within one source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

/// An invalid source coordinate or range.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum PositionError {
    #[error("byte offset is outside the source or is not a UTF-8 character boundary")]
    InvalidByteOffset,
    #[error("position is outside the source or is not an encoded character boundary")]
    InvalidPosition,
    #[error("range endpoints use different position encodings")]
    MixedEncoding,
    #[error("range end precedes range start")]
    ReversedRange,
}

/// Converts a source-local UTF-8 byte offset to a position.
pub fn position_at(
    contents: &str,
    byte_offset: u32,
    encoding: PositionEncoding,
) -> Result<Position, PositionError> {
    let byte_offset = usize::try_from(byte_offset).map_err(|_| PositionError::InvalidByteOffset)?;
    if byte_offset > contents.len() || !contents.is_char_boundary(byte_offset) {
        return Err(PositionError::InvalidByteOffset);
    }

    let mut line = 0_u32;
    let mut column = 0_u32;
    for (index, character) in contents.char_indices() {
        if index == byte_offset {
            break;
        }
        advance(&mut line, &mut column, character, encoding);
    }

    Ok(Position {
        line,
        column,
        encoding,
    })
}

/// Converts an encoding-aware position to a source-local UTF-8 byte offset.
pub fn byte_offset(contents: &str, position: Position) -> Result<u32, PositionError> {
    let mut line = 0_u32;
    let mut column = 0_u32;

    for (index, character) in contents.char_indices() {
        if line == position.line && column == position.column {
            return u32::try_from(index).map_err(|_| PositionError::InvalidPosition);
        }
        if line > position.line || (line == position.line && column > position.column) {
            return Err(PositionError::InvalidPosition);
        }
        advance(&mut line, &mut column, character, position.encoding);
    }

    if line == position.line && column == position.column {
        u32::try_from(contents.len()).map_err(|_| PositionError::InvalidPosition)
    } else {
        Err(PositionError::InvalidPosition)
    }
}

/// Converts a source-local UTF-8 byte span to an encoding-aware range.
pub fn range_from_span(
    contents: &str,
    span: Span,
    encoding: PositionEncoding,
) -> Result<Range, PositionError> {
    if span.hi < span.lo {
        return Err(PositionError::ReversedRange);
    }

    Ok(Range {
        start: position_at(contents, span.lo, encoding)?,
        end: position_at(contents, span.hi, encoding)?,
    })
}

/// Converts an encoding-aware range to a source-local UTF-8 byte span.
pub fn span_from_range(contents: &str, range: Range) -> Result<Span, PositionError> {
    if range.start.encoding != range.end.encoding {
        return Err(PositionError::MixedEncoding);
    }

    let lo = byte_offset(contents, range.start)?;
    let hi = byte_offset(contents, range.end)?;
    if hi < lo {
        return Err(PositionError::ReversedRange);
    }

    Ok(Span { lo, hi })
}

fn advance(line: &mut u32, column: &mut u32, character: char, encoding: PositionEncoding) {
    if character == '\n' {
        *line += 1;
        *column = 0;
    } else {
        *column +=
            match encoding {
                PositionEncoding::Utf8 => u32::try_from(character.len_utf8())
                    .expect("a UTF-8 character width fits in u32"),
                PositionEncoding::CodePoint => 1,
                PositionEncoding::Utf16 => u32::try_from(character.len_utf16())
                    .expect("a UTF-16 character width fits in u32"),
            };
    }
}
