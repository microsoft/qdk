// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Draws an [`App`].
//!
//! ```text
//!  qodec-hi · name · top ▸ … ▸ bottom · bits: zero · 0 pinned          ? help
//! ┌Parts───────┐┌Program──────────────────┐┌Circuit · boundary ──────────────┐
//! │ the top    ││  1 prepare_z 0           ││    1        2                   │
//! │ ISA, to    ││  2 idle 0      ⟹ …       ││  0 R───CX────────M→0            │
//! │ add from   ││                          ││  3 R───CX──M→0                  │
//! └────────────┘└──────────────────────────┘└─────────────────────────────────┘
//!  Live at <boundary> after t 5: qubit 0–4
//! ┌Explain──────────────────────────────────────────────────────────────────┐
//! │ what the selected thing is and does, and where it came from             │
//! └─────────────────────────────────────────────────────────────────────────┘
//!  keys for the focused pane
//! ```

use crate::{
    app::{App, Focus, Mode, Spot, Target},
    circuit::{Ink, Layout as CircuitLayout, draw_row},
    shot::{Event, Outcome},
    syntax::{self, Role, Token},
    vocab,
    workbench::Item,
};
use qodec::{Instruction, InstructionSet};
use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent, KeyModifiers},
    layout::{Constraint, Layout, Margin, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Wrap},
};
use std::{collections::BTreeMap, fmt::Write, ops::Range};

/// The color of each program line, by index, in every pane.
const HUES: [Color; 6] = [
    Color::Cyan,
    Color::Magenta,
    Color::Green,
    Color::Yellow,
    Color::LightBlue,
    Color::LightRed,
];

pub(crate) fn hue(line: usize) -> Color {
    HUES[line % HUES.len()]
}

pub(crate) fn style(role: Role) -> Style {
    let s = Style::new();
    match role {
        Role::Mnemonic => s.fg(Color::LightBlue).add_modifier(Modifier::BOLD),
        Role::Block | Role::Value => s.fg(Color::Green),
        Role::Param => s.fg(Color::Yellow),
        Role::Plain => s,
        Role::Dim | Role::Comment => s.fg(Color::DarkGray),
        Role::Invalid => s.fg(Color::Red).add_modifier(Modifier::UNDERLINED),
        Role::Bit => s.add_modifier(Modifier::BOLD),
        Role::Pinned => s
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        Role::Unresolved => s.fg(Color::Yellow).add_modifier(Modifier::BOLD),
        Role::Name => s.fg(Color::Cyan),
        Role::Error => s.fg(Color::Red).add_modifier(Modifier::BOLD),
    }
}

pub(crate) fn spans(tokens: Vec<Token>) -> Vec<Span<'static>> {
    tokens
        .into_iter()
        .map(|(role, text)| Span::styled(text, style(role)))
        .collect()
}

pub(crate) fn dim(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), style(Role::Dim))
}

pub(crate) fn pane(title: String, focused: bool) -> Block<'static> {
    let border = if focused {
        Style::new().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::DarkGray)
    };
    Block::bordered().title(title).border_style(border)
}

fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// Makes the bordered pane at `area` clickable as `focus`, and so each row of
/// the list in it, which shows items `first..count`, as `spot` of its item.
pub(crate) fn list_spots(
    app: &mut App,
    area: Rect,
    focus: Focus,
    first: usize,
    count: usize,
    spot: impl Fn(usize) -> Spot,
) {
    app.spot(area, Spot::Pane(focus));
    let inner = area.inner(Margin::new(1, 1));
    for (k, index) in (first..count).take(usize::from(inner.height)).enumerate() {
        let row = Rect::new(inner.x, inner.y + to_u16(k), inner.width, 1);
        app.spot(row, spot(index));
    }
}

/// Draws `pieces` as one line in `area`, each clickable as its spot.
fn render_spotted(
    frame: &mut Frame,
    app: &mut App,
    area: Rect,
    pieces: Vec<(Span<'static>, Option<Spot>)>,
) {
    let mut x = area.x;
    let mut line = Vec::new();
    for (span, spot) in pieces {
        let width = to_u16(span.width());
        if let Some(spot) = spot {
            app.spot(Rect::new(x, area.y, width, 1).intersection(area), spot);
        }
        x = x.saturating_add(width);
        line.push(span);
    }
    frame.render_widget(Paragraph::new(Line::from(line)), area);
}

/// The key a hint in the keys line stands for, when it stands for one.
fn hint_key(hint: &str) -> Option<KeyEvent> {
    let code = match hint {
        "Enter" => KeyCode::Enter,
        "Esc" => KeyCode::Esc,
        "Tab" => KeyCode::Tab,
        "Space" => KeyCode::Char(' '),
        "Ctrl-S" => return Some(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)),
        _ => {
            let mut chars = hint.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => KeyCode::Char(c),
                _ => return None,
            }
        }
    };
    Some(KeyEvent::from(code))
}

