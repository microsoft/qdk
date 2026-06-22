// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Low level printing primitives used by [`crate::display`].

use std::fmt::{self, Display, Write};

/// Takes a unicode buffer or stream and wraps it with
/// `indenter::Indented`. Which applies an indentation of 1
/// each time you insert a new line.
pub(super) fn with_indentation<T>(f: &mut T) -> indenter::Indented<'_, T>
where
    T: fmt::Write,
{
    let indent = indenter::indented(f);
    set_indentation(indent, 1)
}

/// Takes an `indenter::Indented` and changes its indentation level.
///
/// Note: This function is a very low level primitive. It's only
///       public to mantain backwards compatibility with existing code.
///       Prefer using
#[must_use]
pub fn set_indentation<T>(
    indent: indenter::Indented<'_, T>,
    level: usize,
) -> indenter::Indented<'_, T>
where
    T: fmt::Write,
{
    match level {
        0 => indent.with_str(""),
        1 => indent.with_str("    "),
        2 => indent.with_str("        "),
        3 => indent.with_str("            "),
        _ => unimplemented!("indentation level not supported"),
    }
}

/// Writes a list of elements to the given buffer or stream.
pub(super) fn write_list<'write, 'itemref, 'item, T, I>(
    f: &'write mut impl Write,
    vals: I,
) -> fmt::Result
where
    'item: 'itemref,
    T: Display + 'item,
    I: IntoIterator<Item = &'itemref T>,
{
    let mut iter = vals.into_iter().peekable();
    if iter.peek().is_none() {
        write!(f, " <empty>")
    } else {
        for elt in iter {
            write!(f, "\n{elt}")?;
        }
        Ok(())
    }
}
