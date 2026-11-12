// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The instruction syntax, the same as `qodec-repl`'s, so a line can move
//! between the two tools:
//!
//! ```text
//! <mnemonic> <block>... [<parameter>=<value>]...
//! ```
//!
//! Statements on one line are separated by `;` and run as one batch. Blocks
//! are ids at the boundary, one per operand: operand `j` is `in[j]` and/or
//! `out[j]`. Rendering produces [`Token`]s tagged with their [`Role`], so the
//! interface can color them without parsing them again.

use qodec::{Instruction, InstructionSet, ParameterKind};
use qodec_runtime_recipes::{Call, OwnedParam};
use smallvec::SmallVec;

/// What a piece of rendered text is, for coloring it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// An instruction's mnemonic.
    Mnemonic,
    /// A block id, or a block operand in a signature.
    Block,
    /// A parameter name.
    Param,
    /// An argument value, or a parameter kind in a signature.
    Value,
    /// Spaces, `=`, `;` and other glue.
    Plain,
    /// Secondary text, such as what a signature reports.
    Dim,
    /// A `#` comment.
    Comment,
    /// Text that does not parse.
    Invalid,
    /// A bit the bottom backend made up.
    Bit,
    /// A bit set by hand.
    Pinned,
    /// A bit that could not be determined, or a recoverable condition.
    Unresolved,
    /// The name of a qodec, layer, code or readout.
    Name,
    /// An error.
    Error,
}

/// A piece of rendered text and its role.
pub type Token = (Role, String);

/// The text of `tokens`, without their roles.
pub fn plain(tokens: &[Token]) -> String {
    tokens.iter().map(|(_, t)| t.as_str()).collect()
}

/// Splits `line` into `;`-separated statements, without a trailing comment.
pub fn statements(line: &str) -> Result<Vec<&str>, String> {
    scan(code(line), |c| c == ';')
}

