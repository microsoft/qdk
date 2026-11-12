// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Draws the workbench's items: the map of the qodec, and each kind of item
//! the map opens.
//!
//! ```text
//! ┌Map──────────────┐┌Circuit · measure_z ──────────┐┌State after 3 ──────┐
//! │ repetition3      ││ in[0]──in0─CX─────────M→0──  ││ ZZ_ _  +in.S0      │
//! │ 0 logical (top)  ││ 0 ─────────CX──…            ││ _ZZ_   +in.S1      │
//! │  ✓ measure_z     │└──────────────────────────────┘└────────────────────┘
//! │  · prepare_x     │┌Parts──┐┌Text · gadget.yaml ─┐┌Findings ───────────┐
//! │  ◇ q: rep [[3,1]]││ MZ q  ││ circuit:           ││ ✓ readout 0 holds  │
//! │ Program          ││ CX q q ││   source:          ││ + check r0+r1      │
//! └──────────────────┘└────────┘└────────────────────┘└────────────────────┘
//! ```

use crate::{
    analysis::Analysis,
    app::{App, Focus, Target},
    circuit::{Ink, Layout as CircuitLayout, draw_row},
    codes,
    stabilizer::Found,
    state::Wiring,
    syntax::{self, Role, Token},
    ui::{dim, pane, spans, style},
    workbench::{self, Item, Mark},
};
use qodec::{Code, Qodec};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use std::{fmt::Write, sync::Arc};

fn mark_style(mark: Mark) -> Style {
    let s = Style::new().add_modifier(Modifier::BOLD);
    match mark {
        Mark::None => s,
        Mark::Ok => s.fg(Color::Green),
        Mark::Warn => s.fg(Color::Yellow),
        Mark::Error => s.fg(Color::Red),
        Mark::Unknown => s.fg(Color::Magenta),
        Mark::Missing => s.fg(Color::DarkGray),
    }
}

fn selected(line: Line<'static>, is: bool, focused: bool) -> Line<'static> {
    if !is {
        return line;
    }
    line.style(Style::new().add_modifier(if focused {
        Modifier::REVERSED
    } else {
        Modifier::UNDERLINED
    }))
}

/// The first of `n` rows to show so that row `at` is in a window `height` tall.
fn first_row(at: usize, height: usize) -> usize {
    at.saturating_sub(height.saturating_sub(1))
}

fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

// ── Map ─────────────────────────────────────────────────────────────────────

/// Draws the map: the whole qodec, with a mark on each part.
pub fn render_map(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Map;
    let rows = app.rows();
    app.map_row = app.map_row.min(rows.len().saturating_sub(1));
    let height = usize::from(area.height.saturating_sub(2));
    let first = first_row(app.map_row, height);
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(r, row)| {
            let mut spans = vec![Span::raw("  ".repeat(row.depth))];
            match row.item {
                Item::Instruction(..) => {
                    spans.push(Span::styled(row.mark.symbol(), mark_style(row.mark)));
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(row.label.clone(), style(Role::Mnemonic)));
                }
                Item::Code(..) => {
                    spans.push(Span::styled(row.label.clone(), style(Role::Block)));
                    if row.mark != Mark::Ok {
                        spans.push(Span::styled(
                            format!(" {}", row.mark.symbol()),
                            mark_style(row.mark),
                        ));
                    }
                }
                Item::Layer(_) => spans.push(Span::styled(row.label.clone(), style(Role::Name))),
                Item::Qodec | Item::Program => {
                    spans.push(Span::styled(
                        row.label.clone(),
                        Style::new().add_modifier(Modifier::BOLD),
                    ));
                    if row.mark != Mark::None {
                        spans.push(Span::styled(
                            format!(" {}", row.mark.symbol()),
                            mark_style(row.mark),
                        ));
                    }
                }
            }
            let shown = row.item == app.item;
            let mut line = selected(Line::from(spans), r == app.map_row, focused);
            if shown && r != app.map_row {
                line = line.style(Style::new().bg(Color::Rgb(40, 40, 48)));
            }
            line
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(pane("Map".to_string(), focused)),
        area,
    );
}

// ── Text ────────────────────────────────────────────────────────────────────

