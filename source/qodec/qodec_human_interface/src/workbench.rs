// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The qodec as things to work on: the items of the map, the document each
//! one lives in, what the tool found about each, and the edits that adopt what
//! it derived.

use crate::{
    analysis::{self, Analysis, Basis, Term, Verdict},
    bundle::{self, Doc, LayerFiles},
    codes,
    syntax::{Role, Token},
};
use qodec::{Gadget, InstructionSet, ParameterKind, Qodec};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

/// Something on the map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Item {
    /// The manifest.
    Qodec,
    /// Layer `k`'s instruction set.
    Layer(usize),
    /// Instruction `i` of layer `k`, and its gadget if it has one.
    Instruction(usize, usize),
    /// The code layer `k` binds to a block type.
    Code(usize, String),
    /// The program, run through the whole stack.
    Program,
}

/// What the tool thinks of an item, at a glance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    /// Nothing to say.
    None,
    /// Everything the tool can check holds.
    Ok,
    /// Something to look at, such as an output sign no check determines.
    Warn,
    /// Something is wrong.
    Error,
    /// The tool cannot tell, past a step it cannot follow.
    Unknown,
    /// An instruction with no gadget yet.
    Missing,
}

impl Mark {
    /// The mark's symbol.
    #[must_use]
    pub fn symbol(self) -> &'static str {
        match self {
            Mark::None => " ",
            Mark::Ok => "✓",
            Mark::Warn => "!",
            Mark::Error => "✗",
            Mark::Unknown => "~",
            Mark::Missing => "·",
        }
    }
}

/// One row of the map.
#[derive(Clone, Debug)]
pub struct Row {
    /// What it shows.
    pub item: Item,
    /// How deep it is nested.
    pub depth: usize,
    /// Its text.
    pub label: String,
    /// What the tool thinks of it.
    pub mark: Mark,
}

/// The manifest's layers.
#[must_use]
pub fn layer_files(docs: &[Doc]) -> Vec<LayerFiles> {
    docs.first()
        .map(|d| bundle::manifest_layers(&d.body))
        .unwrap_or_default()
}

/// The document named `file`, as the manifest names it.
#[must_use]
pub fn find_doc(docs: &[Doc], file: &str) -> Option<usize> {
    let wanted = bundle::resolve("qodec.yaml", file);
    docs.iter()
        .position(|d| bundle::resolve("qodec.yaml", &d.name) == wanted)
        .or_else(|| {
            docs.iter()
                .position(|d| d.name.ends_with(file.trim_start_matches("./")))
        })
}

/// The document `item` lives in, and the line to start at.
#[must_use]
pub fn doc_of(docs: &[Doc], qodec: Option<&Qodec>, item: &Item) -> Option<(usize, usize)> {
    let layers = layer_files(docs);
    match item {
        Item::Qodec => (!docs.is_empty()).then_some((0, 0)),
        Item::Program => None,
        Item::Layer(k) => layers
            .get(*k)?
            .isa
            .as_deref()
            .and_then(|f| find_doc(docs, f))
            .map(|d| (d, 0)),
        Item::Code(k, block) => {
            let file = layers
                .get(*k)?
                .codes
                .iter()
                .find(|(b, _)| b == block)?
                .1
                .clone();
            find_doc(docs, &file).map(|d| (d, 0))
        }
        Item::Instruction(k, i) => {
            let mnemonic = &qodec?
                .layers()
                .get(*k)?
                .instruction_set
                .instructions
                .get(*i)?
                .mnemonic;
            if let Some((_, file)) = layers.get(*k)?.gadgets.iter().find(|(m, _)| m == mnemonic) {
                return find_doc(docs, file).map(|d| (d, 0));
            }
            let d = layers
                .get(*k)?
                .isa
                .as_deref()
                .and_then(|f| find_doc(docs, f))?;
            let line = docs[d]
                .body
                .lines()
                .position(|l| {
                    l.trim_start().trim_start_matches("- ").trim()
                        == format!("mnemonic: {mnemonic}")
                })
                .unwrap_or(0);
            Some((d, line))
        }
    }
}

/// The gadget of instruction `i` of layer `k`, with the instruction set its
/// circuit calls.
#[must_use]
pub fn gadget_of(qodec: &Qodec, k: usize, i: usize) -> Option<(&Gadget, &InstructionSet)> {
    let layers = qodec.layers();
    let mnemonic = &layers.get(k)?.instruction_set.instructions.get(i)?.mnemonic;
    let gadget = layers[k].gadgets.get(mnemonic)?;
    Some((gadget, &layers.get(k + 1)?.instruction_set))
}