/// Draws `app` on `frame`.
pub fn render(frame: &mut Frame, app: &mut App) {
    app.spots.clear();
    let [title, main, live, explain, keys] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(6),
        Constraint::Length(1),
        Constraint::Length(5),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let map_width = (main.width / 5).clamp(22, 34);
    let [map, item] =
        Layout::horizontal([Constraint::Length(map_width), Constraint::Min(20)]).areas(main);

    render_title(frame, app, title);
    crate::views::render_map(frame, app, map);
    match app.item.clone() {
        Item::Program => {
            let parts_width = parts_width(app.engine.top()).min((item.width * 3 / 10).max(16));
            let most = item.width * 36 / 100;
            let program_width = program_width(app).unwrap_or(most).min(most).max(20);
            let [parts, program, view] = Layout::horizontal([
                Constraint::Length(parts_width),
                Constraint::Length(program_width),
                Constraint::Min(20),
            ])
            .areas(item);
            render_parts(frame, app, parts);
            render_program(frame, app, program);
            match app.mode {
                Mode::Circuit => render_circuit(frame, app, view),
                Mode::Flow => render_flow(frame, app, view),
            }
            render_live(frame, app, live);
        }
        Item::Instruction(k, i) if app.has_gadget(k, i) => {
            crate::views::render_gadget(frame, app, item, k, i, live);
        }
        Item::Code(k, block) => crate::views::render_code(frame, app, item, k, &block, live),
        _ => crate::views::render_doc_item(frame, app, item, live),
    }
    render_explain(frame, app, explain);
    render_keys(frame, app, keys);
    if app.help {
        render_help(frame);
    }
}

