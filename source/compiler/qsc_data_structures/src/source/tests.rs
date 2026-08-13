// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{SourceMap, longest_common_folder_prefix};

#[test]
fn longest_common_prefix_preserves_separator_behavior() {
    let cases = [
        (&[][..], ""),
        (&["main.qasm"][..], ""),
        (&["src/main.qasm"][..], "src/"),
        // The root of a file URI is the third slash, not the scheme.
        (&["file:///a.qasm", "file:///b.qasm"][..], "file:///"),
        (
            &["/project/src/a.qasm", "/project/src/b.qasm"][..],
            "/project/src/",
        ),
        (
            &[r"C:\project\src\a.qasm", r"C:\project\src\b.qasm"][..],
            r"C:\project\src\",
        ),
        (
            &["file:///project/a.qasm", "file:///project/b.qasm"][..],
            "file:///project/",
        ),
        (&["C:project/a.qasm", "C:project/b.qasm"][..], "C:project/"),
        (&["alpha.qasm", "beta.qasm"][..], ""),
        (&["same/path.qasm", "same/path.qasm"][..], "same/path.qasm"),
        (&["short/path", "short/path/longer"][..], "short/path"),
    ];

    for (sources, expected) in cases {
        assert_eq!(longest_common_folder_prefix(sources), expected);
    }
}

#[test]
fn longest_common_prefix_truncates_at_the_last_separator_of_either_kind() {
    let cases = [
        (&[r"src/dir\a.qasm"][..], r"src/dir\"),
        (&[r"src/dir\a.qasm", r"src/dir\b.qasm"][..], r"src/dir\"),
        (
            &[r"C:\proj/sub\a.qasm", r"C:\proj/sub\b.qasm"][..],
            r"C:\proj/sub\",
        ),
        (
            &[r"file:///c:/proj\a.qasm", r"file:///c:/proj\b.qasm"][..],
            r"file:///c:/proj\",
        ),
        // A `:` bounds the path only when neither separator is present.
        (&["C:a.qasm", "C:b.qasm"][..], "C:"),
    ];

    for (sources, expected) in cases {
        assert_eq!(longest_common_folder_prefix(sources), expected);
    }
}

#[test]
fn longest_common_prefix_handles_multibyte_boundaries() {
    let cases = [
        (&["/项目/源/a.qasm", "/项目/源/b.qasm"][..], "/项目/源/"),
        (&["/项目/甲.qasm", "/项目/乙.qasm"][..], "/项目/"),
        (&["项目甲.qasm", "项目乙.qasm"][..], ""),
        (
            &["file:///项目/a.qasm", "file:///项目/b.qasm"][..],
            "file:///项目/",
        ),
    ];

    for (sources, expected) in cases {
        assert_eq!(longest_common_folder_prefix(sources), expected);
    }
}

#[test]
fn find_by_offset_rejects_offsets_past_source_end() {
    let source_map = SourceMap::new([("main.qasm".into(), "".into())], None);

    assert_eq!(
        source_map
            .find_by_offset(0)
            .map(|source| source.name.as_ref()),
        Some("main.qasm")
    );
    assert!(source_map.find_by_offset(1).is_none());
}
