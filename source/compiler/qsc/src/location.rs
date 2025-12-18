// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use qsc_data_structures::{
    line_column::{Encoding, Range},
    span::Span,
};
use qsc_frontend::compile::PackageStore;
use qsc_hir::hir::PackageId;

/// Describes a location in source code in terms of a source name and [`Range`].
#[derive(Debug, PartialEq, Clone)]
pub struct Location {
    pub source: Arc<str>,
    pub range: Range,
}

impl Location {
    /// Creates a [`Location`] from a package ID and a SourceMap-relative span.
    ///
    /// To differentiate user sources from library sources, this function takes
    /// a `user_package_id` parameter which denotes the user package.
    /// All other packages in the package store are assumed to be library packages.
    /// Source names from library packages are prepended with a unique URI scheme.
    #[must_use]
    pub fn from(
        span: Span,
        package_id: PackageId,
        package_store: &PackageStore,
        position_encoding: Encoding,
    ) -> Self {
        let source = package_store
            .get(package_id)
            .expect("package id must exist in store")
            .sources
            .find_by_offset(span.lo)
            .expect("source should exist for offset");

        Location {
            source: source.name.clone(),
            range: Range::from_span(position_encoding, &source.contents, &(span - source.offset)),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::compile;
    use expect_test::expect;
    use qsc_data_structures::{
        language_features::LanguageFeatures, line_column::Encoding, source::SourceMap, span::Span,
        target::TargetCapabilityFlags,
    };
    use qsc_frontend::compile::PackageStore;
    use qsc_hir::hir::PackageId;
    use qsc_passes::PackageType;

    use super::Location;

    #[test]
    fn from_std_span() {
        let (store, std_package_id, _) = compile_package();

        let location = Location::from(
            Span { lo: 0, hi: 1 },
            std_package_id,
            &store,
            Encoding::Utf8,
        );

        expect![[r#"
            Location {
                source: "qsharp-library-source:Std/Arrays.qs",
                range: Range {
                    start: Position {
                        line: 0,
                        column: 0,
                    },
                    end: Position {
                        line: 0,
                        column: 1,
                    },
                },
            }
        "#]]
        .assert_debug_eq(&location);
    }

    #[test]
    fn from_core_span() {
        let (store, _, _) = compile_package();

        let location = Location::from(
            Span { lo: 0, hi: 1 },
            PackageId::CORE,
            &store,
            Encoding::Utf8,
        );

        expect![[r#"
            Location {
                source: "qsharp-library-source:core/core.qs",
                range: Range {
                    start: Position {
                        line: 0,
                        column: 0,
                    },
                    end: Position {
                        line: 0,
                        column: 1,
                    },
                },
            }
        "#]]
        .assert_debug_eq(&location);
    }

    #[test]
    fn from_user_span() {
        let (store, _, user_package_id) = compile_package();

        let bar_start_offset = store
            .get(user_package_id)
            .expect("expected to find user package")
            .sources
            .find_by_name("bar.qs")
            .expect("expected to find source")
            .offset;

        let location = Location::from(
            Span {
                lo: bar_start_offset,
                hi: bar_start_offset + 1,
            },
            user_package_id,
            &store,
            Encoding::Utf8,
        );

        expect![[r#"
            Location {
                source: "bar.qs",
                range: Range {
                    start: Position {
                        line: 0,
                        column: 0,
                    },
                    end: Position {
                        line: 0,
                        column: 1,
                    },
                },
            }
        "#]]
        .assert_debug_eq(&location);
    }

    #[test]
    fn from_out_of_bounds_lo() {
        let (store, _, user_package_id) = compile_package();

        let location = Location::from(
            Span { lo: 1000, hi: 2000 },
            user_package_id,
            &store,
            Encoding::Utf8,
        );

        // Per [`Range`] spec, out of bounds positions map to EOF
        expect![[r#"
            Location {
                source: "bar.qs",
                range: Range {
                    start: Position {
                        line: 0,
                        column: 17,
                    },
                    end: Position {
                        line: 0,
                        column: 17,
                    },
                },
            }
        "#]]
        .assert_debug_eq(&location);
    }

    #[test]
    fn from_out_of_bounds_hi() {
        let (store, _, user_package_id) = compile_package();

        let location = Location::from(
            Span { lo: 0, hi: 2000 },
            user_package_id,
            &store,
            Encoding::Utf8,
        );

        // Per [`Range`] spec, out of bounds positions map to EOF
        expect![[r#"
            Location {
                source: "foo.qs",
                range: Range {
                    start: Position {
                        line: 0,
                        column: 0,
                    },
                    end: Position {
                        line: 0,
                        column: 17,
                    },
                },
            }
        "#]]
        .assert_debug_eq(&location);
    }

    fn compile_package() -> (PackageStore, PackageId, PackageId) {
        let mut store = PackageStore::new(compile::core());
        let mut dependencies = Vec::new();

        let (package_type, capabilities) = (PackageType::Lib, TargetCapabilityFlags::all());

        let std_package_id = store.new_package_id();
        let std = compile::std(std_package_id, &store, capabilities);
        store.insert(std_package_id, std);

        dependencies.push((std_package_id, None));
        let sources = SourceMap::new(
            [
                ("foo.qs".into(), "namespace Foo { }".into()),
                ("bar.qs".into(), "namespace Bar { }".into()),
            ],
            None,
        );
        let user_package_id = store.new_package_id();
        let (unit, _) = compile::compile(
            &store,
            &dependencies,
            sources,
            package_type,
            user_package_id,
            capabilities,
            LanguageFeatures::default(),
        );
        store.insert(user_package_id, unit);

        (store, std_package_id, user_package_id)
    }
}