fn render_title(frame: &mut Frame, app: &mut App, area: Rect) {
    let qodec = app.engine.qodec();
    let mut line: Vec<(Span<'static>, Option<Spot>)> = vec![
        (
            Span::styled(" qodec-hi ", Style::new().add_modifier(Modifier::REVERSED)),
            None,
        ),
        (Span::raw(" "), None),
        (
            Span::styled(
                qodec.name().unwrap_or("unnamed").to_string(),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            None,
        ),
    ];
    if let Some(label) = &app.label {
        line.push((dim(" · "), None));
        line.push((
            Span::styled(
                label.clone(),
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            None,
        ));
    }
    line.push((dim(" · "), None));
    for b in 0..app.engine.boundaries() {
        if b > 0 {
            line.push((dim(" ▸ "), None));
        }
        let name = app.engine.isa(b).name.clone();
        let span = if b == app.boundary {
            Span::styled(
                name,
                style(Role::Name).add_modifier(Modifier::REVERSED | Modifier::BOLD),
            )
        } else {
            Span::styled(name, style(Role::Name))
        };
        line.push((span, Some(Spot::Boundary(b))));
    }
    // While a line is edited, letters type rather than press.
    let typing = app.editor.is_some();
    let global = |c: &str| hint_key(c).filter(|_| !typing).map(Spot::Key);
    let bits = global("o");
    line.push((dim(" · bits: "), bits));
    line.push((
        Span::styled(
            app.outcomes.policy.to_string(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        bits,
    ));
    let pins = app.outcomes.pins.len();
    if pins > 0 {
        line.push((dim(", "), None));
        line.push((
            Span::styled(
                format!("{pins} pinned"),
                style(Role::Pinned).remove_modifier(Modifier::UNDERLINED),
            ),
            None,
        ));
    }
    if app.load_error.is_some() {
        line.push((
            Span::styled(" · the text does not load", style(Role::Error)),
            None,
        ));
    } else if app.plan_error.is_some() {
        line.push((
            Span::styled(" · the runtime can't plan it", style(Role::Error)),
            None,
        ));
    }
    if app.dirty() {
        line.push((Span::styled(" · unsaved", style(Role::Unresolved)), None));
    }
    line.push((dim(" · "), None));
    line.push((dim("? help"), global("?")));
    render_spotted(frame, app, area, line);
}

/// A part as a parts pane draws it: its signature, without what it reports,
/// which the explain pane tells.
pub(crate) fn part_line(decl: &Instruction) -> Line<'static> {
    let mut tokens = syntax::signature(decl);
    tokens.retain(|(role, _)| *role != Role::Dim);
    Line::from(spans(tokens))
}

pub(crate) fn parts_title(isa: &InstructionSet) -> String {
    format!("Parts · {}", isa.name)
}

/// The width a parts pane needs to show its title and every part of `isa`
/// whole, borders included.
pub(crate) fn parts_width(isa: &InstructionSet) -> u16 {
    let widest = isa
        .instructions
        .iter()
        .map(|decl| part_line(decl).width())
        .max()
        .unwrap_or(0);
    let title = Line::from(parts_title(isa)).width();
    to_u16(widest.max(title) + 2)
}

fn render_parts(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Parts;
    let isa = app.engine.top();
    let count = isa.instructions.len();
    let height = usize::from(area.height.saturating_sub(2));
    let first = app.part.saturating_sub(height.saturating_sub(1));
    let lines: Vec<Line> = isa
        .instructions
        .iter()
        .enumerate()
        .skip(first)
        .take(height)
        .map(|(k, decl)| {
            let mut line = part_line(decl);
            if k == app.part {
                line = line.style(Style::new().add_modifier(if focused {
                    Modifier::REVERSED
                } else {
                    Modifier::UNDERLINED
                }));
            }
            line
        })
        .collect();
    let title = parts_title(isa);
    list_spots(app, area, Focus::Parts, first, count, Spot::Part);
    frame.render_widget(Paragraph::new(lines).block(pane(title, focused)), area);
}

/// The results of line `index` as named bits, and what became of it.
fn line_tail(app: &App, index: usize) -> Vec<Span<'static>> {
    let run = &app.shot.lines[index];
    let top = app.engine.top();
    let mut tail = Vec::new();
    match &run.outcome {
        Outcome::Ran => {
            let mut first = true;
            if let Some(root) = run.root {
                for traced in &app.shot.nodes[root].calls {
                    let Some(readout) = &traced.readout else {
                        continue;
                    };
                    let decl = &top.instructions[traced.call.instruction_id];
                    tail.push(dim(if first { "  ⟹ " } else { " ; " }));
                    first = false;
                    tail.extend(spans(vocab::readout(decl, readout, &[])));
                }
            }
        }
        Outcome::Unparsed(_) => tail.push(Span::styled("  ✗ does not parse", style(Role::Error))),
        Outcome::Rejected(_) => tail.push(Span::styled("  ⊘ refused", style(Role::Unresolved))),
        Outcome::Lost(_) => tail.push(Span::styled("  ✗ lost the shot", style(Role::Error))),
        Outcome::NotRun => tail.push(dim("  (not run)")),
        Outcome::Blank => {}
    }
    tail
}

/// The width the program pane needs to show every line whole, borders
/// included, or `None` when there are no lines and it shows its hint.
fn program_width(app: &App) -> Option<u16> {
    if app.program.is_empty() {
        return None;
    }
    let gutter = app.program.len().to_string().len();
    let widest = app
        .program
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let text = match (&app.editor, index == app.line) {
                // One more column for the cursor after the last character.
                (Some(editor), true) => Line::from(editor.text.as_str()).width() + 1,
                _ => Line::from(line.text.as_str()).width(),
            };
            gutter + 2 + text + Line::from(line_tail(app, index)).width()
        })
        .max()
        .unwrap_or(0);
    Some(u16::try_from(widest + 2).unwrap_or(u16::MAX))
}

