// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The interface's state, and what each key does to it. Nothing here draws:
//! [`crate::ui`] renders an [`App`], so the keys can be tested without a
//! terminal.
//!
//! The program and the outcomes are the only inputs. Every change to either
//! runs the shot again from the start ([`App::rerun`]), so what is on screen
//! always follows from what the program says.

use crate::{
    analysis::{self, Analysis},
    bundle::{self, Doc, Source},
    circuit::Layout,
    shot::{Engine, Event, Line, Outcome, Shot},
    state::Lives,
    syntax,
    trace::{Outcomes, Policy},
    vocab,
    workbench::{self, Item, Mark, Row},
};
use qodec::Qodec;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{collections::BTreeMap, path::PathBuf, rc::Rc};

/// Which pane keys go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    /// The map of the qodec.
    Map,
    /// The instructions to add: the top ones to the program, the lower ones
    /// to a gadget's circuit.
    Parts,
    /// The program's lines.
    Program,
    /// The circuit or flow of the shown boundary.
    View,
    /// The focused item's text.
    Text,
    /// A gadget's circuit.
    Circuit,
    /// What the tool found about a gadget.
    Findings,
}

/// What a line edit changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// A program line.
    Program,
    /// A line of a document.
    Doc(usize),
}

/// How the view pane shows the shot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// One boundary's calls as a circuit over time.
    Circuit,
    /// The selected line's batches as a tree across every boundary.
    Flow,
}

/// A line being edited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Editor {
    /// The text so far.
    pub text: String,
    /// The cursor, in characters.
    pub cursor: usize,
    /// The text before the edit, to cancel it; `None` for a new line, which
    /// cancelling removes.
    pub original: Option<String>,
}

/// What one boundary looks like, worked out once per shot.
pub struct View {
    /// What is live, and when.
    pub lives: Lives,
    /// Where each call goes in the circuit.
    pub layout: Layout,
    /// Each call's label in the circuit: its mnemonic, and its bits if any.
    pub labels: Vec<String>,
}

/// A state of the inputs, for undo.
#[derive(Clone)]
struct Snapshot {
    docs: Vec<Doc>,
    program: Vec<Line>,
    outcomes: Outcomes,
    line: usize,
}

/// An analysis, with the fingerprint of what it was made from.
type Cached = (u64, Rc<Result<Analysis, String>>);

/// The whole interface.
pub struct App {
    /// The qodec's documents: the text being worked on.
    pub docs: Vec<Doc>,
    /// Where the documents are saved.
    pub source: Option<Source>,
    /// The documents as last saved, to tell whether there is anything to save.
    saved_docs: Vec<Doc>,
    /// The qodec the documents make up, or the last one they made up.
    pub qodec: Qodec,
    /// Why the documents don't make up a qodec now, if they don't.
    pub load_error: Option<String>,
    /// Why the runtime can't plan the qodec, if it can't. The program then
    /// runs on the last qodec that planned.
    pub plan_error: Option<String>,
    /// Analyses of gadgets, by layer and mnemonic, with what they depend on.
    analyses: BTreeMap<(usize, String), Cached>,
    /// The focused item.
    pub item: Item,
    /// The selected row of the map.
    pub map_row: usize,
    /// The selected line of the focused item's text.
    pub text_line: usize,
    /// The state shown in a gadget's circuit: 0 before the first call, `c + 1`
    /// after call `c`.
    pub gadget_call: usize,
    /// The selected row of a gadget's findings.
    pub finding: usize,
    /// The selected lower instruction in a gadget's parts.
    pub gadget_part: usize,
    /// What the line being edited belongs to.
    pub target: Target,
    /// The qodec, planned.
    pub engine: Engine,
    /// The program.
    pub program: Vec<Line>,
    /// Where the bottom bits come from.
    pub outcomes: Outcomes,
    /// The program's run.
    pub shot: Shot,
    /// Per boundary, its view, once needed.
    views: Vec<Option<View>>,
    next_id: u64,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The redo history the last checkpoint cleared, for an edit that is
    /// cancelled to put back.
    cleared_redo: Vec<Snapshot>,

    /// Which pane keys go to.
    pub focus: Focus,
    /// How the view pane shows the shot.
    pub mode: Mode,
    /// The selected top instruction in the parts pane.
    pub part: usize,
    /// The selected line.
    pub line: usize,
    /// The line being edited, if any.
    pub editor: Option<Editor>,
    /// The boundary the view pane shows.
    pub boundary: usize,
    /// The selected call of the shown boundary's timeline, if it has any.
    pub cursor: Option<usize>,
    /// The first circuit column and row in view.
    pub scroll: (usize, usize),
    /// The first line of the flow in view.
    pub flow_scroll: usize,
    /// Whether the help is shown.
    pub help: bool,
    /// A message for the user about the last key, if any.
    pub message: Option<String>,
    /// Where `Ctrl-S` saves the program.
    pub program_path: Option<PathBuf>,
    /// Whether to exit.
    pub quit: bool,
}

impl App {
    /// An interface on the qodec of `docs`, saved to `source`, with the
    /// program `texts`. It opens on the map. Fails if the documents don't make
    /// up a qodec, or not even its bottom layer plans.
    pub fn open(
        docs: Vec<Doc>,
        source: Option<Source>,
        texts: &[String],
        program_path: Option<PathBuf>,
    ) -> Result<Self, String> {
        let qodec = Qodec::from_bundle_str(&bundle::join(&docs))
            .map_err(|e| bundle::relocate(&docs, &e.to_string()))?;
        let (engine, plan_error) = match Engine::new(&qodec) {
            Ok(engine) => (engine, None),
            Err(e) => {
                // Plan what can be planned: the bottom layer alone.
                let n = qodec.layers().len();
                let bottom = qodec
                    .slice(n.saturating_sub(1), n)
                    .map_err(|e| e.to_string())?;
                (Engine::new(&bottom)?, Some(e))
            }
        };
        let mut app = Self::new(engine, texts, program_path);
        app.saved_docs.clone_from(&docs);
        app.docs = docs;
        app.source = source;
        app.qodec = qodec;
        app.plan_error = plan_error;
        app.focus = Focus::Map;
        app.item = Item::Qodec;
        app.text_line = 0;
        Ok(app)
    }