/// The roles of a line of YAML: comments dim, keys named, list dashes plain.
fn yaml_tokens(text: &str) -> Vec<Token> {
    let indent: String = text.chars().take_while(|c| *c == ' ').collect();
    let rest = &text[indent.len()..];
    let mut tokens = vec![(Role::Plain, indent)];
    if rest.starts_with('#') {
        tokens.push((Role::Comment, rest.to_string()));
        return tokens;
    }
    let (dash, rest) = match rest.strip_prefix("- ") {
        Some(r) => ("- ", r),
        None => ("", rest),
    };
    if !dash.is_empty() {
        tokens.push((Role::Dim, dash.to_string()));
    }
    let (body, comment) = match rest.find(" #") {
        Some(at) => (&rest[..at], &rest[at..]),
        None => (rest, ""),
    };
    match body
        .find(": ")
        .or_else(|| body.strip_suffix(':').map(str::len))
    {
        Some(at) if !body[..at].contains(['[', '"', '\'']) => {
            tokens.push((Role::Name, body[..at].to_string()));
            tokens.push((Role::Dim, ":".to_string()));
            tokens.push((Role::Value, body[at + 1..].to_string()));
        }
        _ => tokens.push((Role::Value, body.to_string())),
    }
    if !comment.is_empty() {
        tokens.push((Role::Comment, comment.to_string()));
    }
    tokens
}

/// Draws the focused item's text, with its line editor.
fn render_text(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Text;
    let Some((d, _)) = app.focused_doc() else {
        frame.render_widget(
            Paragraph::new(dim("This item has no text of its own."))
                .block(pane("Text".to_string(), focused)),
            area,
        );
        return;
    };
    let doc = &app.docs[d];
    let lines = doc.lines();
    let block = pane(format!("Text · {}", doc.name), focused);
    let inner = block.inner(area);
    let height = usize::from(inner.height);
    let first = first_row(app.text_line, height);
    let gutter = lines.len().max(1).to_string().len();
    let editing = app.editor.as_ref().filter(|_| app.target == Target::Doc(d));
    let mut shown = Vec::new();
    for (l, text) in lines.iter().enumerate().skip(first).take(height) {
        let is = l == app.text_line;
        let mut number = Style::new().fg(Color::DarkGray);
        if is && focused {
            number = number.fg(Color::White).add_modifier(Modifier::REVERSED);
        }
        let mut row = vec![Span::styled(format!("{:>gutter$} ", l + 1), number)];
        let text = match (editing, is) {
            (Some(editor), true) => &editor.text,
            _ => text,
        };
        row.extend(spans(yaml_tokens(text)));
        let mut row = Line::from(row);
        if is {
            row = row.style(Style::new().bg(Color::Rgb(40, 40, 48)));
        }
        shown.push(row);
    }
    if lines.is_empty() {
        shown.push(Line::from(dim("Empty. Press n to write a line.")));
    }
    frame.render_widget(Paragraph::new(shown).block(block), area);
    if let Some(editor) = editing {
        let x = inner.x + to_u16(gutter + 1 + editor.cursor);
        let y = inner.y + to_u16(app.text_line.saturating_sub(first));
        if x < inner.right() && y < inner.bottom() {
            frame.set_cursor_position(Position::new(x, y));
        }
    }
}

/// Draws one line under the item: why the text does not load or plan, or
/// `fallback` when it does.
fn render_status(frame: &mut Frame, app: &App, area: Rect, fallback: Vec<Span<'static>>) {
    let line = if let Some(e) = &app.load_error {
        vec![
            Span::styled(" ✗ The text does not load: ", style(Role::Error)),
            Span::raw(first_line(e)),
        ]
    } else if let Some(e) = &app.plan_error {
        vec![
            Span::styled(" ✗ The runtime can't plan it: ", style(Role::Error)),
            Span::raw(first_line(e)),
        ]
    } else {
        fallback
    };
    frame.render_widget(Paragraph::new(Line::from(line)), area);
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").to_string()
}

// ── Gadget ──────────────────────────────────────────────────────────────────