fn render_program(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Program;
    let block = pane("Program".to_string(), focused);
    let inner = block.inner(area);
    if app.program.is_empty() {
        app.spot(area, Spot::Pane(Focus::Program));
        let hint = Paragraph::new(vec![
            Line::from(dim("Nothing yet.")),
            Line::from(""),
            Line::from(dim("Pick an instruction in Parts and press Enter,")),
            Line::from(dim("or press Tab to come here and n to type one.")),
        ])
        .wrap(Wrap { trim: true })
        .block(block);
        frame.render_widget(hint, area);
        return;
    }
    let height = usize::from(inner.height);
    let first = app.line.saturating_sub(height.saturating_sub(1));
    let gutter = app.program.len().to_string().len();
    let mut lines = Vec::new();
    for (index, line) in app.program.iter().enumerate().skip(first).take(height) {
        let selected = index == app.line;
        let mut number = Style::new().fg(hue(index));
        if selected {
            number = number.add_modifier(Modifier::REVERSED | Modifier::BOLD);
        }
        let mut row = vec![
            Span::styled(format!("{:>gutter$} ", index + 1), number),
            Span::raw(" "),
        ];
        let outcome = &app.shot.lines[index].outcome;
        let text = match (&app.editor, selected) {
            (Some(editor), true) => syntax::highlight(app.engine.top(), &editor.text),
            _ => syntax::highlight(app.engine.top(), &line.text),
        };
        let mut text = spans(text);
        if *outcome == Outcome::NotRun {
            for span in &mut text {
                span.style = style(Role::Dim);
            }
        }
        row.extend(text);
        row.extend(line_tail(app, index));
        let mut row = Line::from(row);
        if selected {
            row = row.style(Style::new().bg(Color::Rgb(40, 40, 48)));
        }
        lines.push(row);
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
    let count = app.program.len();
    list_spots(app, area, Focus::Program, first, count, Spot::Line);
    if let (Some(editor), true) = (&app.editor, app.line >= first) {
        let x = inner.x + u16::try_from(gutter + 2 + editor.cursor).unwrap_or(u16::MAX);
        let y = inner.y + u16::try_from(app.line - first).unwrap_or(u16::MAX);
        if x < inner.right() && y < inner.bottom() {
            frame.set_cursor_position(Position::new(x, y));
        }
    }
    if app.editor.is_some() && app.target == Target::Program && app.line >= first {
        let y = inner.y + to_u16(app.line - first);
        if y < inner.bottom() {
            let row = Rect::new(inner.x, y, inner.width, 1);
            app.spot(row, Spot::Caret(inner.x + to_u16(gutter + 2)));
        }
    }
}

fn boundary_title(app: &App, prefix: &str) -> String {
    let b = app.boundary;
    let n = app.engine.boundaries();
    let role = match (b, n - 1) {
        (0, 0) => " (top, bottom)",
        (0, _) => " (top)",
        (b, last) if b == last => " (bottom)",
        _ => "",
    };
    format!("{prefix} {b}: {}{role}", app.engine.isa(b).name)
}

fn render_circuit(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::View;
    let n = app.timeline().len();
    let mut title = boundary_title(app, "Circuit");
    if let Some(t) = app.cursor {
        write!(title, " · {}/{n}", t + 1).expect("writing to a string succeeds");
    }
    let block = pane(title, focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.spot(area, Spot::Pane(Focus::View));
    if n == 0 {
        let text = Paragraph::new(dim(
            "Nothing has run at this boundary yet. Add a line to the program.",
        ))
        .wrap(Wrap { trim: true });
        frame.render_widget(text, inner);
        return;
    }

    let b = app.boundary;
    let cursor = app.cursor;
    let selected_line = app.line;
    let timeline: Vec<Event> = app.timeline().to_vec();
    let line_of: Vec<usize> = timeline.iter().map(|&e| app.shot.line_of(e)).collect();
    let pinned: Vec<bool> = timeline
        .iter()
        .map(|&e| app.shot.traced(e).bits.iter().any(|&(_, p)| p))
        .collect();
    let mut scroll = app.scroll;
    let view = app.view(b);
    let layout = &view.layout;
    let gutter = layout.rows.last().map_or(1, |r| r.to_string().len()).max(1) + 1;
    let width = usize::from(inner.width).saturating_sub(gutter);
    let height = usize::from(inner.height).saturating_sub(1);
    let columns = keep_in_view(layout, cursor, &mut scroll, width, height);

    let mut lines = vec![ruler(
        layout,
        &line_of,
        selected_line,
        gutter,
        columns.clone(),
    )];
    let mut call_spots = Vec::new();
    for row in scroll.1..(scroll.1 + height).min(layout.rows.len()) {
        let mut spans = vec![dim(format!("{:>w$} ", layout.rows[row], w = gutter - 1))];
        let top = inner.y + 1 + to_u16(row - scroll.1);
        let mut left = inner.x + to_u16(gutter);
        for segment in draw_row(layout, &view.lives.wiring, row, columns.clone(), |e| {
            view.labels[e].clone()
        }) {
            let (s, call) = match segment.ink {
                Ink::Wire => (Style::new().fg(Color::DarkGray), None),
                Ink::Gap => (Style::new(), None),
                Ink::Gate(e) | Ink::Link(e) => {
                    let mut s = Style::new().fg(hue(line_of[e]));
                    if line_of[e] == selected_line {
                        s = s.add_modifier(Modifier::BOLD);
                    }
                    if matches!(segment.ink, Ink::Gate(_)) && pinned[e] {
                        s = s.add_modifier(Modifier::UNDERLINED);
                    }
                    if Some(e) == cursor {
                        s = s.add_modifier(Modifier::REVERSED);
                    }
                    (s, Some(Spot::Event(e)))
                }
            };
            let span = Span::styled(segment.text, s);
            let width = to_u16(span.width());
            if let Some(call) = call {
                call_spots.push((Rect::new(left, top, width, 1).intersection(inner), call));
            }
            left = left.saturating_add(width);
            spans.push(span);
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), inner);
    app.scroll = scroll;
    for (area, spot) in call_spots {
        app.spot(area, spot);
    }
}

/// Scrolls so the selected call is in a `width` by `height` view, and
/// returns the columns that fit.
fn keep_in_view(
    layout: &CircuitLayout,
    cursor: Option<usize>,
    scroll: &mut (usize, usize),
    width: usize,
    height: usize,
) -> Range<usize> {
    if let Some(t) = cursor {
        let column = layout.column_of[t];
        if column < scroll.0 {
            scroll.0 = column;
        }
        while scroll.0 < column
            && layout.widths[scroll.0..=column]
                .iter()
                .map(|w| w + 1)
                .sum::<usize>()
                > width
        {
            scroll.0 += 1;
        }
        if let Some(&row) = layout.operand_rows[t].first() {
            if row < scroll.1 {
                scroll.1 = row;
            } else if row >= scroll.1 + height {
                scroll.1 = row + 1 - height;
            }
        }
    }
    scroll.0 = scroll.0.min(layout.widths.len().saturating_sub(1));
    scroll.1 = scroll.1.min(layout.rows.len().saturating_sub(height));
    let mut end = scroll.0;
    let mut used = 0;
    while end < layout.widths.len() && used + layout.widths[end] < width {
        used += layout.widths[end] + 1;
        end += 1;
    }
    scroll.0..end.max(scroll.0 + 1).min(layout.widths.len())
}

/// The ruler over the circuit: the program line each stretch of `columns`
/// belongs to, in its color.
fn ruler(
    layout: &CircuitLayout,
    line_of: &[usize],
    selected_line: usize,
    gutter: usize,
    columns: Range<usize>,
) -> Line<'static> {
    let mut ruler = vec![Span::raw(" ".repeat(gutter))];
    let mut previous = None;
    for column in columns {
        let w = layout.widths[column] + 1;
        let line = layout.columns[column].first().map(|&e| line_of[e]);
        let text = match line {
            Some(l) if Some(l) != previous => format!("{:<w$}", l + 1),
            _ => " ".repeat(w),
        };
        let mut s = Style::new().fg(line.map_or(Color::DarkGray, hue));
        if line == Some(selected_line) {
            s = s.add_modifier(Modifier::BOLD);
        }
        ruler.push(Span::styled(text, s));
        previous = line;
    }
    Line::from(ruler)
}

/// The batches of the selected line, a boundary per indent.
fn flow_lines(app: &App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let Some(run) = app.shot.lines.get(app.line) else {
        return lines;
    };
    let text = &app.program[app.line].text;
    lines.push(Line::from(vec![
        Span::styled(
            format!(" line {} ", app.line + 1),
            Style::new().fg(hue(app.line)),
        ),
        Span::styled(text.clone(), Style::new().add_modifier(Modifier::BOLD)),
    ]));
    let Some(root) = run.root else {
        lines.push(Line::from(dim(
            " It did not run, so it sent nothing below.",
        )));
        return lines;
    };
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = &app.shot.nodes[id];
        let isa = app.engine.isa(node.boundary);
        let indent = format!(" {}", "╎ ".repeat(node.boundary));
        lines.push(Line::from(vec![
            dim(indent.clone()),
            Span::styled(
                format!("{}: {}", node.boundary, isa.name),
                style(Role::Name),
            ),
            dim(format!(" · {}", vocab::count(node.calls.len(), "call"))),
        ]));
        for traced in &node.calls {
            let mut row = vec![dim(format!("{indent}  "))];
            row.extend(spans(syntax::render(isa, &traced.call)));
            if let Some(readout) = &traced.readout {
                let decl = &isa.instructions[traced.call.instruction_id];
                row.push(dim(" → "));
                row.extend(spans(vocab::readout(decl, readout, &traced.bits)));
            }
            lines.push(Line::from(row));
        }
        if let Some(fault) = &node.fault {
            lines.push(Line::from(vec![
                dim(format!("{indent}  ")),
                Span::styled(format!("fault: {fault}"), style(Role::Error)),
            ]));
        }
        stack.extend(node.children.iter().rev());
    }
    lines
}