    /// An interface for `engine`, with the program `texts`.
    #[must_use]
    pub fn new(engine: Engine, texts: &[String], program_path: Option<PathBuf>) -> Self {
        let qodec = engine.qodec().clone();
        let docs = qodec
            .to_bundle_string()
            .ok()
            .and_then(|b| bundle::split(&b).ok())
            .map(|(_, docs)| docs)
            .unwrap_or_default();
        let program: Vec<Line> = texts
            .iter()
            .zip(0..)
            .map(|(text, id)| Line {
                id,
                text: text.clone(),
            })
            .collect();
        let mut app = Self {
            saved_docs: docs.clone(),
            docs,
            source: None,
            qodec,
            load_error: None,
            plan_error: None,
            analyses: BTreeMap::new(),
            item: Item::Program,
            map_row: 0,
            text_line: 0,
            gadget_call: 0,
            finding: 0,
            gadget_part: 0,
            target: Target::Program,
            next_id: program.len() as u64,

            shot: Shot::default(),
            views: Vec::new(),
            engine,
            program,
            outcomes: Outcomes::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            cleared_redo: Vec::new(),
            focus: Focus::Parts,
            mode: Mode::Circuit,
            part: 0,
            line: 0,
            editor: None,
            boundary: 0,
            cursor: None,
            scroll: (0, 0),
            flow_scroll: 0,
            help: false,
            message: None,
            program_path,
            quit: false,
        };
        if !app.program.is_empty() {
            app.focus = Focus::Program;
        }
        app.boundary = app.engine.boundaries().saturating_sub(1);
        app.rerun();
        app
    }

    // ── Derived state ───────────────────────────────────────────────────────

    /// Runs the program again, and keeps the selection where it was.
    pub fn rerun(&mut self) {
        let kept = self.cursor_anchor();
        self.shot = self.engine.run(&self.program, &self.outcomes);
        self.views = (0..self.engine.boundaries()).map(|_| None).collect();
        self.line = self.line.min(self.program.len().saturating_sub(1));
        match kept {
            Some((line_id, offset)) => self.restore_anchor(line_id, offset),
            None => self.follow_line(),
        }
    }

    /// The view of boundary `b`.
    pub fn view(&mut self, b: usize) -> &View {
        if self.views[b].is_none() {
            let lives = Lives::of(self.engine.model_isa(b), &self.shot, b);
            let isa = self.engine.isa(b);
            let labels: Vec<String> = self.shot.timelines[b]
                .iter()
                .map(|&e| {
                    let traced = self.shot.traced(e);
                    let mnemonic = isa
                        .instructions
                        .get(traced.call.instruction_id)
                        .map_or("?", |i| i.mnemonic.as_str());
                    match &traced.readout {
                        Some(r) => format!(
                            "{mnemonic}→{}",
                            syntax::plain(&vocab::compact(r, &traced.bits))
                        ),
                        None => mnemonic.to_string(),
                    }
                })
                .collect();
            let timeline = &self.shot.timelines[b];
            let layout = Layout::new(
                &lives.wiring,
                |e| timeline[e].node,
                |e| labels[e].chars().count(),
            );
            self.views[b] = Some(View {
                lives,
                layout,
                labels,
            });
        }
        self.views[b].as_ref().expect("the view was just made")
    }

    /// The shown boundary's timeline.
    #[must_use]
    pub fn timeline(&self) -> &[Event] {
        &self.shot.timelines[self.boundary]
    }

    /// The selected event, if any.
    #[must_use]
    pub fn event(&self) -> Option<Event> {
        self.cursor.and_then(|t| self.timeline().get(t).copied())
    }