/// A fingerprint of what a gadget's analysis depends on.
#[must_use]
pub fn fingerprint(gadget: &Gadget, lower: &InstructionSet) -> u64 {
    let mut h = DefaultHasher::new();
    format!("{gadget:?}{lower:?}").hash(&mut h);
    h.finish()
}

/// The mark of an analysis.
#[must_use]
pub fn mark_of(a: &Result<Analysis, String>) -> Mark {
    match a {
        Err(_) => Mark::Error,
        Ok(a) => {
            let errors = a
                .checks
                .iter()
                .chain(&a.readouts)
                .chain(&a.frames)
                .any(Verdict::fails)
                || !a.action.mismatches.is_empty()
                || (a.followed && a.out_stabilizers.iter().any(|s| s.sign.is_none()));
            let warnings = !a.warnings.is_empty()
                || (a.followed && a.out_stabilizers.iter().any(|s| !s.determined));
            if errors {
                Mark::Error
            } else if warnings {
                Mark::Warn
            } else if !a.followed {
                Mark::Unknown
            } else {
                Mark::Ok
            }
        }
    }
}

/// The map's rows, with `analyze` giving the analysis of the gadget of
/// instruction `i` of layer `k`.
pub fn rows(
    qodec: &Qodec,
    load_error: bool,
    plan_error: bool,
    program_mark: Mark,
    mut analyze: impl FnMut(usize, usize) -> Option<Mark>,
) -> Vec<Row> {
    let mut rows = vec![Row {
        item: Item::Qodec,
        depth: 0,
        label: qodec.name().unwrap_or("unnamed").to_string(),
        mark: if load_error || plan_error {
            Mark::Error
        } else {
            Mark::None
        },
    }];
    let layers = qodec.layers();
    let last = layers.len().saturating_sub(1);
    for (k, layer) in layers.iter().enumerate() {
        let role = match (k, last) {
            (0, 0) => " (top, bottom)",
            (0, _) => " (top)",
            (k, l) if k == l => " (bottom)",
            _ => "",
        };
        rows.push(Row {
            item: Item::Layer(k),
            depth: 0,
            label: format!("{k} {}{role}", layer.instruction_set.name),
            mark: Mark::None,
        });
        for (i, decl) in layer.instruction_set.instructions.iter().enumerate() {
            let mark = if k == last {
                Mark::None
            } else if layer.gadgets.contains_key(&decl.mnemonic) {
                analyze(k, i).unwrap_or(Mark::None)
            } else {
                Mark::Missing
            };
            rows.push(Row {
                item: Item::Instruction(k, i),
                depth: 1,
                label: decl.mnemonic.clone(),
                mark,
            });
        }
        let mut bound: Vec<(String, std::sync::Arc<qodec::Code>)> = layer
            .codes
            .iter()
            .map(|(b, c)| (b.clone(), c.clone()))
            .collect();
        for gadget in layer.gadgets.values() {
            for (enc, op) in gadget.inputs.iter().chain(&gadget.outputs).zip(
                gadget
                    .implements
                    .inputs
                    .iter()
                    .chain(&gadget.implements.outputs),
            ) {
                if !bound.iter().any(|(b, _)| *b == op.block) {
                    bound.push((op.block.clone(), enc.code.clone()));
                }
            }
        }
        for (block, code) in bound {
            let facts = codes::facts(&code);
            rows.push(Row {
                item: Item::Code(k, block.clone()),
                depth: 1,
                label: format!("◇ {block}: {} [[{},{}]]", code.name, facts.n, facts.k),
                mark: if facts.ok() { Mark::Ok } else { Mark::Error },
            });
        }
    }
    rows.push(Row {
        item: Item::Program,
        depth: 0,
        label: "Program".to_string(),
        mark: program_mark,
    });
    rows
}

/// An edit the tool can make from what it derived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Adopt {
    /// Add a check.
    Check(Vec<Term>),
    /// Write every readout.
    Readouts(Vec<String>),
    /// Add a frame for an output logical sign.
    Frame(String, Vec<Term>),
}

