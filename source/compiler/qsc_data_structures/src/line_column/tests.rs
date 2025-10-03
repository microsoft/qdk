// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::span::Span;

use super::{Encoding, Position, Range};
use expect_test::expect;
use std::fmt::Write;

#[test]
fn empty_string() {
    let contents = "";
    let pos = Position::from_utf8_byte_offset(Encoding::Utf8, contents, 0);
    expect![[r"
        Position {
            line: 0,
            column: 0,
        }
    "]]
    .assert_debug_eq(&pos);
}

#[test]
fn offset_out_of_bounds() {
    let contents = "hello";
    let pos = Position::from_utf8_byte_offset(Encoding::Utf8, contents, 10);
    // Should return the <eof> position
    expect![[r"
        Position {
            line: 0,
            column: 5,
        }
    "]]
    .assert_debug_eq(&pos);
}

#[allow(clippy::cast_possible_truncation)]
#[test]
fn position_out_of_bounds() {
    let contents = "hello";
    // A position that is off range for the given string
    let pos = Position {
        line: 10,
        column: 10,
    };
    let offset = pos.to_utf8_byte_offset(Encoding::Utf8, contents);
    // Should return the <eof> offset
    assert!(offset == contents.len() as u32);
}

#[test]
fn one_line() {
    let contents = "Hello, world!";
    check_all_offsets(
        contents,
        &expect![[r"
            byte | utf-8 | utf-16 | char
               0 |  0, 0 |  0, 0  | 'H'
               1 |  0, 1 |  0, 1  | 'e'
               2 |  0, 2 |  0, 2  | 'l'
               3 |  0, 3 |  0, 3  | 'l'
               4 |  0, 4 |  0, 4  | 'o'
               5 |  0, 5 |  0, 5  | ','
               6 |  0, 6 |  0, 6  | ' '
               7 |  0, 7 |  0, 7  | 'w'
               8 |  0, 8 |  0, 8  | 'o'
               9 |  0, 9 |  0, 9  | 'r'
              10 |  0,10 |  0,10  | 'l'
              11 |  0,11 |  0,11  | 'd'
              12 |  0,12 |  0,12  | '!'
              13 |  0,13 |  0,13  | <eof>
        "]],
    );
}

#[test]
fn lines() {
    let contents = "line1\nline2\nline3";
    check_all_offsets(
        contents,
        &expect![[r"
            byte | utf-8 | utf-16 | char
               0 |  0, 0 |  0, 0  | 'l'
               1 |  0, 1 |  0, 1  | 'i'
               2 |  0, 2 |  0, 2  | 'n'
               3 |  0, 3 |  0, 3  | 'e'
               4 |  0, 4 |  0, 4  | '1'
               5 |  0, 5 |  0, 5  | '\n'
               6 |  1, 0 |  1, 0  | 'l'
               7 |  1, 1 |  1, 1  | 'i'
               8 |  1, 2 |  1, 2  | 'n'
               9 |  1, 3 |  1, 3  | 'e'
              10 |  1, 4 |  1, 4  | '2'
              11 |  1, 5 |  1, 5  | '\n'
              12 |  2, 0 |  2, 0  | 'l'
              13 |  2, 1 |  2, 1  | 'i'
              14 |  2, 2 |  2, 2  | 'n'
              15 |  2, 3 |  2, 3  | 'e'
              16 |  2, 4 |  2, 4  | '3'
              17 |  2, 5 |  2, 5  | <eof>
        "]],
    );
}

#[test]
fn newline_at_end() {
    let contents = "Hello, world!\n";
    check_all_offsets(
        contents,
        &expect![[r"
            byte | utf-8 | utf-16 | char
               0 |  0, 0 |  0, 0  | 'H'
               1 |  0, 1 |  0, 1  | 'e'
               2 |  0, 2 |  0, 2  | 'l'
               3 |  0, 3 |  0, 3  | 'l'
               4 |  0, 4 |  0, 4  | 'o'
               5 |  0, 5 |  0, 5  | ','
               6 |  0, 6 |  0, 6  | ' '
               7 |  0, 7 |  0, 7  | 'w'
               8 |  0, 8 |  0, 8  | 'o'
               9 |  0, 9 |  0, 9  | 'r'
              10 |  0,10 |  0,10  | 'l'
              11 |  0,11 |  0,11  | 'd'
              12 |  0,12 |  0,12  | '!'
              13 |  0,13 |  0,13  | '\n'
              14 |  1, 0 |  1, 0  | <eof>
        "]],
    );
}

#[test]
fn windows_crlf_line_breaks() {
    let contents = "line1\r\nline2\r\n";
    check_all_offsets(
        contents,
        &expect![[r"
        byte | utf-8 | utf-16 | char
           0 |  0, 0 |  0, 0  | 'l'
           1 |  0, 1 |  0, 1  | 'i'
           2 |  0, 2 |  0, 2  | 'n'
           3 |  0, 3 |  0, 3  | 'e'
           4 |  0, 4 |  0, 4  | '1'
           5 |  0, 5 |  0, 5  | '\r'
           6 |  0, 6 |  0, 6  | '\n'
           7 |  1, 0 |  1, 0  | 'l'
           8 |  1, 1 |  1, 1  | 'i'
           9 |  1, 2 |  1, 2  | 'n'
          10 |  1, 3 |  1, 3  | 'e'
          11 |  1, 4 |  1, 4  | '2'
          12 |  1, 5 |  1, 5  | '\r'
          13 |  1, 6 |  1, 6  | '\n'
          14 |  2, 0 |  2, 0  | <eof>
    "]],
    );
}

#[test]
fn utf_8_multibyte() {
    // utf-8 encoding has multi-unit characters, utf-16 doesn't
    // string       | ççç
    // chars        | ç        ç        ç
    // code points  | e7       e7       e7
    // utf-8 units  | c3a7     c3a7     c3a7
    // utf-16 units | 00e7     00e7     00e7
    let contents = "ççç\nççç";
    check_all_offsets(
        contents,
        &expect![[r"
            byte | utf-8 | utf-16 | char
               0 |  0, 0 |  0, 0  | 'ç'
               1 |  0, 2 |  0, 1  |
               2 |  0, 2 |  0, 1  | 'ç'
               3 |  0, 4 |  0, 2  |
               4 |  0, 4 |  0, 2  | 'ç'
               5 |  0, 6 |  0, 3  |
               6 |  0, 6 |  0, 3  | '\n'
               7 |  1, 0 |  1, 0  | 'ç'
               8 |  1, 2 |  1, 1  |
               9 |  1, 2 |  1, 1  | 'ç'
              10 |  1, 4 |  1, 2  |
              11 |  1, 4 |  1, 2  | 'ç'
              12 |  1, 6 |  1, 3  |
              13 |  1, 6 |  1, 3  | <eof>
        "]],
    );
}

#[test]
fn utf_8_multibyte_utf_16_surrogate() {
    // both encodings have multi-unit characters
    // string       | 𝑓𝑓
    // chars        | 𝑓                 𝑓
    // code points  | 1d453             1d453
    // utf-8 units  | f09d9193          f09d9193
    // utf-16 units | d835     dc53     d835     dc53

    let contents = "𝑓𝑓\n𝑓𝑓";
    check_all_offsets(
        contents,
        &expect![[r"
            byte | utf-8 | utf-16 | char
               0 |  0, 0 |  0, 0  | '𝑓'
               1 |  0, 4 |  0, 2  |
               2 |  0, 4 |  0, 2  |
               3 |  0, 4 |  0, 2  |
               4 |  0, 4 |  0, 2  | '𝑓'
               5 |  0, 8 |  0, 4  |
               6 |  0, 8 |  0, 4  |
               7 |  0, 8 |  0, 4  |
               8 |  0, 8 |  0, 4  | '\n'
               9 |  1, 0 |  1, 0  | '𝑓'
              10 |  1, 4 |  1, 2  |
              11 |  1, 4 |  1, 2  |
              12 |  1, 4 |  1, 2  |
              13 |  1, 4 |  1, 2  | '𝑓'
              14 |  1, 8 |  1, 4  |
              15 |  1, 8 |  1, 4  |
              16 |  1, 8 |  1, 4  |
              17 |  1, 8 |  1, 4  | <eof>
        "]],
    );
}

#[test]
fn grapheme_clusters() {
    // grapheme clusters, both encodings have multi-unit characters
    // string       | 𝑓(𝑥⃗) ≔ Σᵢ 𝑥ᵢ 𝑟ᵢ
    // chars        | 𝑓                 (        𝑥                 ⃗        )                 ≔                 Σ        ᵢ                 𝑥                 ᵢ                 𝑟                 ᵢ
    // code points  | 1d453             28       1d465             20d7     29       20       2254     20       3a3      1d62     20       1d465             1d62     20       1d45f             1d62
    // utf-8 units  | f09d9193          28       f09d91a5          e28397   29       20       e28994   20       cea3     e1b5a2   20       f09d91a5          e1b5a2   20       f09d919f          e1b5a2
    // utf-16 units | d835     dc53     0028     d835     dc65     20d7     0029     0020     2254     0020     03a3     1d62     0020     d835     dc65     1d62     0020     d835     dc5f     1d62

    let contents = "𝑓(𝑥⃗) ≔ Σᵢ 𝑥ᵢ 𝑟ᵢ";
    check_all_offsets(
        contents,
        &expect![[r"
            byte | utf-8 | utf-16 | char
               0 |  0, 0 |  0, 0  | '𝑓'
               1 |  0, 4 |  0, 2  |
               2 |  0, 4 |  0, 2  |
               3 |  0, 4 |  0, 2  |
               4 |  0, 4 |  0, 2  | '('
               5 |  0, 5 |  0, 3  | '𝑥'
               6 |  0, 9 |  0, 5  |
               7 |  0, 9 |  0, 5  |
               8 |  0, 9 |  0, 5  |
               9 |  0, 9 |  0, 5  | '\u{20d7}'
              10 |  0,12 |  0, 6  |
              11 |  0,12 |  0, 6  |
              12 |  0,12 |  0, 6  | ')'
              13 |  0,13 |  0, 7  | ' '
              14 |  0,14 |  0, 8  | '≔'
              15 |  0,17 |  0, 9  |
              16 |  0,17 |  0, 9  |
              17 |  0,17 |  0, 9  | ' '
              18 |  0,18 |  0,10  | 'Σ'
              19 |  0,20 |  0,11  |
              20 |  0,20 |  0,11  | 'ᵢ'
              21 |  0,23 |  0,12  |
              22 |  0,23 |  0,12  |
              23 |  0,23 |  0,12  | ' '
              24 |  0,24 |  0,13  | '𝑥'
              25 |  0,28 |  0,15  |
              26 |  0,28 |  0,15  |
              27 |  0,28 |  0,15  |
              28 |  0,28 |  0,15  | 'ᵢ'
              29 |  0,31 |  0,16  |
              30 |  0,31 |  0,16  |
              31 |  0,31 |  0,16  | ' '
              32 |  0,32 |  0,17  | '𝑟'
              33 |  0,36 |  0,19  |
              34 |  0,36 |  0,19  |
              35 |  0,36 |  0,19  |
              36 |  0,36 |  0,19  | 'ᵢ'
              37 |  0,39 |  0,20  |
              38 |  0,39 |  0,20  |
              39 |  0,39 |  0,20  | <eof>
        "]],
    );
}

#[test]
fn empty_range() {
    let contents = "hello";
    let span = Span { lo: 1, hi: 1 };
    let range = Range::from_span(Encoding::Utf8, contents, &span);
    expect![[r"
        Range {
            start: Position {
                line: 0,
                column: 1,
            },
            end: Position {
                line: 0,
                column: 1,
            },
        }
    "]]
    .assert_debug_eq(&range);
}

#[test]
fn range_across_lines() {
    let contents = "line1\nline2";
    let span = Span { lo: 0, hi: 10 };
    let range = Range::from_span(Encoding::Utf8, contents, &span);
    expect![[r"
        Range {
            start: Position {
                line: 0,
                column: 0,
            },
            end: Position {
                line: 1,
                column: 4,
            },
        }
    "]]
    .assert_debug_eq(&range);
}

#[test]
fn range_out_of_bounds() {
    let contents = "hello";
    let span = Span { lo: 6, hi: 10 };
    let range = Range::from_span(Encoding::Utf8, contents, &span);
    expect![[r"
        Range {
            start: Position {
                line: 0,
                column: 5,
            },
            end: Position {
                line: 0,
                column: 5,
            },
        }
    "]]
    .assert_debug_eq(&range);
}

#[allow(clippy::cast_possible_truncation)]
fn check_all_offsets(contents: &str, expected: &expect_test::Expect) {
    let byte_offsets = 0..=contents.len();
    let positions = byte_offsets
        .map(|offset| {
            (
                offset,
                Position::from_utf8_byte_offset(
                    Encoding::Utf8,
                    contents,
                    u32::try_from(offset).expect("offset should fit in u32"),
                ),
                Position::from_utf8_byte_offset(
                    Encoding::Utf16,
                    contents,
                    u32::try_from(offset).expect("offset should fit in u32"),
                ),
            )
        })
        .collect::<Vec<_>>();

    // Generate a table for visual validation
    let mut string = String::new();
    let _ = writeln!(string, "byte | utf-8 | utf-16 | char");
    for (offset, utf8pos, utf16pos) in &positions {
        let char = if *offset == contents.len() {
            " <eof>".to_string()
        } else {
            contents
                .char_indices()
                .find_map(|(i, c)| {
                    if i == *offset {
                        Some(format!(" {c:?}"))
                    } else {
                        None
                    }
                })
                .unwrap_or(String::new())
        };

        let _ = writeln!(
            string,
            "{offset: >4} | {: >2},{: >2} | {: >2},{: >2}  |{}",
            utf8pos.line, utf8pos.column, utf16pos.line, utf16pos.column, char
        );
    }

    expected.assert_eq(&string);

    // also validate that we correctly map back to the original utf-8 byte offset
    for (offset, utf8pos, utf16pos) in positions {
        if contents.is_char_boundary(offset) {
            assert!(utf8pos.to_utf8_byte_offset(Encoding::Utf8, contents) == offset as u32);
            assert!(utf16pos.to_utf8_byte_offset(Encoding::Utf16, contents) == offset as u32);
        }
    }
}