    /// The events of line `index` at the shown boundary, as timeline indices.
    fn line_events(&self, index: usize) -> impl Iterator<Item = usize> + '_ {
        self.timeline()
            .iter()
            .enumerate()
            .filter(move |&(_, &e)| self.shot.line_of(e) == index)
            .map(|(t, _)| t)
    }

    /// The selected event as (its line's id, its position among the line's
    /// events), which survives a rerun.
    fn cursor_anchor(&self) -> Option<(u64, usize)> {
        let event = self.event()?;
        let index = self.shot.line_of(event);
        let line_id = self.program.get(index)?.id;
        let t = self.cursor?;
        let offset = self.line_events(index).position(|u| u == t)?;
        Some((line_id, offset))
    }

    fn restore_anchor(&mut self, line_id: u64, offset: usize) {
        match self.program.iter().position(|l| l.id == line_id) {
            Some(index) => {
                let events: Vec<usize> = self.line_events(index).collect();
                self.cursor = events
                    .get(offset)
                    .or(events.last())
                    .copied()
                    .or_else(|| self.last_event_before(index));
            }
            None => self.follow_line(),
        }
    }

    /// Selects the first event of the selected line at the shown boundary, or
    /// else the last one before it.
    fn follow_line(&mut self) {
        let first = self.line_events(self.line).next();
        self.cursor = first.or_else(|| self.last_event_before(self.line));
    }

    fn last_event_before(&self, index: usize) -> Option<usize> {
        self.timeline()
            .iter()
            .rposition(|&e| self.shot.line_of(e) < index)
            .or_else(|| (!self.timeline().is_empty()).then_some(0))
    }

    /// The top blocks live after the selected line, most recently given last.
    #[must_use]
    pub fn live_after_line(&mut self) -> Vec<(u64, String)> {
        let line = self.line;
        let shot_last = self.shot.timelines[0]
            .iter()
            .rposition(|&e| self.shot.line_of(e) <= line);
        let view = self.view(0);
        let mut blocks: Vec<(u64, String, Option<usize>)> = view
            .lives
            .live_after(shot_last)
            .into_iter()
            .map(|(label, (block_type, from))| (label, block_type.to_string(), from))
            .collect();
        blocks.sort_by_key(|(_, _, from)| *from);
        blocks.into_iter().map(|(l, t, _)| (l, t)).collect()
    }

    // ── Changes to the inputs ───────────────────────────────────────────────

    fn checkpoint(&mut self) {
        self.undo.push(Snapshot {
            docs: self.docs.clone(),
            program: self.program.clone(),
            outcomes: self.outcomes.clone(),
            line: self.line,
        });
        self.cleared_redo = std::mem::take(&mut self.redo);
    }

    fn restore(&mut self, snapshot: Snapshot) {
        let docs_changed = snapshot.docs != self.docs;
        self.docs = snapshot.docs;
        self.program = snapshot.program;
        self.outcomes = snapshot.outcomes;
        self.line = snapshot.line;
        if docs_changed {
            self.reload();
        } else {
            self.rerun();
        }
        self.follow_line();
    }

    fn current(&self) -> Snapshot {
        Snapshot {
            docs: self.docs.clone(),
            program: self.program.clone(),
            outcomes: self.outcomes.clone(),
            line: self.line,
        }
    }

    fn new_line(&mut self, text: String) -> Line {
        let id = self.next_id;
        self.next_id += 1;
        Line { id, text }
    }

    /// Adds a line with `text` after the selected line, and selects it.
    pub fn insert_line(&mut self, text: String) {
        self.checkpoint();
        let at = if self.program.is_empty() {
            0
        } else {
            self.line + 1
        };
        let line = self.new_line(text);
        self.program.insert(at, line);
        self.line = at;
        self.cursor = None;
        self.rerun();
        self.follow_line();
    }

    /// Adds a call of top instruction `part`, with default operands, after the
    /// selected line.
    pub fn insert_part(&mut self, part: usize) {
        let live = self.live_after_line();
        let text = vocab::default_line(self.engine.top(), part, &live);
        self.insert_line(text);
    }

    fn delete_line(&mut self) {
        if self.program.is_empty() {
            return;
        }
        self.checkpoint();
        self.program.remove(self.line);
        self.line = self.line.min(self.program.len().saturating_sub(1));
        self.cursor = None;
        self.rerun();
        self.follow_line();
    }

    fn duplicate_line(&mut self) {
        if let Some(text) = self.program.get(self.line).map(|l| l.text.clone()) {
            self.insert_line(text);
        }
    }

    fn move_line(&mut self, down: bool) {
        let to = if down {
            self.line + 1
        } else {
            match self.line.checked_sub(1) {
                Some(to) => to,
                None => return,
            }
        };
        if to >= self.program.len() {
            return;
        }
        self.checkpoint();
        self.program.swap(self.line, to);
        self.line = to;
        self.cursor = None;
        self.rerun();
    }

    /// Flips bit `i` of the selected call, which must be a bottom call, by
    /// pinning it to the other value.
    pub fn flip(&mut self, i: usize) {
        let Some(event) = self.event() else { return };
        let traced = self.shot.traced(event);
        let Some(&(id, _)) = traced.bits.get(i) else {
            self.message = Some(if traced.bits.is_empty() {
                "This call reports no bits made up at the bottom: only bottom bits can be pinned."
                    .to_string()
            } else {
                format!("This call has {}.", vocab::count(traced.bits.len(), "bit"))
            });
            return;
        };
        let value = traced
            .readout
            .as_ref()
            .and_then(|r| r.outcomes.iter().chain(&r.flags).nth(i).copied().flatten())
            .unwrap_or(false);
        self.checkpoint();
        self.outcomes.pins.insert(id, !value);
        self.rerun();
    }

    fn unpin(&mut self) {
        let Some(event) = self.event() else { return };
        let ids: Vec<_> = self
            .shot
            .traced(event)
            .bits
            .iter()
            .map(|&(id, _)| id)
            .collect();
        if ids.iter().any(|id| self.outcomes.pins.contains_key(id)) {
            self.checkpoint();
            for id in ids {
                self.outcomes.pins.remove(&id);
            }
            self.rerun();
        }
    }

    fn set_policy(&mut self, policy: Policy) {
        self.checkpoint();
        self.outcomes.policy = policy;
        self.rerun();
        self.message = Some(format!("Unpinned bits are now {policy}."));
    }

    fn save(&mut self) {
        let mut said = Vec::new();
        if self.docs != self.saved_docs {
            match &self.source {
                Some(source) => match bundle::save(&self.docs, &self.saved_docs, source) {
                    Ok(done) => {
                        self.saved_docs.clone_from(&self.docs);
                        said.push(done);
                    }
                    Err(e) => said.push(e),
                },
                None => said.push("The qodec has no file to save to.".to_string()),
            }
        }
        let Some(path) = &self.program_path else {
            if !self.program.is_empty() || said.is_empty() {
                said.push("Start with `--program <file>` to save the program to it.".to_string());
            }
            self.message = Some(said.join(" "));
            return;
        };
        let mut text: String = self
            .program
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        text.push('\n');
        said.push(match std::fs::write(path, text) {
            Ok(()) => format!("Saved the program to `{}`.", path.display()),
            Err(e) => format!("Cannot save to `{}`: {e}", path.display()),
        });
        self.message = Some(said.join(" "));
    }

    /// Whether the qodec has changes not saved yet.
    #[must_use]
    pub fn dirty(&self) -> bool {
        self.docs != self.saved_docs
    }

    // ── Keys ────────────────────────────────────────────────────────────────

    /// Does what `key` asks.
    pub fn key(&mut self, key: KeyEvent) {
        self.message = None;
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.editor.is_some() {
            self.edit_key(key);
            return;
        }
        if self.help {
            self.help = false;
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.help = true,
            KeyCode::Tab => self.cycle_focus(true),
            KeyCode::BackTab => self.cycle_focus(false),
            KeyCode::Char('v') if self.item == Item::Program => {
                self.mode = match self.mode {
                    Mode::Circuit => Mode::Flow,
                    Mode::Flow => Mode::Circuit,
                };
                self.flow_scroll = 0;
            }
            KeyCode::Char('[') if self.item == Item::Program => self.change_boundary(false),
            KeyCode::Char(']') if self.item == Item::Program => self.change_boundary(true),
            KeyCode::Esc if self.focus != Focus::Map => self.focus = Focus::Map,
            KeyCode::Char('o') => {
                let policy = match self.outcomes.policy {
                    Policy::Zero => Policy::One,
                    Policy::One => Policy::Random(1),
                    Policy::Random(_) => Policy::Zero,
                };
                self.set_policy(policy);
            }
            KeyCode::Char('r') => match self.outcomes.policy {
                Policy::Random(seed) => self.set_policy(Policy::Random(seed + 1)),
                _ => self.message = Some("Press `o` until bits are random first.".to_string()),
            },
            KeyCode::Char('X') => {
                if !self.outcomes.pins.is_empty() {
                    self.checkpoint();
                    self.outcomes.pins.clear();
                    self.rerun();
                    self.message = Some("Unpinned every bit.".to_string());
                }
            }
            KeyCode::Char('u') => match self.undo.pop() {
                Some(snapshot) => {
                    self.redo.push(self.current());
                    self.restore(snapshot);
                }
                None => self.message = Some("Nothing to undo.".to_string()),
            },
            KeyCode::Char('U') => match self.redo.pop() {
                Some(snapshot) => {
                    self.undo.push(self.current());
                    self.restore(snapshot);
                }
                None => self.message = Some("Nothing to redo.".to_string()),
            },
            KeyCode::Char('s') if ctrl => self.save(),
            _ => match self.focus {
                Focus::Map => self.map_key(key),
                Focus::Parts if self.item == Item::Program => self.parts_key(key),
                Focus::Parts => self.gadget_parts_key(key),
                Focus::Program => self.program_key(key),
                Focus::View => match self.mode {
                    Mode::Circuit => self.circuit_key(key),
                    Mode::Flow => self.flow_key(key),
                },
                Focus::Text => self.text_key(key),
                Focus::Circuit => self.gadget_circuit_key(key),
                Focus::Findings => self.findings_key(key),
            },
        }
    }

    /// The panes of the focused item, in `Tab` order.
    #[must_use]
    pub fn panes(&self) -> Vec<Focus> {
        match &self.item {
            Item::Program => vec![Focus::Map, Focus::Parts, Focus::Program, Focus::View],
            Item::Instruction(k, i) if self.has_gadget(*k, *i) => {
                vec![
                    Focus::Map,
                    Focus::Circuit,
                    Focus::Parts,
                    Focus::Text,
                    Focus::Findings,
                ]
            }
            _ => vec![Focus::Map, Focus::Text],
        }
    }

    fn cycle_focus(&mut self, forward: bool) {
        let order = self.panes();
        let k = order.iter().position(|&f| f == self.focus).unwrap_or(0);
        let k = if forward { k + 1 } else { k + order.len() - 1 };
        self.focus = order[k % order.len()];
    }

    /// Shows the next boundary down (`deeper`) or up. From a selected call,
    /// goes to the first call it caused below, or to the call that caused it
    /// above.
    fn change_boundary(&mut self, deeper: bool) {
        let n = self.engine.boundaries();
        let to = if deeper {
            self.boundary + 1
        } else {
            match self.boundary.checked_sub(1) {
                Some(to) => to,
                None => return,
            }
        };
        if to >= n {
            return;
        }
        let from = self.event();
        self.boundary = to;
        self.scroll = (0, 0);
        self.cursor = match from {
            Some(event) if deeper => self
                .timeline()
                .iter()
                .position(|&e| self.shot.nodes[e.node].parent == Some(event.node)),
            Some(event) => self.shot.nodes[event.node]
                .parent
                .and_then(|parent| self.timeline().iter().position(|e| e.node == parent)),
            None => None,
        };
        if self.cursor.is_none() {
            self.follow_line();
        }
    }

    /// Selects timeline event `t`, and the line that caused it.
    fn select_event(&mut self, t: usize) {
        if let Some(&event) = self.timeline().get(t) {
            self.cursor = Some(t);
            self.line = self.shot.line_of(event);
        }
    }

    fn select_line(&mut self, index: usize) {
        if index < self.program.len() {
            self.line = index;
            self.follow_line();
        }
    }

    fn parts_key(&mut self, key: KeyEvent) {
        let n = self.engine.top().instructions.len();
        if n == 0 {
            return;
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.part = self.part.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.part = (self.part + 1).min(n - 1),
            KeyCode::Home => self.part = 0,
            KeyCode::End => self.part = n - 1,
            KeyCode::Enter => self.insert_part(self.part),
            KeyCode::Char('e') => {
                self.insert_part(self.part);
                self.start_edit();
            }
            _ => {}
        }
    }

    fn program_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.select_line(self.line.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => self.select_line(self.line + 1),
            KeyCode::Home => self.select_line(0),
            KeyCode::End => self.select_line(self.program.len().saturating_sub(1)),
            KeyCode::Enter | KeyCode::Char('e') => {
                if self.program.is_empty() {
                    self.insert_line(String::new());
                    self.start_new_edit();
                } else {
                    self.start_edit();
                }
            }
            KeyCode::Char('n') => {
                self.insert_line(String::new());
                self.start_new_edit();
            }
            KeyCode::Char('d') | KeyCode::Delete => self.delete_line(),
            KeyCode::Char('y') => self.duplicate_line(),
            KeyCode::Char('K') => self.move_line(false),
            KeyCode::Char('J') => self.move_line(true),
            _ => {}
        }
    }

    fn circuit_key(&mut self, key: KeyEvent) {
        let n = self.timeline().len();
        if n == 0 {
            return;
        }
        let t = self.cursor.unwrap_or(0);
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => self.select_event(t.saturating_sub(1)),
            KeyCode::Right | KeyCode::Char('l') => self.select_event((t + 1).min(n - 1)),
            KeyCode::Home => self.select_event(0),
            KeyCode::End => self.select_event(n - 1),
            KeyCode::PageUp => {
                let line = self.event().map_or(0, |e| self.shot.line_of(e));
                if let Some(t) = self
                    .timeline()
                    .iter()
                    .rposition(|&e| self.shot.line_of(e) < line)
                {
                    let first = self
                        .line_events(self.shot.line_of(self.timeline()[t]))
                        .next();
                    self.select_event(first.unwrap_or(t));
                }
            }
            KeyCode::PageDown => {
                let line = self.event().map_or(0, |e| self.shot.line_of(e));
                if let Some(t) = self
                    .timeline()
                    .iter()
                    .position(|&e| self.shot.line_of(e) > line)
                {
                    self.select_event(t);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => self.scroll.1 = self.scroll.1.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll.1 += 1,
            KeyCode::Char(' ') => self.flip(0),
            KeyCode::Char(c @ '1'..='9') => {
                self.flip(c.to_digit(10).expect("a digit") as usize - 1);
            }
            KeyCode::Char('x') => self.unpin(),
            KeyCode::Enter => self.focus = Focus::Program,
            _ => {}
        }
    }

    fn flow_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.flow_scroll = self.flow_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => self.flow_scroll += 1,
            KeyCode::PageUp => self.flow_scroll = self.flow_scroll.saturating_sub(10),
            KeyCode::PageDown => self.flow_scroll += 10,
            KeyCode::Home => self.flow_scroll = 0,
            _ => {}
        }
    }

    // ── Editing a line ──────────────────────────────────────────────────────

    fn start_edit(&mut self) {
        let Some(text) = self.program.get(self.line).map(|l| l.text.clone()) else {
            return;
        };
        self.checkpoint();
        self.target = Target::Program;
        self.focus = Focus::Program;
        self.editor = Some(Editor {
            cursor: text.chars().count(),
            original: Some(text.clone()),
            text,
        });
    }

    /// Edits the line just inserted, which cancelling removes. Its insertion
    /// already made the undo checkpoint.
    fn start_new_edit(&mut self) {
        let Some(line) = self.program.get(self.line) else {
            return;
        };
        self.target = Target::Program;
        self.focus = Focus::Program;
        self.editor = Some(Editor {
            cursor: line.text.chars().count(),
            text: line.text.clone(),
            original: None,
        });
    }

    fn edit_key(&mut self, key: KeyEvent) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let byte = |text: &str, chars: usize| {
            text.char_indices()
                .nth(chars)
                .map_or(text.len(), |(b, _)| b)
        };
        match key.code {
            KeyCode::Enter => {
                self.editor = None;
                return;
            }
            KeyCode::Esc => {
                let original = editor.original.clone();
                self.editor = None;
                // The edit is undone, and so is its checkpoint, which leaves
                // the redo history as it was.
                match (self.target, original) {
                    (Target::Program, Some(text)) => self.program[self.line].text = text,
                    (Target::Program, None) => {
                        self.program.remove(self.line);
                        self.line = self.line.saturating_sub(1);
                    }
                    (Target::Doc(d), original) => {
                        let mut lines = self.docs[d].lines();
                        if let Some(text) = original {
                            lines[self.text_line] = text;
                        } else {
                            lines.remove(self.text_line);
                            self.text_line = self.text_line.saturating_sub(1);
                        }
                        self.docs[d].set_lines(&lines);
                    }
                }
                self.undo.pop();
                self.redo = std::mem::take(&mut self.cleared_redo);
                if let Target::Doc(_) = self.target {
                    self.reload();
                } else {
                    self.rerun();
                }
                self.follow_line();
                return;
            }
            KeyCode::Left => editor.cursor = editor.cursor.saturating_sub(1),
            KeyCode::Right => editor.cursor = (editor.cursor + 1).min(editor.text.chars().count()),
            KeyCode::Home => editor.cursor = 0,
            KeyCode::End => editor.cursor = editor.text.chars().count(),
            KeyCode::Backspace => {
                if editor.cursor > 0 {
                    editor.cursor -= 1;
                    let at = byte(&editor.text, editor.cursor);
                    editor.text.remove(at);
                }
            }
            KeyCode::Delete => {
                if editor.cursor < editor.text.chars().count() {
                    let at = byte(&editor.text, editor.cursor);
                    editor.text.remove(at);
                }
            }
            KeyCode::Tab if self.target == Target::Program => self.complete(),
            KeyCode::Char(c) => {
                let at = byte(&editor.text, editor.cursor);
                editor.text.insert(at, c);
                editor.cursor += 1;
            }
            _ => return,
        }
        // Every keystroke runs the shot again: the line's effect shows as it
        // is typed.
        if let Some(editor) = &self.editor {
            let text = editor.text.clone();
            match self.target {
                Target::Program => {
                    self.program[self.line].text = text;
                    self.rerun();
                    self.follow_line();
                }
                Target::Doc(d) => {
                    let mut lines = self.docs[d].lines();
                    if let Some(l) = lines.get_mut(self.text_line) {
                        *l = text;
                    }
                    self.docs[d].set_lines(&lines);
                    self.reload();
                }
            }
        }
    }

    /// Completes the word before the cursor: a mnemonic first in a statement,
    /// then the instruction's parameter names.
    fn complete(&mut self) {
        let Some(editor) = &mut self.editor else {
            return;
        };
        let before: String = editor.text.chars().take(editor.cursor).collect();
        let statement_start = before.rfind(';').map_or(0, |i| i + 1);
        let word_start = before
            .char_indices()
            .rfind(|(_, c)| c.is_whitespace())
            .map_or(0, |(i, c)| i + c.len_utf8())
            .max(statement_start);
        let word = &before[word_start..];
        let earlier: Vec<&str> = before[statement_start..word_start]
            .split_whitespace()
            .collect();
        let top = self.engine.top();
        let candidates: Vec<String> = match earlier.first() {
            None => top
                .instructions
                .iter()
                .map(|i| i.mnemonic.clone())
                .filter(|m| m.starts_with(word))
                .collect(),
            Some(mnemonic) => top
                .instructions
                .iter()
                .find(|i| i.mnemonic == *mnemonic)
                .map(|i| {
                    i.parameters
                        .iter()
                        .map(|p| format!("{}=", p.name))
                        .filter(|p| p.starts_with(word))
                        .collect()
                })
                .unwrap_or_default(),
        };
        let Some(first) = candidates.first() else {
            self.message = Some("Nothing completes this word.".to_string());
            return;
        };
        let common: String = first
            .chars()
            .enumerate()
            .take_while(|&(k, c)| candidates.iter().all(|w| w.chars().nth(k) == Some(c)))
            .map(|(_, c)| c)
            .collect();
        let rest: String = common.chars().skip(word.chars().count()).collect();
        let at = editor
            .text
            .char_indices()
            .nth(editor.cursor)
            .map_or(editor.text.len(), |(b, _)| b);
        editor.text.insert_str(at, &rest);
        editor.cursor += rest.chars().count();
        if candidates.len() == 1 && !rest.ends_with('=') {
            editor.text.insert(at + rest.len(), ' ');
            editor.cursor += 1;
        }
        if candidates.len() > 1 {
            self.message = Some(candidates.join("  "));
        }
    }

    // ── Questions the interface asks ────────────────────────────────────────

    /// The outcome of the selected line, if there is one.
    #[must_use]
    pub fn line_outcome(&self) -> Option<&Outcome> {
        self.shot.lines.get(self.line).map(|l| &l.outcome)
    }
}

