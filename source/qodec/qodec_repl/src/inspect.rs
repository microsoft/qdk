// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Renders a qodec's gadgets and codes, for `:gadgets`, `:gadget`, `:codes`
//! and `:code`.

use crate::style::{self, BLOCK, DIM, MNEMONIC, NAME, paint};
use qodec::{Code, Encoding, Layer, Qodec};
use std::{collections::BTreeMap, fmt::Write, sync::Arc};

/// Lists the gadgets of layer `which`, or of every layer that has some.
pub fn gadgets(qodec: &Qodec, which: Option<usize>) -> String {
    let mut text = String::new();
    for (k, layer) in selected(qodec, which) {
        if which.is_none() && layer.gadgets.is_empty() {
            continue;
        }
        writeln!(text, "{}", header(qodec, k)).expect("writing to a string succeeds");
        if layer.gadgets.is_empty() {
            writeln!(text, "  {}", paint(DIM, "(no gadgets: nothing lies below)"))
                .expect("writing to a string succeeds");
        }
        let width = layer.gadgets.keys().map(String::len).max().unwrap_or(0);
        for (mnemonic, gadget) in &layer.gadgets {
            let calls = gadget.circuit.calls().map_or_else(
                |_| "unparsed circuit".to_string(),
                |c| count(c.len(), "call"),
            );
            writeln!(
                text,
                "  {}  {} {} {}  {}",
                style::pad(&paint(MNEMONIC, mnemonic), width),
                encodings(&gadget.inputs),
                paint(DIM, "->"),
                encodings(&gadget.outputs),
                paint(
                    DIM,
                    &format!("{calls}, {}", count(gadget.checks.len(), "check"))
                ),
            )
            .expect("writing to a string succeeds");
        }
    }
    if text.is_empty() {
        text = "The qodec has no gadgets.".to_string();
    }
    text
}

/// Shows the gadget for `mnemonic`, in layer `which` or the one layer that has
/// it.
pub fn gadget(qodec: &Qodec, mnemonic: &str, which: Option<usize>) -> Result<String, String> {
    let found: Vec<(usize, &Layer)> = selected(qodec, which)
        .filter(|(_, layer)| layer.gadgets.contains_key(mnemonic))
        .collect();
    let (k, layer) = match found.as_slice() {
        [found] => *found,
        [] => return Err(format!("no gadget `{mnemonic}`; `:gadgets` lists them")),
        _ => return Err(ambiguous("gadget", mnemonic, &found)),
    };
    let gadget = &layer.gadgets[mnemonic];
    Ok(format!(
        "{}: gadget {} {}\n{}",
        header(qodec, k),
        paint(MNEMONIC, mnemonic),
        paint(DIM, &format!("({})", count(gadget.checks.len(), "check"))),
        indent(&gadget.to_string())
    ))
}

/// Lists the codes of layer `which`, or of every layer that has some, by the
/// block type they encode.
pub fn codes(qodec: &Qodec, which: Option<usize>) -> String {
    let mut text = String::new();
    for (k, layer) in selected(qodec, which) {
        let codes = bindings(layer);
        if which.is_none() && codes.is_empty() {
            continue;
        }
        writeln!(text, "{}", header(qodec, k)).expect("writing to a string succeeds");
        if codes.is_empty() {
            writeln!(text, "  {}", paint(DIM, "(no codes)")).expect("writing to a string succeeds");
        }
        let width = codes.keys().map(String::len).max().unwrap_or(0);
        for (block, code) in &codes {
            writeln!(
                text,
                "  {}  {} {}",
                style::pad(&paint(BLOCK, block), width),
                paint(NAME, &code.name),
                paint(DIM, &summary(code))
            )
            .expect("writing to a string succeeds");
        }
    }
    if text.is_empty() {
        text = "The qodec has no codes.".to_string();
    }
    text
}

/// Shows the code for block type or code name `name`, in layer `which` or the
/// one layer that has it.
pub fn code(qodec: &Qodec, name: &str, which: Option<usize>) -> Result<String, String> {
    let mut found: Vec<(usize, &Layer, String, Arc<Code>)> = Vec::new();
    for (k, layer) in selected(qodec, which) {
        for (block, code) in bindings(layer) {
            if block == name || code.name == name {
                found.push((k, layer, block, code));
            }
        }
    }
    let (k, block, code) = match found.as_slice() {
        [(k, _, block, code)] => (*k, block, code),
        [] => return Err(format!("no code `{name}`; `:codes` lists them")),
        _ => {
            let layers: Vec<(usize, &Layer)> = found.iter().map(|f| (f.0, f.1)).collect();
            return Err(ambiguous("code", name, &layers));
        }
    };
    Ok(format!(
        "{}: code {} for {} {}\n{}",
        header(qodec, k),
        paint(NAME, &code.name),
        paint(BLOCK, block),
        paint(DIM, &summary(code)),
        indent(&code.to_string())
    ))
}

/// Layer `which`, or every layer.
fn selected(qodec: &Qodec, which: Option<usize>) -> impl Iterator<Item = (usize, &Layer)> {
    qodec
        .layers()
        .iter()
        .enumerate()
        .filter(move |(k, _)| which.is_none_or(|w| w == *k))
}

/// `layer k: <isa>`, and the instruction set its gadgets lower to.
fn header(qodec: &Qodec, k: usize) -> String {
    let layers = qodec.layers();
    let mut text = format!(
        "layer {k}: {}",
        paint(NAME, &layers[k].instruction_set.name)
    );
    if let Some(below) = layers.get(k + 1) {
        write!(
            text,
            " {} {}",
            paint(DIM, "->"),
            paint(NAME, &below.instruction_set.name)
        )
        .expect("writing to a string succeeds");
    }
    text
}

/// The codes a layer encodes its block types with: its explicit bindings, and
/// those its gadgets' encodings imply.
fn bindings(layer: &Layer) -> BTreeMap<String, Arc<Code>> {
    let mut codes = layer.codes.clone();
    for gadget in layer.gadgets.values() {
        let encodings = gadget.inputs.iter().chain(&gadget.outputs);
        let operands = gadget
            .implements
            .inputs
            .iter()
            .chain(&gadget.implements.outputs);
        for (encoding, operand) in encodings.zip(operands) {
            codes
                .entry(operand.block.clone())
                .or_insert_with(|| Arc::clone(&encoding.code));
        }
    }
    codes
}

/// `(c1, c2)`: the codes of a gadget's inputs or outputs.
fn encodings(encodings: &[Encoding]) -> String {
    let names: Vec<String> = encodings
        .iter()
        .map(|e| paint(NAME, &e.code.name))
        .collect();
    format!("({})", names.join(", "))
}

/// `[[n,k]], s stabilizers`.
fn summary(code: &Code) -> String {
    format!(
        "[[{},{}]], {}",
        code.physical_qubit_count(),
        code.logical_count(),
        count(code.stabilizers.len(), "stabilizer")
    )
}

/// `n what`, in plural unless `n` is 1.
pub fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}

fn ambiguous(what: &str, name: &str, found: &[(usize, &Layer)]) -> String {
    let mut layers: Vec<String> = found.iter().map(|(k, _)| k.to_string()).collect();
    layers.dedup();
    format!(
        "layers {} each have a {what} `{name}`; add the layer, e.g. `:{what} {name} {}`",
        layers.join(", "),
        layers[0]
    )
}

fn indent(yaml: &str) -> String {
    yaml.lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("  {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