impl Adopt {
    /// The gadget document `body`, with this adopted.
    #[must_use]
    pub fn apply(&self, body: &str) -> String {
        match self {
            Adopt::Check(terms) => {
                bundle::append_item(body, "checks", &analysis::equation_text(terms))
            }
            Adopt::Readouts(lines) => {
                let block: Vec<String> = lines.iter().map(|l| format!("- {l}")).collect();
                bundle::replace_block(body, "readouts", &block)
            }
            Adopt::Frame(key, terms) => bundle::append_entry(
                body,
                "frames",
                &format!("\"{key}\": {}", analysis::equation_text(terms)),
            ),
        }
    }
}

/// One row of a gadget's findings.
#[derive(Clone, Debug, Default)]
pub struct Finding {
    /// Its text.
    pub tokens: Vec<Token>,
    /// What adopting it would do, if anything.
    pub adopt: Option<Adopt>,
    /// The call it is about, if any.
    pub call: Option<usize>,
}

fn heading(text: &str) -> Finding {
    Finding {
        tokens: vec![(Role::Name, text.to_string())],
        ..Finding::default()
    }
}

fn verdict_row(v: &Verdict, a: &Analysis) -> Finding {
    let mut tokens = Vec::new();
    if v.holds() {
        tokens.push((Role::Bit, "✓ ".to_string()));
        tokens.push((Role::Plain, v.text.clone()));
    } else if v.unknown {
        tokens.push((Role::Unresolved, "~ ".to_string()));
        tokens.push((Role::Plain, v.text.clone()));
        tokens.push((
            Role::Dim,
            "  can't tell past a step the analysis can't follow".to_string(),
        ));
    } else {
        tokens.push((Role::Error, "✗ ".to_string()));
        tokens.push((Role::Plain, v.text.clone()));
        match &v.residual {
            Ok(r) => tokens.push((Role::Error, format!("  sums to {}, not 0", a.vars.show(r)))),
            Err(e) => tokens.push((Role::Error, format!("  {e}"))),
        }
    }
    Finding {
        tokens,
        ..Finding::default()
    }
}

fn suggestion(text: String, adopt: Adopt) -> Finding {
    Finding {
        tokens: vec![
            (Role::Param, "+ ".to_string()),
            (Role::Plain, text),
            (Role::Dim, "  derived; Enter adopts".to_string()),
        ],
        adopt: Some(adopt),
        call: None,
    }
}

