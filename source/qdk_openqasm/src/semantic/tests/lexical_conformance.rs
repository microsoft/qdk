// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use crate::{
    io::InMemorySourceResolver,
    semantic::{ast::StmtKind, parse_source},
};

#[test]
fn lexical_errors_recover_to_following_semantic_statement() {
    for malformed in ["int bad = 1__2;", "bit[4] bad = \"1__0\";", "int π٢ = 0;"] {
        let source: Arc<str> = format!("{malformed}\nint good = 1;").into();
        let good_offset = u32::try_from(source.find("int good").expect("sentinel statement"))
            .expect("source offset should fit into u32");
        let mut resolver = InMemorySourceResolver::from_iter([("test".into(), source.clone())]);
        let result = parse_source(source, "test", &mut resolver);

        assert!(result.has_syntax_errors(), "source: {malformed:?}");
        assert!(
            result.program.statements.iter().any(|statement| {
                statement.span.lo >= good_offset
                    && matches!(statement.kind.as_ref(), StmtKind::ClassicalDecl(_))
            }),
            "source: {malformed:?}"
        );
    }
}

#[test]
fn invalid_strings_are_semantic_syntax_errors() {
    for source in ["include \"\";", "include \"line\nbreak\";"] {
        let source: Arc<str> = source.into();
        let mut resolver = InMemorySourceResolver::from_iter([("test".into(), source.clone())]);
        let result = parse_source(source, "test", &mut resolver);

        assert!(result.has_syntax_errors());
    }
}

#[test]
fn unterminated_block_comment_is_a_semantic_syntax_error() {
    let source: Arc<str> = "/* unterminated".into();
    let mut resolver = InMemorySourceResolver::from_iter([("test".into(), source.clone())]);
    let result = parse_source(source, "test", &mut resolver);

    assert!(result.has_syntax_errors());
}

#[test]
fn neighboring_valid_lexical_forms_lower() {
    let source: Arc<str> =
        "const int decimal = 1_2; const int octal = 0o7; const int octal_cap = 0O7; int π2 = 0;"
            .into();
    let mut resolver = InMemorySourceResolver::from_iter([("test".into(), source.clone())]);
    let result = parse_source(source, "test", &mut resolver);

    assert!(!result.has_errors(), "errors: {:?}", result.all_errors());
    assert_eq!(result.program.statements.len(), 4);
}