fn render_flow(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::View;
    let lines = flow_lines(app);
    let height = usize::from(area.height.saturating_sub(2));
    app.flow_scroll = app.flow_scroll.min(lines.len().saturating_sub(height));
    let title = format!("Flow · line {} through every layer", app.line + 1);
    let scroll = u16::try_from(app.flow_scroll).unwrap_or(u16::MAX);
    app.spot(area, Spot::Pane(Focus::View));
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll, 0))
            .block(pane(title, focused)),
        area,
    );
}

/// `ids` as ranges: `0–3, 7`.
fn ranges(ids: &[u64]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut k = 0;
    while k < ids.len() {
        let mut j = k;
        while j + 1 < ids.len() && ids[j + 1] == ids[j] + 1 {
            j += 1;
        }
        parts.push(match j - k {
            0 => ids[k].to_string(),
            1 => format!("{}, {}", ids[k], ids[j]),
            _ => format!("{}–{}", ids[k], ids[j]),
        });
        k = j + 1;
    }
    parts.join(", ")
}

fn render_live(frame: &mut Frame, app: &mut App, area: Rect) {
    let b = app.boundary;
    let cursor = app.cursor;
    let name = app.engine.isa(b).name.clone();
    let view = app.view(b);
    let mut by_type: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    for (label, (block_type, _)) in view.lives.live_after(cursor) {
        by_type.entry(block_type).or_default().push(label);
    }
    let when = cursor.map_or("before the first call".to_string(), |t| {
        format!("after call {}", t + 1)
    });
    let mut line = vec![
        Span::styled(" Live ", Style::new().add_modifier(Modifier::BOLD)),
        dim(format!("at {name} {when}: ")),
    ];
    if by_type.is_empty() {
        line.push(dim("no blocks"));
    }
    for (k, (block_type, ids)) in by_type.iter().enumerate() {
        if k > 0 {
            line.push(dim(" · "));
        }
        line.push(Span::styled(format!("{block_type} "), style(Role::Name)));
        line.push(Span::styled(ranges(ids), style(Role::Block)));
    }
    frame.render_widget(Paragraph::new(Line::from(line)), area);
}

