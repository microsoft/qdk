// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::*;

const CONTENTS: &str = "aé𝑓\r\n\nZ";

#[test]
fn every_character_boundary_round_trips_in_all_encodings() {
    for encoding in [
        PositionEncoding::Utf8,
        PositionEncoding::CodePoint,
        PositionEncoding::Utf16,
    ] {
        for byte_offset in CONTENTS
            .char_indices()
            .map(|(index, _)| index)
            .chain([CONTENTS.len()])
        {
            let byte_offset = u32::try_from(byte_offset).expect("test offset should fit");
            let position = position_at(CONTENTS, byte_offset, encoding)
                .expect("character boundary should convert");
            assert_eq!(
                byte_offset,
                super::byte_offset(CONTENTS, position).expect("valid position should convert")
            );
        }
    }
}

#[test]
fn positions_count_cr_but_only_lf_advances_the_line() {
    assert_eq!(
        position_at(CONTENTS, 8, PositionEncoding::CodePoint),
        Ok(Position {
            line: 0,
            column: 4,
            encoding: PositionEncoding::CodePoint,
        })
    );
    assert_eq!(
        position_at(CONTENTS, 9, PositionEncoding::CodePoint),
        Ok(Position {
            line: 1,
            column: 0,
            encoding: PositionEncoding::CodePoint,
        })
    );
    assert_eq!(
        position_at(CONTENTS, 10, PositionEncoding::CodePoint),
        Ok(Position {
            line: 2,
            column: 0,
            encoding: PositionEncoding::CodePoint,
        })
    );
}

#[test]
fn eof_is_valid_for_empty_and_nonempty_sources() {
    assert_eq!(
        position_at("", 0, PositionEncoding::Utf16),
        Ok(Position {
            line: 0,
            column: 0,
            encoding: PositionEncoding::Utf16,
        })
    );
    let eof =
        position_at(CONTENTS, 11, PositionEncoding::Utf16).expect("EOF should be a valid position");
    assert_eq!(byte_offset(CONTENTS, eof), Ok(11));
}

#[test]
fn invalid_byte_and_encoded_boundaries_fail_closed() {
    assert_eq!(
        position_at(CONTENTS, 2, PositionEncoding::CodePoint),
        Err(PositionError::InvalidByteOffset)
    );
    assert_eq!(
        position_at(CONTENTS, 13, PositionEncoding::CodePoint),
        Err(PositionError::InvalidByteOffset)
    );
    assert_eq!(
        byte_offset(
            CONTENTS,
            Position {
                line: 0,
                column: 3,
                encoding: PositionEncoding::Utf16,
            }
        ),
        Err(PositionError::InvalidPosition)
    );
    assert_eq!(
        byte_offset(
            CONTENTS,
            Position {
                line: 1,
                column: 1,
                encoding: PositionEncoding::CodePoint,
            }
        ),
        Err(PositionError::InvalidPosition)
    );
}

#[test]
fn ranges_reject_mixed_encodings_and_reversed_endpoints() {
    assert_eq!(
        range_from_span(CONTENTS, Span { lo: 4, hi: 3 }, PositionEncoding::Utf8),
        Err(PositionError::ReversedRange)
    );
    assert_eq!(
        span_from_range(
            CONTENTS,
            Range {
                start: Position {
                    line: 0,
                    column: 0,
                    encoding: PositionEncoding::Utf8,
                },
                end: Position {
                    line: 0,
                    column: 1,
                    encoding: PositionEncoding::CodePoint,
                },
            }
        ),
        Err(PositionError::MixedEncoding)
    );
    assert_eq!(
        span_from_range(
            CONTENTS,
            Range {
                start: Position {
                    line: 2,
                    column: 1,
                    encoding: PositionEncoding::CodePoint,
                },
                end: Position {
                    line: 0,
                    column: 0,
                    encoding: PositionEncoding::CodePoint,
                },
            }
        ),
        Err(PositionError::ReversedRange)
    );
}
