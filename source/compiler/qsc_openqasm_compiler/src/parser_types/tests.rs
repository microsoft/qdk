// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::to_qsharp_source_map;
use qdk_openqasm::source::SourceMap as ParserSourceMap;

#[test]
fn source_map_conversion_preserves_offsets() {
    let parser_map = ParserSourceMap::new(
        [
            ("one.qasm".into(), "first".into()),
            ("two.qasm".into(), "second".into()),
        ],
        Some("entry".into()),
    );

    let qsharp_map = to_qsharp_source_map(&parser_map);
    let parser_sources = parser_map.iter().collect::<Vec<_>>();
    let qsharp_sources = qsharp_map.iter().collect::<Vec<_>>();

    assert_eq!(
        parser_map.entry().map(|source| source.offset),
        qsharp_map.entry().map(|source| source.offset)
    );
    assert_eq!(
        parser_sources
            .iter()
            .map(|source| source.offset)
            .collect::<Vec<_>>(),
        qsharp_sources
            .iter()
            .map(|source| source.offset)
            .collect::<Vec<_>>()
    );
}
