// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{MietteDiagnostic, diagnostic_from};
use miette::LabeledSpan;
use qdk_openqasm::io::InMemorySourceResolver;
use std::{error::Error, fmt};

#[derive(Debug)]
struct RelatedDiagnostic {
    offset: usize,
}

impl fmt::Display for RelatedDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("related diagnostic")
    }
}

impl Error for RelatedDiagnostic {}

impl MietteDiagnostic for RelatedDiagnostic {
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(std::iter::once(
            LabeledSpan::new_primary_with_span(None, (self.offset, 3)),
        )))
    }
}

#[derive(Debug)]
struct CrossSourceDiagnostic {
    entry_offset: usize,
    included_offset: usize,
    related: RelatedDiagnostic,
}

impl fmt::Display for CrossSourceDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("cross-source diagnostic")
    }
}

impl Error for CrossSourceDiagnostic {}

impl MietteDiagnostic for CrossSourceDiagnostic {
    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        Some(Box::new(
            [
                LabeledSpan::new_primary_with_span(None, (self.entry_offset, 8)),
                LabeledSpan::new_with_span(Some("included".into()), (self.included_offset, 3)),
            ]
            .into_iter(),
        ))
    }

    fn related<'a>(&'a self) -> Option<Box<dyn Iterator<Item = &'a dyn MietteDiagnostic> + 'a>> {
        Some(Box::new(std::iter::once(
            &self.related as &dyn MietteDiagnostic,
        )))
    }
}

#[test]
fn projects_cross_source_primary_and_related_labels() {
    let source = "OPENQASM 3.0; include \"child.inc\";";
    let mut resolver =
        InMemorySourceResolver::from_iter([("child.inc".into(), "int child = 1;".into())]);
    let result = qdk_openqasm::parse_source(source, "main.qasm", Some(&mut resolver));
    let included_offset = usize::try_from(result.source_snapshot.files()[1].offset)
        .expect("u32 source offset should fit into usize");
    let diagnostic = CrossSourceDiagnostic {
        entry_offset: 0,
        included_offset,
        related: RelatedDiagnostic {
            offset: included_offset + 4,
        },
    };

    let projected = diagnostic_from(&diagnostic, &result.source_snapshot);

    assert_eq!(projected.labels.len(), 2);
    assert_eq!(projected.related.len(), 1);
    assert_eq!(projected.related[0].labels.len(), 1);
    assert!(projected.rendered.contains("main.qasm"));
    assert!(projected.rendered.contains("child.inc"));
}
