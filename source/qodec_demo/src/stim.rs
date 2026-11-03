//! A parser of the Stim subset that Qodec gadgets use, registered with the
//! qodec library for gadgets written `format: stim`.
//!
//! `[QODEC]` A gap of the qodec library for Rust hosts: it registers only
//! its inline YAML parser; its Python binding registers Stim's own parser,
//! and a Rust host must register one ([`register`]) before planning a spec
//! whose gadgets are written in Stim, such as repetition3's (the runtime
//! refuses it with "No source parser registered for '.stim'"). Ported from
//! the layering prototype's `src/stim.rs`.
//!
//! `NAME(args) t₀ t₁ …` splits its targets into calls of the instruction's
//! block arity (`CX 0 7 1 8` is two calls), and `args` bind the
//! instruction's parameters in order. `NAME` is matched exactly, else
//! regardless of case, as Stim's names are (`cx` is `CX`). Annotations
//! that do not act on qubits are skipped; what this parser cannot
//! represent (`REPEAT`, measurement-record, sweep-bit and inverted targets)
//! is refused.
//!
//! KNOWN ISSUE: `args` must be written without spaces. Stim's own printer
//! writes `QUBIT_COORDS(0, 0) 0`, which this parser refuses as malformed,
//! as the layering prototype's does: the head is split from the targets at
//! the first space. Repetition3's gadgets have no such line.
//!
//! KNOWN ISSUE: the port keeps the layering prototype's leniencies, which
//! no gadget reaches; where the parser lives settles them:
//! - empty argument slots are dropped, so `rotate_z(,0.7) 0` and
//!   `rotate_z(0.7,) 0` bind `theta` to 0.7;
//! - `CX 0 0`, which Stim refuses, gives a CX of qubit 0 on itself;
//! - targets `+0` and `00`, which Stim refuses, are qubit 0;
//! - non-finite arguments are accepted: `rotate_z(NaN)`, `(inf)`,
//!   `(1e400)`;
//! - Stim's aliases, such as `CNOT` and `ZCX` for `CX`, are refused as not
//!   in the set;
//! - an annotation's targets are not checked: `TICK 0 1`, which Stim
//!   refuses, is skipped like `TICK`;
//! - a name that matches no instruction exactly matches one regardless of
//!   case, which Stim does too; the first such instruction is taken, so a
//!   set whose names differ only by case is matched by declaration order.

use std::collections::BTreeMap;

use qodec::{Argument, InstructionCall, InstructionSet, Operand, ParameterKind};

const SKIPPED: [&str; 5] = [
    "TICK",
    "DETECTOR",
    "OBSERVABLE_INCLUDE",
    "QUBIT_COORDS",
    "SHIFT_COORDS",
];

/// Registers [`parse`] for the format `stim`, in this process, replacing any
/// parser registered for it before.
///
/// # Errors
///
/// When the qodec library's registry is unavailable.
pub fn register() -> Result<(), String> {
    qodec::register(parse, "stim")
}

/// `source`'s calls of `set`'s instructions, in order.
///
/// # Errors
///
/// The first line that is not a call of `set`, with its line number: an
/// unknown name, arguments that do not match the instruction's parameters,
/// targets that are not qubits, not in groups of its arity, or given to an
/// instruction on no qubits, or a construct this parser does not support.
pub fn parse(source: &str, set: &InstructionSet) -> Result<Vec<InstructionCall>, String> {
    let mut calls = Vec::new();
    for (n, line) in source.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = |e: String| format!("stim line {}: {e}", n + 1);
        let (head, targets) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let (name, args) = match head.split_once('(') {
            Some((name, rest)) => (
                name,
                rest.strip_suffix(')')
                    .ok_or_else(|| at(format!("malformed `{head}`")))?,
            ),
            None => (head, ""),
        };
        if SKIPPED.contains(&name.to_ascii_uppercase().as_str()) {
            continue;
        }
        if name.eq_ignore_ascii_case("REPEAT") || name == "}" {
            return Err(at("REPEAT blocks are not supported".into()));
        }
        let instruction = set
            .instructions
            .iter()
            .find(|i| i.mnemonic == name)
            .or_else(|| {
                set.instructions
                    .iter()
                    .find(|i| i.mnemonic.eq_ignore_ascii_case(name))
            })
            .ok_or_else(|| at(format!("`{name}` is not in {}", set.name)))?;
        let mut arguments = BTreeMap::new();
        let values: Vec<&str> = args
            .split(',')
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .collect();
        if values.len() != instruction.parameters.len() {
            return Err(at(format!(
                "`{name}` takes {} arguments",
                instruction.parameters.len()
            )));
        }
        for (p, v) in instruction.parameters.iter().zip(values) {
            let value = match p.kind {
                ParameterKind::Number => Argument::Number(
                    v.parse()
                        .map_err(|_| at(format!("`{v}` is not a number")))?,
                ),
                ParameterKind::Integer => Argument::Integer(
                    v.parse()
                        .map_err(|_| at(format!("`{v}` is not an integer")))?,
                ),
                other => return Err(at(format!("`{name}` takes a {other:?} parameter"))),
            };
            arguments.insert(p.name.clone(), value);
        }
        let mut qubits = Vec::new();
        for t in targets.split_whitespace() {
            if t.starts_with("rec[") || t.starts_with('!') || t.starts_with("sweep[") {
                return Err(at(format!("target `{t}` is not supported")));
            }
            qubits.push(
                t.parse::<usize>()
                    .map_err(|_| at(format!("target `{t}` is not a qubit")))?,
            );
        }
        let arity = instruction.inputs.len().max(instruction.outputs.len());
        if arity == 0 {
            if !qubits.is_empty() {
                return Err(at(format!("`{name}` takes no targets")));
            }
            calls.push(InstructionCall {
                mnemonic: instruction.mnemonic.clone(),
                operands: vec![],
                arguments,
                select: vec![],
            });
            continue;
        }
        if qubits.is_empty() || qubits.len() % arity != 0 {
            return Err(at(format!("`{name}` takes targets in groups of {arity}")));
        }
        for group in qubits.chunks(arity) {
            calls.push(InstructionCall {
                mnemonic: instruction.mnemonic.clone(),
                operands: group.iter().copied().map(Operand::Index).collect(),
                arguments: arguments.clone(),
                select: vec![],
            });
        }
    }
    Ok(calls)
}
