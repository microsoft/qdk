// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub mod core;

use crate::span::Span;
use core::{set_indentation, with_indentation, write_list};
use std::fmt::{self, Display, Formatter, Write};

/// Displays values separated by the provided string.
pub fn join(
    f: &mut Formatter,
    mut vals: impl Iterator<Item = impl Display>,
    sep: &str,
) -> fmt::Result {
    if let Some(v) = vals.next() {
        v.fmt(f)?;
    }
    for v in vals {
        write!(f, "{sep}")?;
        v.fmt(f)?;
    }
    Ok(())
}

/// Writes a list of elements to the given buffer or stream
/// with an additional indentation level.
pub fn write_indented_list<'write, 'itemref, 'item, T, I>(
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
        let mut indent = with_indentation(f);
        for elt in iter {
            write!(indent, "\n{elt}")?;
        }
        Ok(())
    }
}

/// Writes the name and span of a structure to the given buffer or stream.
pub fn write_header(f: &mut impl Write, name: &str, span: Span) -> fmt::Result {
    write!(f, "{name} {span}:")
}

/// Writes the name and span of a structure to the given buffer or stream.
/// Inserts a newline afterwards.
pub fn writeln_header(f: &mut impl Write, name: &str, span: Span) -> fmt::Result {
    writeln!(f, "{name} {span}:")
}

/// Writes a field of a structure to the given buffer
/// or stream with an additional indentation level.
pub fn write_field<T: Display>(f: &mut impl Write, field_name: &str, val: &T) -> fmt::Result {
    let mut indent = with_indentation(f);
    write!(indent, "{field_name}: {val}")
}

/// Writes a field of a structure to the given buffer
/// or stream with an additional indentation level.
/// Inserts a newline afterwards.
pub fn writeln_field<T: Display>(f: &mut impl Write, field_name: &str, val: &T) -> fmt::Result {
    write_field(f, field_name, val)?;
    writeln!(f)
}

/// Writes an optional field of a structure to the given buffer
/// or stream with an additional indentation level.
pub fn write_opt_field<T: Display>(
    f: &mut impl Write,
    field_name: &str,
    opt_val: Option<&T>,
) -> fmt::Result {
    if let Some(val) = opt_val {
        write_field(f, field_name, val)
    } else {
        write_field(f, field_name, &"<none>")
    }
}

/// Writes an optional field of a structure to the given buffer
/// or stream with an additional indentation level.
/// Inserts a newline afterwards.
pub fn writeln_opt_field<T: Display>(
    f: &mut impl Write,
    field_name: &str,
    opt_val: Option<&T>,
) -> fmt::Result {
    write_opt_field(f, field_name, opt_val)?;
    writeln!(f)
}

/// Writes a field of a structure to the given buffer
/// or stream with an additional indentation level.
/// The field must be an iterable.
pub fn write_list_field<'write, 'itemref, 'item, T, I>(
    f: &mut impl Write,
    field_name: &str,
    vals: I,
) -> fmt::Result
where
    'item: 'itemref,
    T: Display + 'item,
    I: IntoIterator<Item = &'itemref T>,
{
    let mut indent = with_indentation(f);
    write!(indent, "{field_name}:")?;
    let mut indent = set_indentation(indent, 2);
    write_list(&mut indent, vals)
}

/// Writes a field of a structure to the given buffer
/// or stream with an additional indentation level.
/// The field must be an iterable.
/// Inserts a newline afterwards.
pub fn writeln_list_field<'write, 'itemref, 'item, T, I>(
    f: &mut impl Write,
    field_name: &str,
    vals: I,
) -> fmt::Result
where
    'item: 'itemref,
    T: Display + 'item,
    I: IntoIterator<Item = &'itemref T>,
{
    write_list_field(f, field_name, vals)?;
    writeln!(f)
}

/// Writes an optional field of a structure to the given buffer
/// or stream with an additional indentation level.
/// The field must be an iterable.
pub fn write_opt_list_field<'write, 'itemref, 'item, T, I>(
    f: &mut impl Write,
    field_name: &str,
    opt_vals: Option<I>,
) -> fmt::Result
where
    'item: 'itemref,
    T: Display + 'item,
    I: IntoIterator<Item = &'itemref T>,
{
    if let Some(vals) = opt_vals {
        write_list_field(f, field_name, vals)
    } else {
        let mut indent = with_indentation(f);
        write!(indent, "{field_name}: <none>")
    }
}

/// Writes an optional field of a structure to the given buffer
/// or stream with an additional indentation level.
/// The field must be an iterable.
/// Inserts a newline afterwards.
pub fn writeln_opt_list_field<'write, 'itemref, 'item, T, I>(
    f: &mut impl Write,
    field_name: &str,
    opt_vals: Option<I>,
) -> fmt::Result
where
    'item: 'itemref,
    T: Display + 'item,
    I: IntoIterator<Item = &'itemref T>,
{
    write_opt_list_field(f, field_name, opt_vals)?;
    writeln!(f)
}