/// What the analysis of `gadget` found, as rows to read and adopt.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn findings(a: &Analysis, gadget: &Gadget) -> Vec<Finding> {
    let mut rows = Vec::new();
    let implemented = &gadget.implements;

    rows.push(heading("What each measurement measures"));
    if a.record.is_empty() {
        rows.push(Finding {
            tokens: vec![(Role::Dim, "  The circuit reports no bits.".to_string())],
            ..Finding::default()
        });
    }
    for (j, bit) in a.record.iter().enumerate() {
        let call = &a.calls[bit.call];
        let (role, kind) = match &bit.value {
            crate::stabilizer::Found::Random(_) => (Role::Unresolved, "is random"),
            crate::stabilizer::Found::Determined(_) => (Role::Bit, "="),
        };
        let mut tokens = vec![
            (Role::Block, format!("  readouts[{j}] ")),
            (Role::Dim, format!("{} ", call.text)),
        ];
        if !bit.what.is_empty() {
            tokens.push((Role::Dim, format!("({}) ", bit.what)));
        }
        tokens.push((role, kind.to_string()));
        if let crate::stabilizer::Found::Determined(form) = &bit.value {
            tokens.push((Role::Plain, format!(" {}", a.vars.show(form))));
        }
        rows.push(Finding {
            tokens,
            adopt: None,
            call: Some(bit.call),
        });
    }

    rows.push(heading("Checks"));
    for v in &a.checks {
        rows.push(verdict_row(v, a));
    }
    for c in &a.suggestions.checks {
        rows.push(suggestion(
            analysis::equation_text(c),
            Adopt::Check(c.clone()),
        ));
    }
    if a.checks.is_empty() && a.suggestions.checks.is_empty() {
        rows.push(Finding {
            tokens: vec![(Role::Dim, "  No parity of these bits is fixed.".to_string())],
            ..Finding::default()
        });
    }

    let observables = implemented.observe_count();
    let wanted = observables + implemented.flags.len();
    if wanted > 0 || !a.readouts.is_empty() {
        rows.push(heading("Readouts"));
        let names: Vec<String> = crate::vocab::readout_names(implemented);
        for (i, v) in a.readouts.iter().enumerate() {
            let mut r = verdict_row(v, a);
            r.tokens.insert(
                1,
                (
                    Role::Name,
                    format!("{} ", names.get(i).map_or("?", String::as_str)),
                ),
            );
            rows.push(r);
        }
        if a.readouts.len() < wanted {
            rows.push(Finding {
                tokens: vec![(
                    Role::Error,
                    format!(
                        "✗ {} readouts declared; `{}` reports {wanted}",
                        a.readouts.len(),
                        implemented.mnemonic
                    ),
                )],
                ..Finding::default()
            });
        }
        // Adopting writes them all: derived ones, or the declared ones for
        // flags the tool cannot derive.
        let derived = &a.suggestions.readouts;
        let declared: Vec<String> = gadget
            .readouts
            .iter()
            .map(|r| {
                let eq: Vec<String> = r
                    .equation
                    .iter()
                    .map(|t| match t {
                        qodec::ParityTerm::Bit(b) => u8::from(*b).to_string(),
                        qodec::ParityTerm::Reference(r) => format!("\"{r}\""),
                    })
                    .collect();
                let eq = format!("[{}]", eq.join(", "));
                match &r.name {
                    Some(n) => format!("{n}: {eq}"),
                    None => eq,
                }
            })
            .collect();
        let lines: Option<Vec<String>> = (0..wanted)
            .map(|i| {
                derived
                    .get(i)
                    .cloned()
                    .flatten()
                    .map(|t| {
                        let eq = analysis::equation_text(&t);
                        match i
                            .checked_sub(observables)
                            .and_then(|f| implemented.flags.get(f))
                        {
                            Some(flag) => format!("{flag}: {eq}"),
                            None => eq,
                        }
                    })
                    .or_else(|| declared.get(i).cloned())
            })
            .collect();
        let differs = lines.as_ref().is_some_and(|l| *l != declared);
        if let (Some(lines), true) = (lines, differs) {
            rows.push(suggestion(
                format!("readouts: {}", lines.join(", ")),
                Adopt::Readouts(lines),
            ));
        }
    }

    if !a.frames.is_empty() || !a.suggestions.frames.is_empty() {
        rows.push(heading("Frames"));
        for v in &a.frames {
            rows.push(verdict_row(v, a));
        }
        for ((e, basis, k), t) in &a.suggestions.frames {
            let key = format!(
                "out[{e}].{}[{k}]",
                match basis {
                    Basis::X => "x",
                    Basis::Z => "z",
                    Basis::Stabilizer => "stabilizers",
                }
            );
            rows.push(suggestion(
                format!("\"{key}\": {}", analysis::equation_text(t)),
                Adopt::Frame(key, t.clone()),
            ));
        }
    }

    if !a.out_stabilizers.is_empty() {
        rows.push(heading("Output stabilizers"));
        for s in &a.out_stabilizers {
            let name = format!("out[{}].stabilizers[{}]", s.e, s.k);
            let tokens = match (&s.sign, s.determined, a.followed) {
                (_, _, false) => vec![
                    (Role::Unresolved, "~ ".to_string()),
                    (Role::Plain, name),
                    (
                        Role::Dim,
                        "  can't tell past a step the analysis can't follow".to_string(),
                    ),
                ],
                (None, _, _) => vec![
                    (Role::Error, "✗ ".to_string()),
                    (Role::Plain, name),
                    (
                        Role::Error,
                        "  the output does not hold it: the block is not in its code".to_string(),
                    ),
                ],
                (Some(f), true, _) => vec![
                    (Role::Bit, "✓ ".to_string()),
                    (Role::Plain, name),
                    (Role::Dim, format!(" = {}", a.vars.show(f))),
                ],
                (Some(f), false, _) => vec![
                    (Role::Unresolved, "! ".to_string()),
                    (Role::Plain, name),
                    (Role::Dim, format!(" = {}", a.vars.show(f))),
                    (Role::Unresolved, "  no check determines it".to_string()),
                ],
            };
            rows.push(Finding {
                tokens,
                ..Finding::default()
            });
        }
    }

    rows.push(heading("Does it implement the instruction?"));
    if !a.action.compared {
        rows.push(Finding {
            tokens: vec![
                (Role::Unresolved, "~ ".to_string()),
                (
                    Role::Dim,
                    "can't tell: the analysis can't follow every step".to_string(),
                ),
            ],
            ..Finding::default()
        });
    } else if a.action.mismatches.is_empty() {
        rows.push(Finding {
            tokens: vec![
                (Role::Bit, "✓ ".to_string()),
                (
                    Role::Plain,
                    format!(
                        "it implements `{}`, with these readouts and frames",
                        implemented.mnemonic
                    ),
                ),
            ],
            ..Finding::default()
        });
    }
    for m in &a.action.mismatches {
        rows.push(Finding {
            tokens: vec![(Role::Error, "✗ ".to_string()), (Role::Plain, m.clone())],
            ..Finding::default()
        });
    }
    for p in &a.suggestions.problems {
        rows.push(Finding {
            tokens: vec![(Role::Unresolved, "! ".to_string()), (Role::Dim, p.clone())],
            ..Finding::default()
        });
    }
    for (c, w) in &a.warnings {
        rows.push(Finding {
            tokens: vec![
                (Role::Unresolved, "! ".to_string()),
                (Role::Dim, format!("call {c}: {w}")),
            ],
            adopt: None,
            call: Some(*c),
        });
    }
    for (c, n) in &a.notes {
        let at = if *c == usize::MAX {
            String::new()
        } else {
            format!("call {c}: ")
        };
        rows.push(Finding {
            tokens: vec![
                (Role::Unresolved, "~ ".to_string()),
                (Role::Dim, format!("{at}{n}")),
            ],
            adopt: None,
            call: (*c != usize::MAX).then_some(*c),
        });
    }
    rows
}