/// A gadget's circuit, ready to draw: pseudo-calls `in`/`out` for its
/// operands around its calls.
struct GadgetCircuit {
    wiring: Vec<Wiring>,
    labels: Vec<String>,
    kinds: Vec<CallKind>,
    /// Where the circuit's own calls start.
    offset: usize,
    layout: CircuitLayout,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CallKind {
    Port,
    Plain,
    Measures,
    Noted,
    Warned,
}

impl GadgetCircuit {
    fn of(a: &Analysis) -> Self {
        let row = |label: &String| -> u64 {
            a.labels
                .iter()
                .position(|l| l == label)
                .map_or(u64::MAX, |p| p as u64)
        };
        let rows = |labels: &[String]| -> Vec<u64> {
            labels.iter().map(row).filter(|r| *r != u64::MAX).collect()
        };
        let mut wiring = Vec::new();
        let mut labels = Vec::new();
        let mut kinds = Vec::new();
        for (e, support) in a.in_supports.iter().enumerate() {
            let outs = rows(support);
            wiring.push(Wiring {
                ins: Vec::new(),
                out_types: vec![0; outs.len()],
                outs,
            });
            labels.push(format!("in{e}"));
            kinds.push(CallKind::Port);
        }
        let offset = wiring.len();
        for (c, call) in a.calls.iter().enumerate() {
            let outs = rows(&call.outs);
            wiring.push(Wiring {
                ins: rows(&call.ins),
                out_types: vec![0; outs.len()],
                outs,
            });
            let mut label = call.mnemonic.clone();
            match call.record.len() {
                0 => {}
                1 => write!(label, "→{}", call.record.start).expect("writing to a string succeeds"),
                _ => write!(label, "→{}‥{}", call.record.start, call.record.end - 1)
                    .expect("writing to a string succeeds"),
            }
            labels.push(label);
            kinds.push(if a.warnings.iter().any(|(w, _)| *w == c) {
                CallKind::Warned
            } else if a.notes.iter().any(|(w, _)| *w == c) {
                CallKind::Noted
            } else if call.record.is_empty() {
                CallKind::Plain
            } else {
                CallKind::Measures
            });
        }
        for (e, support) in a.out_supports.iter().enumerate() {
            wiring.push(Wiring {
                ins: rows(support),
                outs: Vec::new(),
                out_types: Vec::new(),
            });
            labels.push(format!("out{e}"));
            kinds.push(CallKind::Port);
        }
        // Each port gets its own column, so inputs line up at the start.
        let ports_in = offset;
        let calls = a.calls.len();
        let batch = move |e: usize| {
            if e < ports_in {
                0
            } else if e < ports_in + calls {
                1
            } else {
                2
            }
        };
        let layout = CircuitLayout::new(&wiring, batch, |e| labels[e].chars().count());
        Self {
            wiring,
            labels,
            kinds,
            offset,
            layout,
        }
    }

