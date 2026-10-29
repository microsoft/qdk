// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The REPL's colors. Rendering always adds them; `anstream` strips them when
//! the output is not a color terminal, so colors only repeat what the text
//! already says.

use anstream::adapter::strip_str;
use anstyle::{AnsiColor, Style};

const fn fg(color: AnsiColor) -> Style {
    Style::new().fg_color(Some(anstyle::Color::Ansi(color)))
}

/// A mnemonic.
pub const MNEMONIC: Style = fg(AnsiColor::Blue).bold();
/// A block id, or a block operand in a signature.
pub const BLOCK: Style = fg(AnsiColor::Green);
/// A parameter name.
pub const PARAM: Style = fg(AnsiColor::Yellow);
/// An argument value, or a parameter kind in a signature.
pub const VALUE: Style = fg(AnsiColor::Green);
/// A REPL command.
pub const COMMAND: Style = fg(AnsiColor::Blue).bold();
/// A determined bit.
pub const BIT: Style = Style::new().bold();
/// An unresolved bit, or a recoverable condition.
pub const UNRESOLVED: Style = fg(AnsiColor::Yellow).bold();
/// The name of a qodec or a layer.
pub const NAME: Style = Style::new().bold().underline();
/// Punctuation and secondary text: `;`, `->`, `=>`, comments.
pub const DIM: Style = Style::new().dimmed();
/// An `error:` prefix.
pub const ERROR: Style = fg(AnsiColor::Red).bold();
/// A token the REPL will reject.
pub const INVALID: Style = fg(AnsiColor::Red).underline();
/// The prompt.
pub const PROMPT: Style = fg(AnsiColor::White).bold();

/// Every style, by name, for tests to make styles readable.
#[cfg(test)]
pub const NAMED: &[(&str, Style)] = &[
    ("mnemonic", MNEMONIC),
    ("block", BLOCK),
    ("param", PARAM),
    ("value", VALUE),
    ("command", COMMAND),
    ("bit", BIT),
    ("unresolved", UNRESOLVED),
    ("name", NAME),
    ("dim", DIM),
    ("error", ERROR),
    ("invalid", INVALID),
    ("prompt", PROMPT),
];

/// `text` in `style`.
pub fn paint(style: Style, text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        format!("{}{text}{}", style.render(), style.render_reset())
    }
}

/// `error:` in its style.
pub fn error_prefix() -> String {
    paint(ERROR, "error:")
}

/// `text` without its styles.
pub fn plain(text: &str) -> String {
    strip_str(text).to_string()
}

/// `text` padded with spaces to `width` visible characters.
pub fn pad(text: &str, width: usize) -> String {
    let visible = plain(text).chars().count();
    format!("{text}{}", " ".repeat(width.saturating_sub(visible)))
}

/// The visible width of `text`.
pub fn width(text: &str) -> usize {
    plain(text).chars().count()
}