// ── The workbench: the qodec's text, its items, and what the tool found ─────

impl App {
    /// Makes the qodec anew from its documents, plans it, and runs the
    /// program again. What does not load or plan is kept as a reason; what
    /// did keeps working.
    pub fn reload(&mut self) {
        match Qodec::from_bundle_str(&bundle::join(&self.docs)) {
            Ok(qodec) => {
                self.load_error = None;
                match Engine::new(&qodec) {
                    Ok(engine) => {
                        self.engine = engine;
                        self.plan_error = None;
                    }
                    Err(e) => self.plan_error = Some(e),
                }
                self.qodec = qodec;
            }
            Err(e) => self.load_error = Some(bundle::relocate(&self.docs, &e.to_string())),
        }
        self.boundary = self.boundary.min(self.engine.boundaries() - 1);
        self.part = self
            .part
            .min(self.engine.top().instructions.len().saturating_sub(1));
        self.cursor = None;
        self.rerun();
        let calls = self.gadget_calls();
        self.gadget_call = self.gadget_call.min(calls);
    }

    /// Whether instruction `i` of layer `k` has a gadget.
    #[must_use]
    pub fn has_gadget(&self, k: usize, i: usize) -> bool {
        workbench::gadget_of(&self.qodec, k, i).is_some()
    }