/// The lines of the explain pane: what the focused thing means.
fn explanation(app: &mut App) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    if let Some(message) = &app.message {
        lines.push(Line::from(Span::styled(
            message.clone(),
            Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
        )));
    }
    match app.focus {
        Focus::Parts if app.item == Item::Program => explain_part(app, &mut lines),
        Focus::Program => explain_line(app, &mut lines),
        Focus::View => match app.mode {
            Mode::Circuit => explain_event(app, &mut lines),
            Mode::Flow => explain_line(app, &mut lines),
        },
        Focus::Map | Focus::Text | Focus::Circuit | Focus::Findings | Focus::Parts => {
            crate::views::explain(app, &mut lines);
        }
    }
    lines
}

/// The signature, description and implementation of instruction `index` of
/// boundary `b`.
fn explain_instruction(app: &App, b: usize, index: usize, lines: &mut Vec<Line<'static>>) {
    let Some(decl) = app.engine.isa(b).instructions.get(index) else {
        return;
    };
    let mut first = spans(syntax::signature(decl));
    if !decl.description.trim().is_empty() {
        first.push(dim("  "));
        first.push(Span::raw(decl.description.trim().to_string()));
    }
    lines.push(Line::from(first));
    lines.push(Line::from(dim(vocab::implementation(
        app.engine.qodec(),
        b,
        &decl.mnemonic,
    ))));
}

fn explain_part(app: &mut App, lines: &mut Vec<Line<'static>>) {
    if app.engine.top().instructions.is_empty() {
        lines.push(Line::from(dim(
            "The top instruction set has no instructions.",
        )));
        return;
    }
    explain_instruction(app, 0, app.part, lines);
    let live = app.live_after_line();
    let text = vocab::default_line(app.engine.top(), app.part, &live);
    let after = if app.program.is_empty() {
        "as the first line".to_string()
    } else {
        format!("after line {}", app.line + 1)
    };
    lines.push(Line::from(vec![
        dim("Enter adds "),
        Span::styled(text, Style::new().add_modifier(Modifier::BOLD)),
        dim(format!(" {after}; e adds it and edits it.")),
    ]));
}

