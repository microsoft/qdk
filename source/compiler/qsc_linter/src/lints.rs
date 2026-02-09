// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub(super) mod ast;
pub(super) mod hir;

macro_rules! lint {
    ($lint:expr, $span:expr) => {
        Lint {
            span: $span,
            level: $lint.level,
            message: $lint.message(),
            help: $lint.help(),
            kind: $lint.lint_kind(),
            code_action_edits: vec![],
            code_action_title: None,
        }
    };
    ($lint:expr, $span:expr, $code_action_edits:expr) => {
        Lint {
            span: $span,
            level: $lint.level,
            message: $lint.message(),
            help: $lint.help(),
            kind: $lint.lint_kind(),
            code_action_edits: $code_action_edits,
            code_action_title: None,
        }
    };
    ($lint:expr, $span:expr, $code_action_edits:expr, $code_action_title:expr) => {
        Lint {
            span: $span,
            level: $lint.level,
            message: $lint.message(),
            help: $lint.help(),
            kind: $lint.lint_kind(),
            code_action_edits: $code_action_edits,
            code_action_title: $code_action_title,
        }
    };
}

pub(crate) use lint;