    /// The analysis of the gadget of instruction `i` of layer `k`, if it has
    /// one.
    pub fn analysis(&mut self, k: usize, i: usize) -> Option<Rc<Result<Analysis, String>>> {
        let (gadget, lower) = workbench::gadget_of(&self.qodec, k, i)?;
        let print = workbench::fingerprint(gadget, lower);
        let key = (k, gadget.implements.mnemonic.clone());
        if let Some((p, a)) = self.analyses.get(&key)
            && *p == print
        {
            return Some(Rc::clone(a));
        }
        let a = Rc::new(analysis::analyze(gadget, lower));
        self.analyses.insert(key, (print, Rc::clone(&a)));
        Some(a)
    }

    /// The analysis of the focused gadget, if the focused item is one.
    pub fn focused_analysis(&mut self) -> Option<Rc<Result<Analysis, String>>> {
        match self.item {
            Item::Instruction(k, i) => self.analysis(k, i),
            _ => None,
        }
    }

    fn gadget_calls(&mut self) -> usize {
        self.focused_analysis()
            .and_then(|a| a.as_ref().as_ref().ok().map(|a| a.calls.len()))
            .unwrap_or(0)
    }

    /// The map's rows.
    pub fn rows(&mut self) -> Vec<Row> {
        let layers = self.qodec.layers().len();
        let mut marks = BTreeMap::new();
        for k in 0..layers.saturating_sub(1) {
            for i in 0..self.qodec.layers()[k].instruction_set.instructions.len() {
                if let Some(a) = self.analysis(k, i) {
                    marks.insert((k, i), workbench::mark_of(&a));
                }
            }
        }
        let program = if self.plan_error.is_some()
            || self
                .shot
                .lines
                .iter()
                .any(|l| matches!(l.outcome, Outcome::Lost(_)))
        {
            Mark::Error
        } else if self
            .shot
            .lines
            .iter()
            .any(|l| matches!(l.outcome, Outcome::Rejected(_) | Outcome::Unparsed(_)))
        {
            Mark::Warn
        } else if self.shot.lines.iter().any(|l| l.outcome == Outcome::Ran) {
            Mark::Ok
        } else {
            Mark::None
        };
        workbench::rows(
            &self.qodec,
            self.load_error.is_some(),
            self.plan_error.is_some(),
            program,
            |k, i| marks.get(&(k, i)).copied(),
        )
    }