    /// The drawn calls the cursor `c` selects: before any call, the inputs;
    /// after call `c`, that call.
    fn selected(&self, c: usize, calls: usize) -> Vec<usize> {
        if c == 0 {
            (0..self.offset).collect()
        } else if c > calls {
            Vec::new()
        } else {
            vec![self.offset + c - 1]
        }
    }
}

/// Draws a gadget: its circuit and state over time, the parts it is built
/// from, its text, and what the tool found.
pub fn render_gadget(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    k: usize,
    i: usize,
    status: Rect,
) {
    let [top, bottom] =
        Layout::vertical([Constraint::Percentage(50), Constraint::Min(6)]).areas(area);
    let state_width = (top.width * 2 / 5).clamp(24, 60);
    let [circuit, state] =
        Layout::horizontal([Constraint::Min(20), Constraint::Length(state_width)]).areas(top);
    let parts_width = gadget_parts_width(app, k).min(bottom.width / 5).max(14);
    let [parts, text, findings] = Layout::horizontal([
        Constraint::Length(parts_width),
        Constraint::Percentage(28),
        Constraint::Min(30),
    ])
    .areas(bottom);

    let a = app.analysis(k, i);
    let mnemonic = app.qodec.layers()[k].instruction_set.instructions[i]
        .mnemonic
        .clone();
    match a.as_deref() {
        Some(Ok(a)) => {
            render_gadget_circuit(frame, app, circuit, a, &mnemonic);
            render_gadget_state(frame, app, state, a);
            let fallback = gadget_status(app, a);
            render_status(frame, app, status, fallback);
        }
        Some(Err(e)) => {
            let text = Paragraph::new(vec![
                Line::from(Span::styled(
                    "The circuit can't be read:",
                    style(Role::Error),
                )),
                Line::from(e.clone()),
            ])
            .wrap(Wrap { trim: true })
            .block(pane(
                format!("Circuit · {mnemonic}"),
                app.focus == Focus::Circuit,
            ));
            frame.render_widget(text, top);
            render_status(frame, app, status, Vec::new());
        }
        None => render_status(frame, app, status, Vec::new()),
    }
    render_gadget_parts(frame, app, parts, k);
    render_text(frame, app, text);
    render_findings(frame, app, findings);
}

fn gadget_status(app: &App, a: &Analysis) -> Vec<Span<'static>> {
    let c = app.gadget_call;
    let mut line = vec![Span::styled(
        " State ",
        Style::new().add_modifier(Modifier::BOLD),
    )];
    if c == 0 {
        line.push(dim(if a.in_supports.is_empty() {
            "before the first call: empty, as the gadget takes no blocks"
        } else {
            "before the first call: the inputs, entangled with reference qubits"
        }));
        return line;
    }
    let Some(call) = a.calls.get(c - 1) else {
        return line;
    };
    line.push(dim(format!("after call {c} of {}: ", a.calls.len())));
    line.push(Span::styled(call.text.clone(), style(Role::Mnemonic)));
    for (_, note) in a.notes.iter().filter(|(w, _)| *w == c - 1) {
        line.push(Span::styled(
            format!("  ~ {note}"),
            Style::new().fg(Color::Magenta),
        ));
    }
    for (_, warning) in a.warnings.iter().filter(|(w, _)| *w == c - 1) {
        line.push(Span::styled(
            format!("  ! {warning}"),
            style(Role::Unresolved),
        ));
    }
    line
}

