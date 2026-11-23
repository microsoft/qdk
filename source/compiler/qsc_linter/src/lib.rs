// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! This crate contains the linter for the Q# language.
//!
//! It includes lints for the following stages of the compilation process:
//!  - AST
//!  - HIR
//!
//! # Usage
//!
//! The entry points to the linter is the `run_lints` function, which takes
//! a [`qsc_frontend::compile::CompileUnit`] and the capabilities of the target
//! it is compiled for as input, and outputs a [`Vec<Lint>`](Lint).
//!
//! ## Example
//!
//! ```
//! use linter::run_lints;;
//! use qsc::compile::compile;
//!
//! let unit: CompileUnit = compile(...);
//!
//! // The last argument is an optional user configuration.
//! let lints: Vec<Lint> = run_lints(&package_store, &unit, capabilities, None);
//! ```
//!
//! # How to add a new Lint
//!
//! We can add a new lint in two steps:
//!  1. Declaring the lint: here you set the lint name, the default [`LintLevel`], and the message the user will see.
//!  2. Implementing the lint: here you write the pattern matching logic of the new lint.
//!
//! Below is a full example of how to a new AST lint.
//!
//! ## Example
//!
//! First, we add our lint to `src/lints/ast.rs`.
//! ```
//! declare_ast_lints!{
//!   ...
//!   (DoubleParens, LintLevel::Warn, "unnecessary parentheses", "remove the extra parentheses for clarity"),
//! }
//! ```
//!
//! Then we implement the right `LintPass` for our new lint, in this case `linter::ast::AstLintPass`
//! ```
//! impl linter::ast::AstLintPass for DoubleParens {
//!     // we only need to impl the relevant check_* method, all the other ones
//!     // will default to an empty method that will get optimized by rust
//!     fn check_expr(expr: &qsc_ast::ast::Expr, buffer: &mut Vec<Lint>) {
//!         // we match the relevant pattern
//!         if let ExprKind::Paren(ref inner_expr) = *expr.kind {
//!             if matches!(*inner_expr.kind, ExprKind::Paren(_)) {
//!                 // we push the lint to the buffer
//!                 buffer.push(lint!(self, child.span))
//!             }
//!         }
//!     }
//! }
//! ```

#![deny(missing_docs)]

mod lint_groups;
mod linter;
mod lints;
#[cfg(test)]
mod tests;

pub use lint_groups::GroupConfig;
pub use linter::{Lint, LintConfig, LintKind, LintLevel, LintOrGroupConfig, run_lints};
pub use lints::{ast::AstLint, hir::HirLint};
