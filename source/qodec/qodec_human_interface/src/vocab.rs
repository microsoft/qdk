// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The vocabulary: what each thing on screen means, in words, next to it, so
//! nobody has to look it up or work it out in their head.
//!
//! Bits are labelled with what they are (an outcome by the observable it
//! measures, a flag by its name), conditions with what they mean, and an
//! instruction with what it does and how the layer below implements it.

use crate::{
    syntax::{Role, Token},
    trace::BitId,
};
use qodec::{Action, Instruction, InstructionSet, ParameterKind, Qodec};
use qodec_runtime_recipes::{BasicReadout, Bit, Status};
use std::fmt::Write;

/// The names of an instruction's result bits: each outcome by the observable
/// it measures, then each flag by its name.
#[must_use]
pub fn readout_names(decl: &Instruction) -> Vec<String> {
    let mut names: Vec<String> = decl
        .action
        .iter()
        .flat_map(|step| match &step.action {
            Action::Observe(observables) => observables.iter().map(|o| o.pauli.0.clone()).collect(),
            _ => Vec::new(),
        })
        .collect();
    names.extend(decl.flags.iter().cloned());
    names
}

/// The tokens of `bit`, pinned or not.
#[must_use]
pub fn bit(bit: Bit, pinned: bool) -> Token {
    match bit {
        Some(b) if pinned => (Role::Pinned, u8::from(b).to_string()),
        Some(b) => (Role::Bit, u8::from(b).to_string()),
        None => (Role::Unresolved, "?".to_string()),
    }
}

/// `readout`, the result of a call of `decl`, as named bits: `Z_0=0
/// reject=1`, then its conditions, if any. `pins` says, per bit, whether it
/// is pinned; it is empty above the bottom.
#[must_use]
pub fn readout(decl: &Instruction, readout: &BasicReadout, pins: &[(BitId, bool)]) -> Vec<Token> {
    let names = readout_names(decl);
    let mut tokens = Vec::new();
    for (i, &b) in readout.outcomes.iter().chain(&readout.flags).enumerate() {
        if i > 0 {
            tokens.push((Role::Plain, " ".to_string()));
        }
        let name = names.get(i).map_or("?", String::as_str);
        tokens.push((Role::Name, name.to_string()));
        tokens.push((Role::Plain, "=".to_string()));
        tokens.push(bit(b, pins.get(i).is_some_and(|&(_, p)| p)));
    }
    if let Some(conditions) = conditions(readout.status) {
        tokens.push((Role::Unresolved, format!(" ({conditions})")));
    }
    tokens
}

/// The bits of `readout` alone, `0`, `01` or `?`, for tight spaces.
#[must_use]
pub fn compact(readout: &BasicReadout, pins: &[(BitId, bool)]) -> Vec<Token> {
    readout
        .outcomes
        .iter()
        .chain(&readout.flags)
        .enumerate()
        .map(|(i, &b)| bit(b, pins.get(i).is_some_and(|&(_, p)| p)))
        .collect()
}

/// The conditions `status` holds, `rejected, unresolved`, if any.
#[must_use]
pub fn conditions(status: Status) -> Option<String> {
    let names: Vec<&str> = meanings(status).iter().map(|(name, _)| *name).collect();
    (!names.is_empty()).then(|| names.join(", "))
}

/// Each condition `status` holds, with what it means.
#[must_use]
pub fn meanings(status: Status) -> Vec<(&'static str, &'static str)> {
    [
        (
            status.rejected,
            "rejected",
            "no `select` pattern of the gadget matched its flags",
        ),
        (
            status.inconsistent,
            "inconsistent",
            "the parity evidence contradicts itself",
        ),
        (
            status.unresolved,
            "unresolved",
            "a value the decoder needed could not be determined",
        ),
    ]
    .into_iter()
    .filter_map(|(set, name, meaning)| set.then_some((name, meaning)))
    .collect()
}

/// How the layer below boundary `b` implements `mnemonic`, in one line.
#[must_use]
pub fn implementation(qodec: &Qodec, b: usize, mnemonic: &str) -> String {
    let layers = qodec.layers();
    let Some(below) = layers.get(b + 1) else {
        return "A bottom instruction: the backend runs it.".to_string();
    };
    let below = &below.instruction_set.name;
    match layers[b].gadgets.get(mnemonic) {
        Some(gadget) => {
            let codes = |encodings: &[qodec::Encoding]| -> String {
                let names: Vec<&str> = encodings.iter().map(|e| e.code.name.as_str()).collect();
                format!("({})", names.join(", "))
            };
            let calls = gadget.circuit.calls().map_or_else(
                |_| "an unparsed circuit".to_string(),
                |c| count(c.len(), &format!("{below} call")),
            );
            format!(
                "Gadget {} -> {}: {calls}, {}.",
                codes(&gadget.inputs),
                codes(&gadget.outputs),
                count(gadget.checks.len(), "check"),
            )
        }
        None => format!("No gadget: applied with {below} corrections, or as a release."),
    }
}

/// The instructions of `isa` that give a new block of type `block`.
#[must_use]
pub fn producers(isa: &InstructionSet, block: &str) -> Vec<String> {
    isa.instructions
        .iter()
        .filter(|i| {
            i.outputs
                .iter()
                .enumerate()
                .any(|(j, o)| o.block == block && i.inputs.get(j).is_none_or(|x| x.block != block))
        })
        .map(|i| i.mnemonic.clone())
        .collect()
}

/// A complete line calling instruction `index` of `isa`, ready to run: each
/// input on the most recently given live block of its type, each output-only
/// operand on the lowest free id, and each argument a plain default. `live`
/// is the live blocks, by id, with their types, most recently given last.
#[must_use]
pub fn default_line(isa: &InstructionSet, index: usize, live: &[(u64, String)]) -> String {
    let decl = &isa.instructions[index];
    let mut chosen: Vec<u64> = Vec::new();
    // Of any `n + 1` ids, one is free when `n` are taken.
    let fresh = |chosen: &[u64]| {
        let taken = (chosen.len() + live.len()) as u64;
        (0..=taken)
            .find(|id| !chosen.contains(id) && !live.iter().any(|(l, _)| l == id))
            .expect("some id is free")
    };
    for j in 0..decl.inputs.len().max(decl.outputs.len()) {
        let block = match decl.inputs.get(j) {
            Some(input) => live
                .iter()
                .rev()
                .find(|(l, t)| *t == input.block && !chosen.contains(l))
                .map_or_else(|| fresh(&chosen), |(l, _)| *l),
            None => fresh(&chosen),
        };
        chosen.push(block);
    }
    let mut text = decl.mnemonic.clone();
    for block in chosen {
        write!(text, " {block}").expect("writing to a string succeeds");
    }
    for parameter in &decl.parameters {
        let value = match parameter.kind {
            ParameterKind::Bit | ParameterKind::Integer | ParameterKind::Number => "0",
            ParameterKind::Boolean => "false",
            ParameterKind::String => "\"\"",
            ParameterKind::Pauli => "Z_0",
        };
        write!(text, " {}={value}", parameter.name).expect("writing to a string succeeds");
    }
    text
}

/// `n what`, in plural unless `n` is 1.
#[must_use]
pub fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}
