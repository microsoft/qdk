// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{
    compilation::{Compilation, CompilationKind},
    qsc_utils::into_range,
};
use qsc::{
    ast::visit::{Visitor, walk_namespace},
    line_column::{Encoding, Range},
};

/// Provides information about where auto-imports should be inserted
/// in the document based on the cursor offset.
pub(crate) struct TextEditRange {
    /// Location to insert any auto-import text edits at.
    pub insert_import_at: Option<Range>,
    /// The indentation level for the auto-import text edits.
    pub indent: String,
}

impl TextEditRange {
    pub fn init(offset: u32, compilation: &Compilation, position_encoding: Encoding) -> Self {
        let mut finder = StartOfNamespace {
            offset,
            start_of_namespace: None,
        };
        finder.visit_package(&compilation.user_unit().ast.package);

        let insert_open_at = match &compilation.kind {
            CompilationKind::OpenProject { .. } => finder.start_of_namespace,
            // Since notebooks don't typically contain namespace declarations,
            // open statements should just get before the first non-whitespace
            // character (i.e. at the top of the cell)
            CompilationKind::Notebook { .. } => Some(Self::get_first_non_whitespace_in_source(
                compilation,
                offset,
            )),
            CompilationKind::OpenQASM { .. } => {
                unreachable!("OpenQASM should not be used for Q# text edits")
            }
        };

        let indent = match insert_open_at {
            Some(start) => Self::get_indent(compilation, start),
            None => String::new(),
        };

        let insert_open_range = insert_open_at.map(|o| {
            into_range(
                position_encoding,
                qsc::Span { lo: o, hi: o },
                &compilation.user_unit().sources,
            )
        });

        TextEditRange {
            insert_import_at: insert_open_range,
            indent,
        }
    }

    fn get_first_non_whitespace_in_source(compilation: &Compilation, package_offset: u32) -> u32 {
        const QSHARP_MAGIC: &str = "//qsharp";
        let source = compilation
            .user_unit()
            .sources
            .find_by_offset(package_offset)
            .expect("source should exist in the user source map");

        // Skip the //qsharp magic if it exists (notebook cells)
        let start = if let Some(qsharp_magic_start) = source.contents.find(QSHARP_MAGIC) {
            qsharp_magic_start + QSHARP_MAGIC.len()
        } else {
            0
        };

        let source_after_magic = &source.contents[start..];

        let first = start
            + source_after_magic
                .find(|c: char| !c.is_whitespace())
                .unwrap_or(source_after_magic.len());

        let first = u32::try_from(first).expect("source length should fit into u32");

        source.offset + first
    }

    fn get_indent(compilation: &Compilation, package_offset: u32) -> String {
        let source = compilation
            .user_unit()
            .sources
            .find_by_offset(package_offset)
            .expect("source should exist in the user source map");
        let source_offset = (package_offset - source.offset)
            .try_into()
            .expect("offset can't be converted to uszie");
        let before_offset = &source.contents[..source_offset];
        let mut indent = match before_offset.rfind(['{', '\n']) {
            Some(begin) => {
                let indent = &before_offset[begin..];
                indent.strip_prefix('{').unwrap_or(indent)
            }
            None => before_offset,
        }
        .to_string();
        if !indent.starts_with('\n') {
            indent.insert(0, '\n');
        }
        indent
    }
}

/// Find the start of the namespace that contains the offset.
struct StartOfNamespace {
    offset: u32,
    start_of_namespace: Option<u32>,
}

impl<'a> Visitor<'a> for StartOfNamespace {
    fn visit_namespace(&mut self, namespace: &'a qsc::ast::Namespace) {
        if namespace.span.contains(self.offset) {
            self.start_of_namespace = None;
            walk_namespace(self, namespace);
        }
    }

    fn visit_item(&mut self, item: &'a qsc::ast::Item) {
        if self.start_of_namespace.is_none() {
            self.start_of_namespace = Some(item.span.lo);
        }
    }
}