/// `line` without its `#` comment, if any.
fn code(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
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

/// Parses `line` into the batch of calls of `isa` it stands for. A blank line
/// or a comment is an empty batch.
pub fn parse_line(isa: &InstructionSet, line: &str) -> Result<Vec<Call>, String> {
    statements(line)?
        .into_iter()
        .map(|s| parse_instruction(isa, s))
        .collect()
}

/// Parses one statement into a call of `isa`.
pub fn parse_instruction(isa: &InstructionSet, statement: &str) -> Result<Call, String> {
    let tokens = scan(statement, char::is_whitespace)?;
    let (mnemonic, rest) = tokens.split_first().ok_or("empty instruction")?;
    let (instruction_id, decl) = isa
        .instructions
        .iter()
        .enumerate()
        .find(|(_, i)| i.mnemonic == *mnemonic)
        .ok_or_else(|| format!("`{mnemonic}` is not an instruction of `{}`", isa.name))?;
    let usage = || format!("usage: {}", plain(&signature(decl)));

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
    Ok(Call {
        instruction_id,
        parameters,
    })
}

fn parse_value(kind: ParameterKind, text: &str) -> Result<OwnedParam, String> {
    if text.starts_with('[') {
        return Err("parameters take a single value, not a list".to_string());
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

/// The block operands of `call`.
pub fn blocks(call: &Call) -> impl Iterator<Item = u64> + '_ {
    call.parameters.iter().map_while(|p| match p {
        OwnedParam::Block(b) => Some(*b),
        _ => None,
    })
}

/// Renders `call`, a call of `isa`, in the input syntax.
pub fn render(isa: &InstructionSet, call: &Call) -> Vec<Token> {
    let Some(decl) = isa.instructions.get(call.instruction_id) else {
        return vec![(
            Role::Invalid,
            format!("<unknown instruction {}>", call.instruction_id),
        )];
    };
    let mut tokens = vec![(Role::Mnemonic, decl.mnemonic.clone())];
    let count = blocks(call).count();
    let (operands, args) = call.parameters.split_at(count);
    for operand in operands {
        tokens.push((Role::Plain, " ".to_string()));
        tokens.push((Role::Block, render_value(operand, None)));
    }
    for (i, arg) in args.iter().enumerate() {
        let parameter = decl.parameters.get(i);
        let name = parameter.map_or("?", |p| p.name.as_str());
        tokens.push((Role::Plain, " ".to_string()));
        tokens.push((Role::Param, name.to_string()));
        tokens.push((Role::Plain, "=".to_string()));
        tokens.push((Role::Value, render_value(arg, parameter.map(|p| p.kind))));
    }
    tokens
}

fn render_value(value: &OwnedParam, kind: Option<ParameterKind>) -> String {
    match value {
        OwnedParam::Block(b) => b.to_string(),
        OwnedParam::Bool(b) if kind == Some(ParameterKind::Bit) => u8::from(*b).to_string(),
        OwnedParam::Bool(b) => b.to_string(),
        OwnedParam::Int(i) => i.to_string(),
        OwnedParam::Float(f) => format!("{f:?}"),
        OwnedParam::Text(s) => quote(s),
    }
}

fn quote(text: &str) -> String {
    if text.is_empty() || text.contains(|c: char| c.is_whitespace() || ";,[]\"#".contains(c)) {
        format!("\"{text}\"")
    } else {
        text.to_string()
    }
}

/// The instruction's usage, e.g. `XIF <qubit> c=<bit>`, then what it reports.
pub fn signature(decl: &Instruction) -> Vec<Token> {
    let mut tokens = vec![(Role::Mnemonic, decl.mnemonic.clone())];
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
        tokens.push((Role::Plain, " ".to_string()));
        tokens.push((Role::Block, format!("<{operand}>{dots}")));
    }
    for parameter in &decl.parameters {
        let kind = format!("{:?}", parameter.kind).to_lowercase();
        tokens.push((Role::Plain, " ".to_string()));
        tokens.push((Role::Param, parameter.name.clone()));
        tokens.push((Role::Plain, "=".to_string()));
        tokens.push((Role::Value, format!("<{kind}>")));
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
        tokens.push((Role::Dim, format!("  => {}", reports.join(", "))));
    }
    tokens
}

/// Colors `line`, as typed, a statement at a time: what parses keeps its
/// roles, and a statement that does not parse is marked invalid.
pub fn highlight(isa: &InstructionSet, line: &str) -> Vec<Token> {
    let code_len = code(line).len();
    let (body, comment) = line.split_at(code_len);
    let mut tokens = Vec::new();
    let mut rest = body;
    while !rest.is_empty() {
        let end = rest.find(';').unwrap_or(rest.len());
        let (statement, after) = rest.split_at(end);
        tokens.extend(highlight_statement(isa, statement));
        if let Some(after) = after.strip_prefix(';') {
            tokens.push((Role::Plain, ";".to_string()));
            rest = after;
        } else {
            rest = "";
        }
    }
    if !comment.is_empty() {
        tokens.push((Role::Comment, comment.to_string()));
    }
    tokens
}

fn highlight_statement(isa: &InstructionSet, statement: &str) -> Vec<Token> {
    let valid = parse_instruction(isa, statement).is_ok() || statement.trim().is_empty();
    let mut tokens = Vec::new();
    let mut first = true;
    let mut word = String::new();
    let flush = |word: &mut String, first: &mut bool, tokens: &mut Vec<Token>| {
        if word.is_empty() {
            return;
        }
        let role = if !valid && *first {
            if isa.instructions.iter().any(|i| i.mnemonic == *word) {
                Role::Mnemonic
            } else {
                Role::Invalid
            }
        } else if *first {
            Role::Mnemonic
        } else if word.contains('=') {
            Role::Value
        } else if word.parse::<u64>().is_ok() {
            Role::Block
        } else {
            Role::Invalid
        };
        if role == Role::Value {
            let (name, value) = word.split_once('=').expect("the word has a `=`");
            tokens.push((Role::Param, name.to_string()));
            tokens.push((Role::Plain, "=".to_string()));
            tokens.push((Role::Value, value.to_string()));
        } else {
            tokens.push((role, std::mem::take(word)));
        }
        word.clear();
        *first = false;
    };
    let mut quoted = false;
    for c in statement.chars() {
        if c == '"' {
            quoted = !quoted;
        }
        if c.is_whitespace() && !quoted {
            flush(&mut word, &mut first, &mut tokens);
            tokens.push((Role::Plain, c.to_string()));
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut first, &mut tokens);
    if !valid
        && !tokens.iter().any(|(r, _)| *r == Role::Invalid)
        && let Some(last) = tokens.iter_mut().rev().find(|(r, _)| *r != Role::Plain)
    {
        last.0 = Role::Invalid;
    }
    tokens
}
