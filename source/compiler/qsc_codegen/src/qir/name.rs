// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Helpers for rendering LLVM global symbol names in valid textual IR.
//!
//! Reference: LLVM Language Reference, Identifiers
//! <https://llvm.org/docs/LangRef.html#identifiers>
//!
//! In particular, named identifiers use the regex `[%@][-a-zA-Z$._][-a-zA-Z$._0-9]*`.
//! Names that require other characters must be quoted, and special characters can
//! be escaped as `\xx` (hex ASCII byte).

#[cfg(test)]
mod tests;

use std::fmt::Write;

/// Formats an LLVM global symbol name for use in IR after `@`.
///
/// LLVM allows an unquoted identifier form for a restricted ASCII character set;
/// other names must be quoted (`@"..."`) with escaped bytes.
#[must_use]
pub fn llvm_global_name(name: &str) -> String {
    if is_unquoted_global_identifier(name) {
        format!("@{name}")
    } else {
        format!("@\"{}\"", escape_quoted_global_identifier(name))
    }
}

fn is_unquoted_global_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !matches!(first, 'A'..='Z' | 'a'..='z' | '$' | '.' | '_' | '-') {
        return false;
    }

    chars.all(|ch| matches!(ch, 'A'..='Z' | 'a'..='z' | '0'..='9' | '$' | '.' | '_' | '-'))
}

fn escape_quoted_global_identifier(name: &str) -> String {
    let mut escaped = String::new();
    for byte in name.bytes() {
        match byte {
            b'"' | b'\\' => {
                write!(&mut escaped, "\\{byte:02X}").expect("writing to string should succeed");
            }
            0x20..=0x7E => {
                escaped.push(char::from(byte));
            }
            _ => {
                write!(&mut escaped, "\\{byte:02X}").expect("writing to string should succeed");
            }
        }
    }
    escaped
}