    /// The document of the focused item, and the line it starts at.
    #[must_use]
    pub fn focused_doc(&self) -> Option<(usize, usize)> {
        workbench::doc_of(&self.docs, Some(&self.qodec), &self.item)
    }

    /// Focuses `item`, keeping the map's selection where it is.
    fn show(&mut self, item: Item) {
        if self.item != item {
            self.item = item;
            self.text_line = self.focused_doc().map_or(0, |(_, l)| l);
            self.gadget_call = 0;
            self.finding = 0;
            self.gadget_part = 0;
        }
    }

    fn map_key(&mut self, key: KeyEvent) {
        let rows = self.rows();
        let n = rows.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.map_row = self.map_row.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => self.map_row = (self.map_row + 1).min(n - 1),
            KeyCode::Home => self.map_row = 0,
            KeyCode::End => self.map_row = n - 1,
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => {
                if let Some(next) = self.panes().get(1) {
                    self.focus = *next;
                }
                return;
            }
            KeyCode::Char('g') => {
                if let Item::Instruction(k, i) = rows[self.map_row.min(n - 1)].item {
                    self.new_gadget(k, i);
                }
                return;
            }
            _ => return,
        }
        let item = self.rows()[self.map_row].item.clone();
        self.show(item);
    }

    /// Selects the map row of `item`, and shows it.
    pub fn select(&mut self, item: &Item) {
        if let Some(r) = self.rows().iter().position(|r| r.item == *item) {
            self.map_row = r;
        }
        self.show(item.clone());
    }

    // ── Text ────────────────────────────────────────────────────────────────

    fn text_key(&mut self, key: KeyEvent) {
        let Some((d, _)) = self.focused_doc() else {
            return;
        };
        let lines = self.docs[d].lines();
        let n = lines.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.text_line = self.text_line.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.text_line = (self.text_line + 1).min(n.saturating_sub(1));
            }
            KeyCode::PageUp => self.text_line = self.text_line.saturating_sub(10),
            KeyCode::PageDown => self.text_line = (self.text_line + 10).min(n.saturating_sub(1)),
            KeyCode::Home => self.text_line = 0,
            KeyCode::End => self.text_line = n.saturating_sub(1),
            KeyCode::Enter | KeyCode::Char('e') => {
                let text = lines.get(self.text_line).cloned().unwrap_or_default();
                self.checkpoint();
                self.target = Target::Doc(d);
                self.editor = Some(Editor {
                    cursor: text.chars().count(),
                    original: Some(text.clone()),
                    text,
                });
            }
            KeyCode::Char('n') => {
                let current = lines.get(self.text_line).cloned().unwrap_or_default();
                let indent: String = current.chars().take_while(|c| *c == ' ').collect();
                let lead = if current.trim_start().starts_with("- ") {
                    format!("{indent}- ")
                } else {
                    indent
                };
                self.checkpoint();
                let mut lines = lines;
                let at = if n == 0 { 0 } else { self.text_line + 1 };
                lines.insert(at, lead.clone());
                self.docs[d].set_lines(&lines);
                self.text_line = at;
                self.target = Target::Doc(d);
                self.editor = Some(Editor {
                    cursor: lead.chars().count(),
                    original: None,
                    text: lead,
                });
                self.reload();
            }
            KeyCode::Char('d') | KeyCode::Delete if n > 0 => {
                self.change_lines(d, |lines, at| {
                    lines.remove(at);
                    at.min(lines.len().saturating_sub(1))
                });
            }
            KeyCode::Char('y') if n > 0 => {
                self.change_lines(d, |lines, at| {
                    lines.insert(at + 1, lines[at].clone());
                    at + 1
                });
            }
            KeyCode::Char('K') if self.text_line > 0 => {
                self.change_lines(d, |lines, at| {
                    lines.swap(at, at - 1);
                    at - 1
                });
            }
            KeyCode::Char('J') if self.text_line + 1 < n => {
                self.change_lines(d, |lines, at| {
                    lines.swap(at, at + 1);
                    at + 1
                });
            }
            _ => {}
        }
    }

    /// Changes the lines of document `d` with `change`, which gets the
    /// selected line and returns the line to select.
    fn change_lines(&mut self, d: usize, change: impl FnOnce(&mut Vec<String>, usize) -> usize) {
        self.checkpoint();
        let mut lines = self.docs[d].lines();
        self.text_line = change(&mut lines, self.text_line);
        self.docs[d].set_lines(&lines);
        self.reload();
    }

    /// Replaces the body of document `d` with `body`, as one step of history.
    fn change_doc(&mut self, d: usize, body: String, what: &str) {
        if self.docs[d].body == body {
            return;
        }
        self.checkpoint();
        self.docs[d].body = body;
        self.reload();
        self.message = Some(what.to_string());
    }

    // ── Gadgets ─────────────────────────────────────────────────────────────

    fn gadget_circuit_key(&mut self, key: KeyEvent) {
        let n = self.gadget_calls();
        match key.code {
            KeyCode::Left | KeyCode::Char('h') => {
                self.gadget_call = self.gadget_call.saturating_sub(1);
            }
            KeyCode::Right | KeyCode::Char('l') => self.gadget_call = (self.gadget_call + 1).min(n),
            KeyCode::Home => self.gadget_call = 0,
            KeyCode::End => self.gadget_call = n,
            _ => {}
        }
    }

    /// The findings of the focused gadget.
    pub fn findings(&mut self) -> Vec<workbench::Finding> {
        let Item::Instruction(k, i) = self.item else {
            return Vec::new();
        };
        let Some(a) = self.analysis(k, i) else {
            return Vec::new();
        };
        let Some((gadget, _)) = workbench::gadget_of(&self.qodec, k, i) else {
            return Vec::new();
        };
        match a.as_ref() {
            Ok(a) => workbench::findings(a, gadget),
            Err(e) => vec![workbench::Finding {
                tokens: vec![(
                    syntax::Role::Error,
                    format!("✗ The circuit can't be read: {e}"),
                )],
                ..workbench::Finding::default()
            }],
        }
    }

    fn findings_key(&mut self, key: KeyEvent) {
        let rows = self.findings();
        let n = rows.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.finding = self.finding.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.finding = (self.finding + 1).min(n.saturating_sub(1));
            }
            KeyCode::PageUp => self.finding = self.finding.saturating_sub(10),
            KeyCode::PageDown => self.finding = (self.finding + 10).min(n.saturating_sub(1)),
            KeyCode::Home => self.finding = 0,
            KeyCode::Enter | KeyCode::Char('a') => {
                match rows.get(self.finding).and_then(|r| r.adopt.clone()) {
                    Some(adopt) => self.adopt(&[adopt]),
                    None => self.message = Some("Only rows marked + can be adopted.".to_string()),
                }
            }
            KeyCode::Char('A') => {
                let all: Vec<workbench::Adopt> =
                    rows.iter().filter_map(|r| r.adopt.clone()).collect();
                if all.is_empty() {
                    self.message =
                        Some("Nothing to adopt: the tool derived nothing new.".to_string());
                } else {
                    self.adopt(&all);
                }
            }
            _ => {}
        }
        if let Some(c) = self.findings().get(self.finding).and_then(|r| r.call) {
            self.gadget_call = c + 1;
        }
    }

    /// Writes `adopt` into the focused gadget's document.
    pub fn adopt(&mut self, adopt: &[workbench::Adopt]) {
        let Some((d, _)) = self.focused_doc() else {
            return;
        };
        let mut body = self.docs[d].body.clone();
        for a in adopt {
            body = a.apply(&body);
        }
        let what = if adopt.len() == 1 {
            "Adopted it into the gadget's text.".to_string()
        } else {
            format!(
                "Adopted {} derived equations into the gadget's text.",
                adopt.len()
            )
        };
        self.change_doc(d, body, &what);
    }

    /// Gives instruction `i` of layer `k` a new, empty gadget, to grow.
    pub fn new_gadget(&mut self, k: usize, i: usize) {
        let layers = self.qodec.layers();
        if k + 1 >= layers.len() {
            self.message = Some(
                "The bottom layer has no gadgets: the backend runs its instructions.".to_string(),
            );
            return;
        }
        if self.has_gadget(k, i) {
            self.message = Some("It has a gadget already.".to_string());
            return;
        }
        let mnemonic = layers[k].instruction_set.instructions[i].mnemonic.clone();
        let (file, body) = workbench::skeleton(&self.docs, k, &mnemonic);
        self.checkpoint();
        self.docs[0].body = bundle::add_gadget(&self.docs[0].body, k, &mnemonic, &file);
        self.docs.push(Doc {
            name: file.clone(),
            lead: String::new(),
            body,
            external: matches!(self.source, Some(Source::Manifest(_))),
        });
        self.reload();
        self.item = Item::Program;
        self.select(&Item::Instruction(k, i));
        self.focus = Focus::Parts;
        self.message = Some(format!(
            "Made `{file}`: an empty circuit. Add calls from Parts, and adopt what the findings derive."
        ));
    }

    fn gadget_parts_key(&mut self, key: KeyEvent) {
        let Item::Instruction(k, _) = self.item else {
            return;
        };
        let Some(lower) = self
            .qodec
            .layers()
            .get(k + 1)
            .map(|l| l.instruction_set.clone())
        else {
            return;
        };
        let n = lower.instructions.len();
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.gadget_part = self.gadget_part.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.gadget_part = (self.gadget_part + 1).min(n.saturating_sub(1));
            }
            KeyCode::Enter if n > 0 => self.insert_call(self.gadget_part),
            _ => {}
        }
    }

    /// Adds a call of lower instruction `part` at the end of the focused
    /// gadget's circuit.
    pub fn insert_call(&mut self, part: usize) {
        let Item::Instruction(k, _) = self.item else {
            return;
        };
        let Some((d, _)) = self.focused_doc() else {
            return;
        };
        let lower = self.qodec.layers()[k + 1].instruction_set.clone();
        let a = self.focused_analysis();
        let analysis = a.as_ref().and_then(|a| a.as_ref().as_ref().ok());
        let call = workbench::default_call(&lower, part, analysis);
        let body = &self.docs[d].body;
        let nested = body
            .lines()
            .any(|l| l.trim_start().starts_with("source:") && l.starts_with(' '));
        let inline_list = body.lines().any(|l| l.starts_with("circuit:"))
            && !nested
            && body
                .lines()
                .skip_while(|l| !l.starts_with("circuit:"))
                .nth(1)
                .is_some_and(|l| l.trim_start().starts_with("- "));
        let next = if inline_list {
            bundle::append_item(body, "circuit", &call)
        } else if nested || !body.contains("circuit:") {
            bundle::append_nested(body, &["circuit", "source"], &call)
        } else {
            self.message =
                Some("This circuit is not an inline call list; edit its text instead.".to_string());
            return;
        };
        self.change_doc(d, next, &format!("Added `{call}` to the circuit."));
        self.gadget_call = self.gadget_calls();
    }
}
