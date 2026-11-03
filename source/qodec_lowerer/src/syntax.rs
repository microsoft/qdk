// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The REPL's instruction syntax, used both to parse what the user types and to
//! print lowered instructions, so a printed instruction can be typed back in:
//!
//! ```text
//! <mnemonic> <block>... [<parameter>=<value>]...
//! ```
//!
//! Blocks are ids at the boundary, one per operand: operand `j` is `in[j]`
//! and/or `out[j]`.

use crate::style::{BLOCK, DIM, INVALID, MNEMONIC, PARAM, VALUE, paint};
use qodec::{Instruction, InstructionSet, ParameterKind};
use qodec_runtime::{BackendInstruction, OwnedParam};
use smallvec::SmallVec;
use std::fmt::Write;

/// An instruction at some boundary of the qodec.
pub type Ins = BackendInstruction<usize, OwnedParam>;

/// Splits `line` into `;`-separated statements.
pub fn statements(line: &str) -> Result<Vec<&str>, String> {
    scan(line, |c| c == ';')
}

/// Splits `text` at every `sep` outside double quotes and brackets, and drops
/// empty parts.
fn scan(text: &str, is_sep: impl Fn(char) -> bool) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let (mut start, mut quoted, mut depth) = (0, false, 0usize);
    for (i, c) in text.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => depth += 1,
            ']' if !quoted => depth = depth.checked_sub(1).ok_or("unbalanced `]`")?,
            c if !quoted && depth == 0 && is_sep(c) => {
                parts.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    if quoted {
        return Err("unterminated `\"`".to_string());
    }
    if depth > 0 {
        return Err("unbalanced `[`".to_string());
    }
    parts.push(&text[start..]);
    Ok(parts
        .into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect())
}

/// Parses one statement into an instruction of `isa`.
pub fn parse_instruction(isa: &InstructionSet, statement: &str) -> Result<Ins, String> {
    let tokens = scan(statement, char::is_whitespace)?;
    let (mnemonic, rest) = tokens.split_first().ok_or("empty instruction")?;
    let (instruction_id, decl) = isa
        .instructions
        .iter()
        .enumerate()
        .find(|(_, i)| i.mnemonic == *mnemonic)
        .ok_or_else(|| {
            format!(
                "`{mnemonic}` is not an instruction of `{}`; see `:isa`",
                isa.name
            )
        })?;
    let usage = || format!("usage: {}", signature(decl));

    let mut parameters = SmallVec::new();
    let mut args: Vec<(&str, &str)> = Vec::new();
    for token in rest {
        if let Some((name, value)) = token.split_once('=') {
            if !decl.parameters.iter().any(|p| p.name == name) {
                return Err(format!(
                    "`{mnemonic}` has no parameter `{name}`; {}",
                    usage()
                ));
            }
            if args.iter().any(|(n, _)| *n == name) {
                return Err(format!("argument `{name}` is given twice"));
            }
            args.push((name, value));
        } else if !args.is_empty() {
            return Err(format!(
                "block `{token}` must come before the arguments; {}",
                usage()
            ));
        } else {
            let block = token.parse::<u64>().map_err(|_| {
                format!(
                    "`{token}` is not a block id, and arguments are written `name=value`; {}",
                    usage()
                )
            })?;
            parameters.push(OwnedParam::Block(block));
        }
    }
    // Arguments follow the blocks, in declaration order.
    for parameter in &decl.parameters {
        let (_, value) = args
            .iter()
            .find(|(name, _)| *name == parameter.name)
            .ok_or_else(|| format!("missing argument `{}`; {}", parameter.name, usage()))?;
        let value = parse_value(parameter.kind, value)
            .map_err(|e| format!("argument `{}`: {e}", parameter.name))?;
        parameters.push(value);
    }
    Ok(Ins {
        instruction_id,
        parameters,
    })
}

fn parse_value(kind: ParameterKind, text: &str) -> Result<OwnedParam, String> {
    if let Some(inner) = text.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        let items = scan(inner, |c| c == ',')?;
        return Ok(
            match items.iter().map(|i| i.parse()).collect::<Result<_, _>>() {
                Ok(ints) => OwnedParam::IntList(ints),
                Err(_) => OwnedParam::TextList(items.iter().map(|i| unquote(i).into()).collect()),
            },
        );
    }
    match kind {
        ParameterKind::Bit | ParameterKind::Boolean => match text {
            "0" | "false" => Ok(OwnedParam::Bool(false)),
            "1" | "true" => Ok(OwnedParam::Bool(true)),
            _ => Err(format!("expected 0, 1, false or true, got `{text}`")),
        },
        ParameterKind::Integer => text
            .parse()
            .map(OwnedParam::Int)
            .map_err(|_| format!("expected an integer, got `{text}`")),
        ParameterKind::Number => text.parse().map(OwnedParam::Int).or_else(|_| {
            text.parse()
                .map(OwnedParam::Float)
                .map_err(|_| format!("expected a number, got `{text}`"))
        }),
        ParameterKind::String | ParameterKind::Pauli => Ok(OwnedParam::Text(unquote(text).into())),
    }
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
}