fn explain_line(app: &App, lines: &mut Vec<Line<'static>>) {
    if let Some(editor) = &app.editor {
        let top = app.engine.top();
        let mnemonic = editor.text.split_whitespace().next().unwrap_or("");
        if let Some(index) = top.instructions.iter().position(|i| i.mnemonic == mnemonic) {
            explain_instruction(app, 0, index, lines);
        }
        if let Some(Outcome::Unparsed(e) | Outcome::Rejected(e) | Outcome::Lost(e)) =
            app.line_outcome()
        {
            lines.push(Line::from(Span::styled(e.clone(), style(Role::Error))));
        }
        return;
    }
    let Some(run) = app.shot.lines.get(app.line) else {
        lines.push(Line::from(dim(
            "The program is empty. Add a line from Parts with Enter.",
        )));
        return;
    };
    match &run.outcome {
        Outcome::Ran => {
            let first = run.calls.first().map_or(0, |c| c.instruction_id);
            explain_instruction(app, 0, first, lines);
            if let Some(root) = run.root {
                for traced in &app.shot.nodes[root].calls {
                    if let Some(readout) = &traced.readout {
                        for (name, meaning) in vocab::meanings(readout.status) {
                            lines.push(Line::from(vec![
                                Span::styled(name, style(Role::Unresolved)),
                                dim(format!(": {meaning}.")),
                            ]));
                        }
                    }
                }
            }
        }
        Outcome::Unparsed(e) => {
            lines.push(Line::from(Span::styled(e.clone(), style(Role::Error))));
            lines.push(Line::from(dim(
                "Nothing ran. Press Enter to edit the line; Tab completes names.",
            )));
        }
        Outcome::Rejected(e) => {
            lines.push(Line::from(Span::styled(
                format!("Refused: {e}"),
                style(Role::Unresolved),
            )));
            let mut hint = "Nothing ran, and the shot goes on.".to_string();
            if e.contains("not live") {
                let top = app.engine.top();
                let types: Vec<&str> = top.blocks.iter().map(|b| b.name.as_str()).collect();
                let makers: Vec<String> = types
                    .iter()
                    .flat_map(|t| vocab::producers(top, t))
                    .collect();
                if !makers.is_empty() {
                    write!(
                        hint,
                        " A block is live once an instruction gives it, such as {}.",
                        makers.join(", ")
                    )
                    .expect("writing to a string succeeds");
                }
            }
            lines.push(Line::from(dim(hint)));
        }
        Outcome::Lost(e) => {
            lines.push(Line::from(Span::styled(e.clone(), style(Role::Error))));
            lines.push(Line::from(dim(
                "This lost the shot: the lines after it do not run. Edit or delete it.",
            )));
        }
        Outcome::NotRun => {
            let lost = app
                .shot
                .lines
                .iter()
                .position(|l| matches!(l.outcome, Outcome::Lost(_)))
                .map_or(0, |k| k + 1);
            lines.push(Line::from(dim(format!(
                "Line {lost} lost the shot, so this line does not run."
            ))));
        }
        Outcome::Blank => lines.push(Line::from(dim("A blank line or a comment."))),
    }
}

fn explain_event(app: &App, lines: &mut Vec<Line<'static>>) {
    let Some(event) = app.event() else {
        lines.push(Line::from(dim("No call is selected.")));
        return;
    };
    let b = app.boundary;
    let isa = app.engine.isa(b);
    let traced = app.shot.traced(event);
    let mut first = spans(syntax::render(isa, &traced.call));
    if let Some(readout) = &traced.readout {
        let decl = &isa.instructions[traced.call.instruction_id];
        first.push(dim(" → "));
        first.extend(spans(vocab::readout(decl, readout, &traced.bits)));
    }
    if let Some(decl) = isa.instructions.get(traced.call.instruction_id)
        && !decl.description.trim().is_empty()
    {
        first.push(dim("  "));
        first.push(Span::raw(decl.description.trim().to_string()));
    }
    lines.push(Line::from(first));

    // Where it came from: the batch it is in, and every batch above it.
    let mut path = Vec::new();
    let mut node = Some(event.node);
    while let Some(id) = node {
        path.push(id);
        node = app.shot.nodes[id].parent;
    }
    let index = app.shot.line_of(event);
    let mut from = vec![
        dim("From "),
        Span::styled(format!("line {}", index + 1), Style::new().fg(hue(index))),
        dim(" "),
    ];
    from.extend(spans(syntax::highlight(
        app.engine.top(),
        &app.program[index].text,
    )));
    for &id in path.iter().rev().skip(1) {
        let parent = app.shot.nodes[id].parent.expect("a node below the top");
        let k = app.shot.nodes[parent]
            .children
            .iter()
            .position(|&c| c == id)
            .unwrap_or(0);
        let node = &app.shot.nodes[id];
        from.push(dim(" ▸ "));
        from.push(Span::styled(
            app.engine.isa(node.boundary).name.clone(),
            style(Role::Name),
        ));
        from.push(dim(format!(
            " batch {} ({})",
            k + 1,
            vocab::count(node.calls.len(), "call")
        )));
    }
    lines.push(Line::from(from));

    let hint = if !traced.bits.is_empty() {
        let pinned = traced.bits.iter().filter(|&&(_, p)| p).count();
        let what = if pinned > 0 {
            format!("{} pinned", vocab::count(pinned, "bit"))
        } else {
            format!("bits made up as {}", app.outcomes.policy)
        };
        let keys = if traced.bits.len() == 1 {
            "Space flips it".to_string()
        } else {
            format!("1–{} flip a bit", traced.bits.len())
        };
        format!("{what}. {keys}, x unpins; every result above it follows.")
    } else if traced.readout.is_some() {
        "Decoded by the layer below from the bits it measured; ] goes down to them.".to_string()
    } else if b + 1 < app.engine.boundaries() {
        "] goes down to the calls it became; [ goes up to the call it is part of.".to_string()
    } else {
        "[ goes up to the call it is part of.".to_string()
    };
    lines.push(Line::from(dim(hint)));
}