fn render_gadget_circuit(frame: &mut Frame, app: &App, area: Rect, a: &Analysis, mnemonic: &str) {
    let focused = app.focus == Focus::Circuit;
    let calls = a.calls.len();
    let title = if app.gadget_call == 0 {
        format!("Circuit · {mnemonic} · {calls} calls")
    } else {
        format!("Circuit · {mnemonic} · {}/{calls}", app.gadget_call)
    };
    let block = pane(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if calls == 0 && a.in_supports.is_empty() && a.out_supports.is_empty() {
        frame.render_widget(
            Paragraph::new(dim("No calls yet. Pick one in Parts and press Enter."))
                .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    let g = GadgetCircuit::of(a);
    let layout = &g.layout;
    let chosen = g.selected(app.gadget_call, calls);
    let gutter = a
        .labels
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        + 1;
    let width = usize::from(inner.width).saturating_sub(gutter);
    let height = usize::from(inner.height);

    // Columns: as many as fit, starting so the selected call is in view.
    let focus_column = chosen.first().map_or(0, |&e| layout.column_of[e]);
    let mut start = focus_column;
    let mut used = layout.widths.get(focus_column).map_or(0, |w| w + 1);
    while start > 0 && used + layout.widths[start - 1] < width {
        start -= 1;
        used += layout.widths[start] + 1;
    }
    let mut end = focus_column + 1;
    while end < layout.columns.len() && used + layout.widths[end] < width {
        used += layout.widths[end] + 1;
        end += 1;
    }
    let end = end.min(layout.columns.len());
    let focus_row = chosen
        .first()
        .and_then(|&e| layout.operand_rows[e].first().copied())
        .unwrap_or(0);
    let first = first_row(focus_row, height);

    let mut lines = Vec::new();
    for row in first..(first + height).min(layout.rows.len()) {
        let label = usize::try_from(layout.rows[row])
            .ok()
            .and_then(|r| a.labels.get(r))
            .cloned()
            .unwrap_or_default();
        let mut row_spans = vec![Span::styled(
            format!("{label:>w$} ", w = gutter - 1),
            style(Role::Block),
        )];
        for segment in draw_row(layout, &g.wiring, row, start..end, |e| g.labels[e].clone()) {
            let s = match segment.ink {
                Ink::Wire => Style::new().fg(Color::DarkGray),
                Ink::Gap => Style::new(),
                Ink::Gate(e) | Ink::Link(e) => {
                    let mut s = match g.kinds[e] {
                        CallKind::Port => Style::new().fg(Color::Green),
                        CallKind::Plain => Style::new().fg(Color::LightBlue),
                        CallKind::Measures => Style::new().fg(Color::Yellow),
                        CallKind::Noted => Style::new().fg(Color::Magenta),
                        CallKind::Warned => Style::new().fg(Color::Red),
                    };
                    if chosen.contains(&e) {
                        s = s.add_modifier(Modifier::REVERSED | Modifier::BOLD);
                    }
                    s
                }
            };
            row_spans.push(Span::styled(segment.text, s));
        }
        lines.push(Line::from(row_spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The letter of `pauli` on qubit `q`, styled.
fn letter(p: &crate::stabilizer::Pauli, q: usize) -> Span<'static> {
    match p.letter(q) {
        None => dim("·"),
        Some('X') => Span::styled("X", Style::new().fg(Color::LightRed)),
        Some('Z') => Span::styled("Z", Style::new().fg(Color::LightBlue)),
        Some(c) => Span::styled(c.to_string(), Style::new().fg(Color::LightMagenta)),
    }
}

fn render_gadget_state(frame: &mut Frame, app: &App, area: Rect, a: &Analysis) {
    let c = app.gadget_call.min(a.snapshots.len().saturating_sub(1));
    let title = if c == 0 {
        format!("State · inputs · {} generators", a.snapshots[0].gens.len())
    } else {
        format!(
            "State · after {c} · {} generators",
            a.snapshots[c].gens.len()
        )
    };
    let block = pane(title, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(snapshot) = a.snapshots.get(c) else {
        return;
    };
    // Only the qubits something acts on, so a big circuit stays readable.
    let used: Vec<usize> = (0..a.qubits.len())
        .filter(|&q| snapshot.gens.iter().any(|g| g.pauli.touches(q)))
        .collect();
    if snapshot.gens.is_empty() {
        frame.render_widget(
            Paragraph::new(dim("Nothing yet: no call has given a block a state."))
                .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    // A column per qubit, headed by its label's last character; `r` for
    // the reference qubits the inputs' logical qubits are entangled with.
    let mut header: Vec<Span> = used
        .iter()
        .map(|&q| {
            let name = &a.qubits[q];
            if name.starts_with("ref") {
                Span::styled("r", style(Role::Name))
            } else {
                Span::styled(
                    name.chars().last().unwrap_or(' ').to_string(),
                    style(Role::Block),
                )
            }
        })
        .collect();
    header.push(dim(" sign"));
    let mut lines = vec![Line::from(header)];
    for g in &snapshot.gens {
        let mut row: Vec<Span> = used.iter().map(|&q| letter(&g.pauli, q)).collect();
        row.push(Span::raw(" "));
        row.push(match a.vars.show(&g.sign).as_str() {
            "0" => dim("+"),
            "1" => Span::styled("−", style(Role::Bit)),
            sign => Span::styled(format!("(−1)^({sign})"), style(Role::Param)),
        });
        lines.push(Line::from(row));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn gadget_parts_width(app: &App, k: usize) -> u16 {
    let widest = app.qodec.layers().get(k + 1).map_or(10, |l| {
        l.instruction_set
            .instructions
            .iter()
            .map(|i| syntax::plain(&syntax::signature(i)).chars().count())
            .max()
            .unwrap_or(10)
    });
    to_u16(widest + 2)
}

fn render_gadget_parts(frame: &mut Frame, app: &App, area: Rect, k: usize) {
    let focused = app.focus == Focus::Parts;
    let Some(lower) = app.qodec.layers().get(k + 1) else {
        return;
    };
    let isa = &lower.instruction_set;
    let height = usize::from(area.height.saturating_sub(2));
    let first = first_row(app.gadget_part, height);
    let lines: Vec<Line> = isa
        .instructions
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(p, decl)| {
            let mut tokens = syntax::signature(decl);
            tokens.retain(|(role, _)| *role != Role::Dim);
            selected(Line::from(spans(tokens)), p == app.gadget_part, focused)
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(pane(format!("Parts · {}", isa.name), focused)),
        area,
    );
}

fn render_findings(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Findings;
    let rows = app.findings();
    app.finding = app.finding.min(rows.len().saturating_sub(1));
    let adoptable = rows.iter().filter(|r| r.adopt.is_some()).count();
    let title = if adoptable == 0 {
        "Findings".to_string()
    } else {
        format!("Findings · {adoptable} to adopt")
    };
    let height = usize::from(area.height.saturating_sub(2));
    let first = first_row(app.finding, height);
    let lines: Vec<Line> = rows
        .into_iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(r, row)| selected(Line::from(spans(row.tokens)), r == app.finding, focused))
        .collect();
    frame.render_widget(Paragraph::new(lines).block(pane(title, focused)), area);
}

// ── Code ────────────────────────────────────────────────────────────────────

/// The code of `block` at layer `k`: as the layer declares it, or as a gadget
/// encodes it.
fn code_of(qodec: &Qodec, k: usize, block: &str) -> Option<Arc<Code>> {
    let layer = qodec.layers().get(k)?;
    if let Some(code) = layer.codes.get(block) {
        return Some(code.clone());
    }
    layer.gadgets.values().find_map(|g| {
        g.inputs
            .iter()
            .chain(&g.outputs)
            .zip(g.implements.inputs.iter().chain(&g.implements.outputs))
            .find(|(_, op)| op.block == block)
            .map(|(enc, _)| enc.code.clone())
    })
}

/// Draws a code: its operators as a grid, what holds of them, and its text.
pub fn render_code(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    k: usize,
    block: &str,
    status: Rect,
) {
    let [grid, text] =
        Layout::vertical([Constraint::Percentage(55), Constraint::Min(5)]).areas(area);
    render_text(frame, app, text);
    let Some(code) = code_of(&app.qodec, k, block) else {
        render_status(
            frame,
            app,
            status,
            vec![dim(" This code is no longer bound to a block.")],
        );
        return;
    };
    let facts = codes::facts(&code);
    let [operators, holds] =
        Layout::horizontal([Constraint::Min(20), Constraint::Percentage(45)]).areas(grid);
    let name_width = facts
        .operators
        .iter()
        .map(|o| o.name.len())
        .max()
        .unwrap_or(2);
    let mut lines = vec![Line::from(dim(format!(
        "{:w$} {}",
        "",
        (0..facts.n)
            .map(|q| char::from_digit(u32::try_from(q % 10).unwrap_or(0), 10).unwrap_or(' '))
            .collect::<String>(),
        w = name_width
    )))];
    for o in &facts.operators {
        let role = match o.name.chars().next() {
            Some('S') => Style::new().fg(Color::Gray),
            Some('X') => Style::new()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
            _ => Style::new()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        };
        let mut row = vec![Span::styled(
            format!("{:>w$} ", o.name, w = name_width),
            role,
        )];
        match &o.pauli {
            Ok(p) => row.extend((0..facts.n).map(|q| letter(p, q))),
            Err(e) => row.push(Span::styled(e.clone(), style(Role::Error))),
        }
        lines.push(Line::from(row));
    }
    frame.render_widget(
        Paragraph::new(lines).block(pane(
            format!("{block}: {} [[{},{}]]", code.name, facts.n, facts.k),
            false,
        )),
        operators,
    );
    let fact_lines: Vec<Line> = facts
        .holds
        .iter()
        .map(|(ok, text)| {
            Line::from(vec![
                Span::styled(
                    if *ok { "✓ " } else { "✗ " },
                    mark_style(if *ok { Mark::Ok } else { Mark::Error }),
                ),
                Span::raw(text.clone()),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(fact_lines)
            .wrap(Wrap { trim: true })
            .block(pane("What holds".to_string(), false)),
        holds,
    );
    render_status(
        frame,
        app,
        status,
        vec![dim(format!(
            " {} stabilizers, {} logical qubits on {} physical qubits",
            facts
                .operators
                .iter()
                .filter(|o| o.name.starts_with('S'))
                .count(),
            facts.k,
            facts.n
        ))],
    );
}

// ── Other items ─────────────────────────────────────────────────────────────

/// Draws an item that is only text: the qodec, a layer, or an instruction
/// with no gadget yet.
pub fn render_doc_item(frame: &mut Frame, app: &mut App, area: Rect, status: Rect) {
    let [text, info] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Min(20)]).areas(area);
    render_text(frame, app, text);
    let lines = item_info(app);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(pane("About".to_string(), false)),
        info,
    );
    render_status(frame, app, status, Vec::new());
}

fn item_info(app: &App) -> Vec<Line<'static>> {
    let layers = app.qodec.layers();
    let last = layers.len().saturating_sub(1);
    let mut lines = Vec::new();
    match &app.item {
        Item::Qodec => {
            lines.push(Line::from(Span::styled(
                app.qodec.name().unwrap_or("unnamed").to_string(),
                Style::new().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(dim(format!(
                "{} layers, top to bottom: {}",
                layers.len(),
                layers
                    .iter()
                    .map(|l| l.instruction_set.name.clone())
                    .collect::<Vec<_>>()
                    .join(" ▸ ")
            ))));
            if let Some(e) = &app.load_error {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "The text does not load:",
                    style(Role::Error),
                )));
                lines.extend(e.lines().map(|l| Line::from(l.to_string())));
            }
            if let Some(e) = &app.plan_error {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "The runtime can't plan it; the program runs on the last qodec that did:",
                    style(Role::Error),
                )));
                lines.extend(e.lines().map(|l| Line::from(l.to_string())));
            }
        }
        Item::Layer(k) => {
            let Some(layer) = layers.get(*k) else {
                return lines;
            };
            let isa = &layer.instruction_set;
            lines.push(Line::from(Span::styled(
                isa.name.clone(),
                style(Role::Name),
            )));
            lines.push(Line::from(dim(if *k == last {
                "The bottom layer: the backend runs these instructions.".to_string()
            } else {
                format!(
                    "{} of {} instructions have gadgets.",
                    isa.instructions
                        .iter()
                        .filter(|i| layer.gadgets.contains_key(&i.mnemonic))
                        .count(),
                    isa.instructions.len()
                )
            })));
            lines.push(Line::from(""));
            for decl in &isa.instructions {
                lines.push(Line::from(spans(syntax::signature(decl))));
            }
        }
        Item::Instruction(k, i) => {
            let Some(decl) = layers
                .get(*k)
                .and_then(|l| l.instruction_set.instructions.get(*i))
            else {
                return lines;
            };
            lines.push(Line::from(spans(syntax::signature(decl))));
            if !decl.description.trim().is_empty() {
                lines.push(Line::from(decl.description.trim().to_string()));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(dim(if *k == last {
                "The bottom layer has no gadgets: the backend runs this instruction."
            } else {
                "No gadget yet. Press g in the map to give it an empty one, then build its circuit from Parts."
            })));
        }
        Item::Code(..) | Item::Program => {}
    }
    lines
}

// ── Explain ─────────────────────────────────────────────────────────────────

/// What the focused thing of the workbench means.
pub fn explain(app: &mut App, lines: &mut Vec<Line<'static>>) {
    match app.focus {
        Focus::Map => explain_map(app, lines),
        Focus::Text => explain_text(app, lines),
        Focus::Circuit => explain_call(app, lines),
        Focus::Findings => {
            let rows = app.findings();
            if let Some(row) = rows.get(app.finding) {
                lines.push(Line::from(spans(row.tokens.clone())));
                if row.adopt.is_some() {
                    lines.push(Line::from(dim("The tool derived this. Enter writes it into the gadget's text; A adopts every + row.")));
                }
            }
        }
        Focus::Parts => explain_gadget_part(app, lines),
        Focus::Program | Focus::View => {}
    }
}

fn explain_map(app: &mut App, lines: &mut Vec<Line<'static>>) {
    let rows = app.rows();
    let Some(row) = rows.get(app.map_row) else {
        return;
    };
    let meaning = match row.mark {
        Mark::Ok => "✓ the tool checked it and found nothing wrong.",
        Mark::Warn => "! it works, but something is worth a look.",
        Mark::Error => "✗ something is wrong with it.",
        Mark::Unknown => "~ it has a step (a rotation) the tool can't follow exactly.",
        Mark::Missing => "· it has no gadget yet: g makes one.",
        Mark::None => "",
    };
    let what = match &row.item {
        Item::Qodec => {
            "The qodec's manifest: its layers, and the files that make each.".to_string()
        }
        Item::Layer(k) => format!(
            "Layer {k}: an instruction set, and how each instruction lowers to the next layer."
        ),
        Item::Instruction(k, i) => {
            let decl = &app.qodec.layers()[*k].instruction_set.instructions[*i];
            format!(
                "{} — {}",
                syntax::plain(&syntax::signature(decl)),
                decl.description.trim()
            )
        }
        Item::Code(_, block) => format!("The code blocks of type `{block}` are encoded in."),
        Item::Program => "A program of top instructions, run through every layer.".to_string(),
    };
    lines.push(Line::from(what));
    let mut said = vec![Span::styled(meaning, mark_style(row.mark))];
    if let Item::Instruction(k, i) = row.item
        && let Some(a) = app.analysis(k, i)
        && let Ok(a) = a.as_ref()
    {
        let n = a.problems();
        if n > 0 {
            said.push(dim(format!(
                " {n} {} in its findings.",
                if n == 1 { "problem" } else { "problems" }
            )));
        }
    }
    if !meaning.is_empty() {
        lines.push(Line::from(said));
    }
    lines.push(Line::from(dim(
        "Enter opens it; Tab moves through its panes; Esc comes back here.",
    )));
}

fn explain_text(app: &App, lines: &mut Vec<Line<'static>>) {
    if let Some(e) = app.load_error.as_ref().or(app.plan_error.as_ref()) {
        lines.push(Line::from(Span::styled(first_line(e), style(Role::Error))));
    }
    let Some((d, _)) = app.focused_doc() else {
        return;
    };
    let editing = app.editor.is_some() && app.target == Target::Doc(d);
    lines.push(Line::from(dim(format!(
        "{} line {}. {}",
        app.docs[d].name,
        app.text_line + 1,
        if editing {
            "Everything is computed again as you type; Enter keeps the line, Esc puts it back."
        } else {
            "Enter edits the line; comments are kept when the tool writes to the text."
        }
    ))));
}

fn explain_call(app: &mut App, lines: &mut Vec<Line<'static>>) {
    let c = app.gadget_call;
    let Some(a) = app.focused_analysis() else {
        return;
    };
    let Ok(a) = a.as_ref() else {
        return;
    };
    if c == 0 {
        lines.push(Line::from(dim(if a.in_supports.is_empty() {
            "Before the first call: nothing. The gadget takes no blocks, so every qubit starts with the call that gives it a state."
        } else {
            "Before the first call: each input block is in its code, its logical qubits entangled with reference qubits, so every sign is a variable."
        })));
        return;
    }
    let Some(call) = a.calls.get(c - 1) else {
        return;
    };
    lines.push(Line::from(vec![
        Span::styled(call.text.clone(), style(Role::Mnemonic)),
        dim(format!(
            "  takes {} gives {}",
            list(&call.ins),
            list(&call.outs)
        )),
    ]));
    for j in call.record.clone() {
        let Some(bit) = a.record.get(j) else {
            continue;
        };
        let (how, form) = match &bit.value {
            Found::Determined(f) => ("determined", f),
            Found::Random(f) => ("random", f),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("record[{j}] "), style(Role::Bit)),
            dim(format!("measures {}: {how}, ", bit.what)),
            Span::styled(a.vars.show(form), style(Role::Param)),
        ]));
    }
}

fn list(labels: &[String]) -> String {
    if labels.is_empty() {
        "nothing".to_string()
    } else {
        labels.join(" ")
    }
}

fn explain_gadget_part(app: &mut App, lines: &mut Vec<Line<'static>>) {
    let Item::Instruction(k, _) = app.item else {
        return;
    };
    let Some(lower) = app
        .qodec
        .layers()
        .get(k + 1)
        .map(|l| l.instruction_set.clone())
    else {
        return;
    };
    let Some(decl) = lower.instructions.get(app.gadget_part) else {
        return;
    };
    let mut first = spans(syntax::signature(decl));
    if !decl.description.trim().is_empty() {
        first.push(dim("  "));
        first.push(Span::raw(decl.description.trim().to_string()));
    }
    lines.push(Line::from(first));
    let a = app.focused_analysis();
    let call = workbench::default_call(
        &lower,
        app.gadget_part,
        a.as_deref().and_then(|a| a.as_ref().ok()),
    );
    lines.push(Line::from(vec![
        dim("Enter adds "),
        Span::styled(call, Style::new().add_modifier(Modifier::BOLD)),
        dim(" at the end of the circuit."),
    ]));
}