/// Renders `ins`, an instruction of `isa`, in the input syntax.
pub fn render_instruction(isa: &InstructionSet, ins: &Ins) -> String {
    let Some(decl) = isa.instructions.get(ins.instruction_id) else {
        return paint(
            INVALID,
            &format!("<unknown instruction {}>", ins.instruction_id),
        );
    };
    let mut text = paint(MNEMONIC, &decl.mnemonic);
    let blocks = ins
        .parameters
        .iter()
        .take_while(|p| matches!(p, OwnedParam::Block(_)))
        .count();
    let (blocks, args) = ins.parameters.split_at(blocks);
    for block in blocks {
        write!(text, " {}", paint(BLOCK, &render_value(block, None)))
            .expect("writing to a string succeeds");
    }
    for (i, arg) in args.iter().enumerate() {
        let parameter = decl.parameters.get(i);
        let name = parameter.map_or("?", |p| p.name.as_str());
        let value = render_value(arg, parameter.map(|p| p.kind));
        write!(text, " {}={}", paint(PARAM, name), paint(VALUE, &value))
            .expect("writing to a string succeeds");
    }
    text
}

fn render_value(value: &OwnedParam, kind: Option<ParameterKind>) -> String {
    match value {
        OwnedParam::Block(b) => b.to_string(),
        OwnedParam::Bool(b) if kind == Some(ParameterKind::Bit) => u8::from(*b).to_string(),
        OwnedParam::Bool(b) => b.to_string(),
        OwnedParam::Int(i) => i.to_string(),
        OwnedParam::Float(f) => format!("{f:?}"),
        OwnedParam::Text(s) => quote(s),
        OwnedParam::IntList(items) => list(items.iter().map(ToString::to_string)),
        OwnedParam::TextList(items) => list(items.iter().map(|s| quote(s))),
    }
}

fn quote(text: &str) -> String {
    if text.is_empty() || text.contains(|c: char| c.is_whitespace() || ";,[]\"".contains(c)) {
        format!("\"{text}\"")
    } else {
        text.to_string()
    }
}

fn list(items: impl Iterator<Item = String>) -> String {
    format!("[{}]", items.collect::<Vec<_>>().join(", "))
}

/// The instruction's usage, e.g. `XIF <qubit> c=<bit>`, then what it reports.
pub fn signature(decl: &Instruction) -> String {
    let mut text = paint(MNEMONIC, &decl.mnemonic);
    for j in 0..decl.inputs.len().max(decl.outputs.len()) {
        let operand = match (decl.inputs.get(j), decl.outputs.get(j)) {
            (Some(i), Some(o)) if i.block == o.block => i.block.clone(),
            (Some(i), Some(o)) => format!("{}->{}", i.block, o.block),
            (Some(i), None) => format!("in:{}", i.block),
            (None, Some(o)) => format!("out:{}", o.block),
            (None, None) => unreachable!("j is below the longer side's length"),
        };
        let variadic = [decl.inputs.get(j), decl.outputs.get(j)]
            .into_iter()
            .flatten()
            .any(|o| o.is_variadic);
        let dots = if variadic { "..." } else { "" };
        write!(text, " {}", paint(BLOCK, &format!("<{operand}>{dots}")))
            .expect("writing to a string succeeds");
    }
    for parameter in &decl.parameters {
        let kind = format!("{:?}", parameter.kind).to_lowercase();
        write!(
            text,
            " {}={}",
            paint(PARAM, &parameter.name),
            paint(VALUE, &format!("<{kind}>"))
        )
        .expect("writing to a string succeeds");
    }
    let observes = decl.observe_count();
    let mut reports = Vec::new();
    if observes > 0 {
        let s = if observes == 1 { "" } else { "s" };
        reports.push(format!("{observes} outcome{s}"));
    }
    if !decl.flags.is_empty() {
        reports.push(format!("flags {}", decl.flags.join(" ")));
    }
    if !reports.is_empty() {
        write!(text, "  {} {}", paint(DIM, "=>"), reports.join(", "))
            .expect("writing to a string succeeds");
    }
    text
}
