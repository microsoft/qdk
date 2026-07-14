// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{SourceMap, longest_common_prefix};

#[test]
fn longest_common_prefix_preserves_separator_behavior() {
    let cases = [
        (&[][..], ""),
        (&["main.qasm"][..], ""),
        (&["src/main.qasm"][..], "src/"),
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
        assert_eq!(longest_common_prefix(sources), expected);
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
        assert_eq!(longest_common_prefix(sources), expected);
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