pub(crate) fn render_explain(frame: &mut Frame, app: &mut App, area: Rect) {
    let lines = explanation(app);
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().border_style(Style::new().fg(Color::DarkGray))),
        area,
    );
}

fn render_keys(frame: &mut Frame, app: &mut App, area: Rect) {
    let keys: &[(&str, &str)] = if app.editor.is_some() {
        if app.target == Target::Program {
            &[
                ("Enter", "keep"),
                ("Esc", "cancel"),
                ("Tab", "complete"),
                ("←→", "move"),
            ]
        } else {
            &[("Enter", "keep"), ("Esc", "cancel"), ("←→", "move")]
        }
    } else {
        match app.focus {
            Focus::Map => &[
                ("↑↓", "item"),
                ("Enter", "open"),
                ("g", "new gadget"),
                ("Tab", "next pane"),
                ("Ctrl-S", "save"),
            ],
            Focus::Circuit => &[
                ("←→", "step"),
                ("Home/End", "start/end"),
                ("Tab", "next pane"),
            ],
            Focus::Findings => &[
                ("↑↓", "row"),
                ("Enter", "adopt"),
                ("A", "adopt all"),
                ("Tab", "next pane"),
            ],
            Focus::Parts if app.item != Item::Program => &[
                ("↑↓", "choose"),
                ("Enter", "add to circuit"),
                ("Tab", "next pane"),
            ],
            Focus::Parts => &[
                ("↑↓", "choose"),
                ("Enter", "add"),
                ("e", "add+edit"),
                ("Tab", "next pane"),
            ],
            Focus::Program | Focus::Text => &[
                ("↑↓", "line"),
                ("Enter", "edit"),
                ("n", "new"),
                ("d", "delete"),
                ("y", "copy"),
                ("J/K", "move"),
                ("u/U", "undo/redo"),
            ],
            Focus::View if app.mode == Mode::Flow => {
                &[("↑↓", "scroll"), ("v", "circuit"), ("Tab", "next pane")]
            }
            Focus::View => &[
                ("←→", "time"),
                ("PgUp/Dn", "line"),
                ("[ ]", "layer"),
                ("Space", "flip bit"),
                ("v", "flow"),
            ],
        }
    };
    let mut line = Vec::new();
    for (key, what) in keys {
        let spot = hint_key(key).map(Spot::Key);
        line.push((
            Span::styled(
                format!(" {key} "),
                Style::new().add_modifier(Modifier::REVERSED),
            ),
            spot,
        ));
        line.push((dim(format!(" {what}")), spot));
        line.push((dim("  "), None));
    }
    // While a line is edited, letters type rather than press.
    let typing = app.editor.is_some();
    let global = |c: &str| hint_key(c).filter(|_| !typing).map(Spot::Key);
    line.push((dim("o bits"), global("o")));
    line.push((dim("  "), None));
    line.push((dim("q quit"), global("q")));
    render_spotted(frame, app, area, line);
}

const HELP: &str = "\
Build a qodec and watch it lower a program, in one loop: change any text, and
everything that follows from it is computed again at once.

The map (left) is the whole qodec: each layer's instruction set, its
instructions with their gadgets, its codes, and the program. Marks: ✓ verified,
! worth a look, ✗ wrong, ~ can't tell (a rotation), · no gadget yet. Enter
opens an item; g gives an instruction a new, empty gadget.

A gadget shows its circuit (a time cursor steps through it, and the state at
the cursor is shown beside it), the instruction set below as parts to add,
its text, and what the tool found: what each measurement measures, which
checks, readouts and frames hold, which it derives (+, Enter adopts, A adopts
all), and whether the circuit implements its instruction.

A code shows its operators as a grid, and whether they fit together. Any
item's text is edited a line at a time: Enter edits, n new, d delete, y copy,
J/K move.

The program is run through the whole stack: Parts adds a top instruction,
Program edits lines, the circuit shows any layer ([ ]), v shows a line's flow,
and Space flips a bottom bit.

Everywhere: Tab next pane · Esc back to the map · u/U undo/redo ·
o bits zero/one/random · Ctrl-S save the qodec and program · q quit.

The mouse: a click focuses a pane and selects what is under it; a double
click does what Enter would (on a call, ] instead). A right click on a bottom
call flips its bit. The wheel moves through a pane; with Shift, through a
circuit's time. Click a layer in the title to show it, and a key below to
press it.

Press any key to close.";

fn render_help(frame: &mut Frame) {
    let area = frame.area();
    let width = 82.min(area.width);
    let height = 34.min(area.height);
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(HELP)
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(" Help ")),
        popup,
    );
}