/// A new gadget for `mnemonic` in layer `k`: its file name, and its body.
#[must_use]
pub fn skeleton(docs: &[Doc], k: usize, mnemonic: &str) -> (String, String) {
    let layers = layer_files(docs);
    let dir = layers
        .get(k)
        .and_then(|l| l.gadgets.first())
        .and_then(|(_, f)| f.rfind('/').map(|i| f[..=i].to_string()))
        .unwrap_or_default();
    (
        format!("{dir}{mnemonic}.gadget.yaml"),
        "circuit:\n  source: []\n".to_string(),
    )
}

/// A call of instruction `index` of `lower`, ready to add at the end of the
/// circuit `analysis` read, as one inline YAML item. Operands it takes are the
/// live labels most recently used; operands it only gives are the output
/// labels that have no state yet, then new labels.
#[must_use]
pub fn default_call(lower: &InstructionSet, index: usize, analysis: Option<&Analysis>) -> String {
    let decl = &lower.instructions[index];
    let n = decl.inputs.len().max(decl.outputs.len());
    let labels: Vec<String> = analysis.map(|a| a.labels.clone()).unwrap_or_default();
    let mut live: Vec<String> = Vec::new();
    let mut unfilled: Vec<String> = Vec::new();
    // Labels by how recently a call used them, most recent first.
    let mut recent: Vec<String> = Vec::new();
    if let Some(a) = analysis {
        live = a.in_supports.iter().flatten().cloned().collect();
        for c in &a.calls {
            live.retain(|l| !c.ins.contains(l));
            live.extend(c.outs.iter().cloned());
        }
        unfilled = a
            .out_supports
            .iter()
            .flatten()
            .filter(|l| !live.contains(l))
            .cloned()
            .collect();
        for c in a.calls.iter().rev() {
            for l in c.ins.iter().chain(&c.outs) {
                if !recent.contains(l) {
                    recent.push(l.clone());
                }
            }
        }
    }
    for l in &labels {
        if !recent.contains(l) {
            recent.push(l.clone());
        }
    }
    let mut chosen: Vec<String> = Vec::new();
    let mut next_fresh = labels
        .iter()
        .filter_map(|l| l.parse::<u64>().ok())
        .max()
        .map_or(0, |m| m + 1);
    for j in 0..n {
        let pick = if decl.inputs.get(j).is_some() {
            recent
                .iter()
                .find(|l| live.contains(l) && !chosen.contains(l))
        } else {
            unfilled.iter().find(|l| !chosen.contains(l))
        };
        if let Some(l) = pick {
            chosen.push(l.clone());
        } else {
            chosen.push(next_fresh.to_string());
            next_fresh += 1;
        }
    }
    let mut parts: Vec<String> = chosen;
    for p in &decl.parameters {
        let value = match p.kind {
            ParameterKind::Bit | ParameterKind::Integer | ParameterKind::Number => "0",
            ParameterKind::Boolean => "false",
            ParameterKind::String => "\"\"",
            ParameterKind::Pauli => "Z_0",
        };
        parts.push(format!("{}: {value}", p.name));
    }
    format!("{}: [{}]", decl.mnemonic, parts.join(", "))
}
