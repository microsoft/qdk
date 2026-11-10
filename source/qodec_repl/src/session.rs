// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A REPL session: one qodec, which commands inspect and change, and the
//! runtime of the current shot over a recording bottom backend.

use crate::{
    bundle::{self, Doc},
    inspect,
    recorder::{AskFn, Log, Oracle, Outcomes, Recorder},
    style::{self, BIT, BLOCK, DIM, NAME, PARAM, UNRESOLVED, paint},
    syntax::{self, Ins},
};
use qodec::{InstructionSet, Qodec};
use qodec_runtime_builder::{
    Backend, BasicReadout, Bit, FaultKind, FrameDecoding, PlanError, QodecPlan, QodecRuntime,
    QodecRuntimeError, Stack, TypeChecker,
};
use std::{
    cell::RefCell,
    fmt::Write,
    mem,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const HELP: &str = r#"Type top-ISA instructions to lower them to the bottom ISA:
  <mnemonic> <block>... [<parameter>=<value>]...
Separate instructions with `;` to lower them as one batch, e.g.
  prepare_z 0; idle 0; measure 0
Blocks are ids, one per operand. Values are 0/1/false/true for bit and boolean
parameters, numbers, or text (quote it with "..." if it has spaces, e.g.
p="X_0 Z_1").

Bottom instructions that report bits are followed by `->` and the bits the
recording backend returned; top results are printed after `=>`. Bits are 0, 1,
or ? when unresolved. The shot carries over from one line to the next.

Commands:
  :isa [<layer>|top|bottom]   list a layer's instructions (default: top)
  :layers                     list the qodec's layers, top first
  :outcomes [zero|one|random [<seed>]|ask]
                              show or set the bits the bottom backend returns
  :reset                      start a new shot
  :help                       show this help
  :quit                       exit (also Ctrl-D)

Inspecting the qodec:
  :gadgets [<layer>]          list gadgets: codes in and out, calls, checks
  :gadget <mnemonic> [<layer>]
                              show a gadget: encodings, circuit, checks,
                              readouts and frames
  :codes [<layer>]            list codes by the block type they encode
  :code <name> [<layer>]      show a code: stabilizers and logical X and Z
  :yaml [<item>]              print the qodec as a YAML bundle, or one item

Changing the qodec (a change that does not plan leaves the qodec as it was):
  :edit <item>                edit an item's YAML in $VISUAL or $EDITOR
  :set <item> <path> <yaml>   set a value in an item's YAML, e.g.
                                :set h_all circuit[0] {S: [0]}
                              an index one past the end appends
  :unset <item> <path>        remove a key or list entry from an item's YAML
  :slice <first> <last>       keep only layers <first> to <last>
  :drop top|bottom            remove the top or bottom layer
  :stack above|below <file>   add another qodec's layers above or below,
                              joined at the instruction set they share
  :load <file>                load another qodec
  :reload                     load the qodec's file again
  :undo                       undo the last change
  :save <file>|<dir>/         write the qodec as one YAML bundle, or, for a
                              directory, as one file per item
  :replay [on|off]            after a change, start a new shot (off, the
                              default), or replay this shot's lines (on)
An <item> is a document name from `:yaml`, `qodec` for the manifest, a layer
(top, bottom, or its number) for its instruction set, a gadget's mnemonic, or
a code's block type or name. A <path> is keys after dots and indices in
brackets, e.g. checks[0][1], as in the YAML `:yaml <item>` prints; quote
keys with dots: frames["out[0].z[0]"].
Lines starting with `#` are comments."#;

/// The REPL commands, without their `:`.
pub const COMMANDS: &[&str] = &[
    "help", "isa", "layers", "outcomes", "reset", "quit", "gadgets", "gadget", "codes", "code",
    "yaml", "edit", "set", "unset", "slice", "drop", "stack", "load", "reload", "undo", "save",
    "replay",
];

/// The commands whose first argument is an item, or a gadget or code.
pub const ITEM_COMMANDS: &[&str] = &["yaml", "edit", "set", "unset", "gadget", "code"];

/// Edits `text`, the YAML of the document named by the first argument, and
/// returns the result.
pub type EditFn = Box<dyn FnMut(&str, &str) -> Result<String, String>>;

/// How the first line of every error comment in an edited document starts.
const EDIT_COMMENT: &str = "# qodec-repl:";

/// Other names for some of [`COMMANDS`].
pub const COMMAND_ALIASES: &[&str] = &["h", "?", "q", "exit"];

/// What the REPL does with a line.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    /// Prints this text, if any.
    Print(String),
    /// Exits.
    Quit,
}

/// One shot: batches of top-ISA calls in, their results out.
type Runtime = Box<dyn Backend<Vec<Ins>, Result<Vec<BasicReadout>, QodecRuntimeError<String>>>>;

pub struct Session {
    qodec: Qodec,
    /// The qodec's YAML, one document per artifact, or why it has none.
    docs: Result<Vec<Doc>, String>,
    /// The file `:reload` loads.
    source: Option<PathBuf>,
    /// The qodec before each change, for `:undo`.
    undo: Vec<Snapshot>,
    /// Whether a change replays the shot's lines, instead of starting a new
    /// shot.
    replay: bool,
    /// The lines lowered in this shot, for replaying.
    shot: Vec<String>,
    edit: EditFn,
    /// Every layer's ISA, top first.
    layers: Vec<Arc<InstructionSet>>,
    plan: QodecPlan,
    log: Rc<RefCell<Log>>,
    oracle: Rc<RefCell<Oracle>>,
    runtime: Runtime,
}

/// A qodec as it was before a change.
struct Snapshot {
    qodec: Qodec,
    docs: Result<Vec<Doc>, String>,
    source: Option<PathBuf>,
}

impl Session {
    /// Plans `qodec` and starts its first shot. `ask` supplies bits in
    /// `:outcomes ask` mode.
    pub fn new(qodec: &Qodec, ask: AskFn) -> Result<Self, PlanError> {
        let plan = QodecPlan::new(qodec, &FrameDecoding)?;
        let layers = isas(qodec);
        let log = Rc::default();
        let oracle = Rc::new(RefCell::new(Oracle {
            mode: Outcomes::Zero,
            ask,
        }));
        let runtime = instantiate(&plan, &layers, &log, &oracle);
        Ok(Self {
            qodec: qodec.clone(),
            docs: documents(qodec),
            source: None,
            undo: Vec::new(),
            replay: false,
            shot: Vec::new(),
            edit: Box::new(|_, _| Err("no editor is set up".to_string())),
            layers,
            plan,
            log,
            oracle,
            runtime,
        })
    }

    /// Sets the file `qodec` was loaded from, which `:reload` loads again.
    #[must_use]
    pub fn with_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.source = Some(path.into());
        self
    }

    /// Sets how `:edit` edits a document.
    #[must_use]
    pub fn with_editor(mut self, edit: EditFn) -> Self {
        self.edit = edit;
        self
    }

    pub fn top(&self) -> &Arc<InstructionSet> {
        &self.layers[0]
    }

    fn bottom(&self) -> &Arc<InstructionSet> {
        self.layers.last().expect("a planned qodec has a layer")
    }

    /// The names commands take for an item, gadget or code, to complete.
    pub fn items(&self) -> Vec<String> {
        let mut items = vec!["qodec".to_string(), "top".to_string(), "bottom".to_string()];
        if let Ok(docs) = &self.docs {
            items.extend(docs.iter().map(|d| d.name.clone()));
        }
        for layer in self.qodec.layers() {
            items.extend(layer.gadgets.keys().cloned());
            items.extend(layer.codes.keys().cloned());
            items.extend(layer.codes.values().map(|c| c.name.clone()));
        }
        items.sort();
        items.dedup();
        items
    }

    pub fn banner(&self) -> String {
        format!(
            "Loaded qodec {}\nType top-ISA instructions to lower them, or `:help`.",
            self.chain()
        )
    }

    /// `` `name`: top -> ... -> bottom ``.
    fn chain(&self) -> String {
        let chain: Vec<String> = self.layers.iter().map(|l| paint(NAME, &l.name)).collect();
        format!(
            "`{}`: {}",
            paint(NAME, self.qodec.name().unwrap_or("unnamed")),
            chain.join(&format!(" {} ", paint(DIM, "->")))
        )
    }

    /// Evaluates one line.
    pub fn eval(&mut self, line: &str) -> Reply {
        self.try_eval(line)
            .unwrap_or_else(|e| Reply::Print(format!("{} {e}", style::error_prefix())))
    }

    fn try_eval(&mut self, line: &str) -> Result<Reply, String> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            Ok(Reply::Print(String::new()))
        } else if let Some(command) = line.strip_prefix(':') {
            self.command(command)
        } else {
            self.lower(line).map(Reply::Print)
        }
    }

    fn lower(&mut self, line: &str) -> Result<String, String> {
        let (text, accepted) = self.run(line)?;
        if accepted {
            self.shot.push(line.to_string());
        }
        Ok(text)
    }

    /// Lowers `line` in this shot: what to print, and whether the shot took
    /// the line.
    fn run(&mut self, line: &str) -> Result<(String, bool), String> {
        let top = self.top();
        let instructions = syntax::statements(line)?
            .into_iter()
            .map(|s| syntax::parse_instruction(top, s))
            .collect::<Result<Vec<_>, _>>()?;
        self.log.borrow_mut().clear();
        let result = self.runtime.execute(instructions.clone());
        let mut text = self.render_log();
        match result {
            Ok(batch) => {
                text += &self.render_results(&instructions, &batch);
                Ok((text, true))
            }
            Err(e) => {
                let hint = match &e {
                    QodecRuntimeError::Rejected(_) => self.usage_hint(&instructions),
                    QodecRuntimeError::Fault(fault)
                        if fault.layer.is_none_or(|l| l == 0)
                            && fault.kind == FaultKind::InvalidInstruction =>
                    {
                        format!(
                            "; use `:reset` to start a new shot{}",
                            self.usage_hint(&instructions)
                        )
                    }
                    QodecRuntimeError::Fault(_)
                    | QodecRuntimeError::Bottom(_)
                    | QodecRuntimeError::Poisoned => {
                        "; use `:reset` to start a new shot".to_string()
                    }
                };
                Ok((format!("{text}{} {e}{hint}", style::error_prefix()), false))
            }
        }
    }

    /// Reminds the user of the operands each instruction of `instructions` takes.
    fn usage_hint(&self, instructions: &[Ins]) -> String {
        let top = self.top();
        let mut ids: Vec<usize> = Vec::new();
        for ins in instructions {
            if !ids.contains(&ins.instruction_id) {
                ids.push(ins.instruction_id);
            }
        }
        let mut text =
            "\n  input operands must be live blocks of the declared type; usage:".to_string();
        for id in ids {
            write!(text, "\n    {}", syntax::signature(&top.instructions[id]))
                .expect("writing to a string succeeds");
        }
        text
    }

    fn render_log(&self) -> String {
        let log = self.log.borrow();
        let bottom = self.bottom();
        if log.iter().all(Vec::is_empty) {
            return format!("  {}\n", paint(DIM, "(no bottom instructions)"));
        }
        let mut text = String::new();
        for (k, batch) in log.iter().enumerate() {
            if log.len() > 1 {
                let separator = format!("-- lower batch {} of {} --", k + 1, log.len());
                writeln!(text, "  {}", paint(DIM, &separator))
                    .expect("writing to a string succeeds");
            }
            let rendered: Vec<String> = batch
                .iter()
                .map(|e| syntax::render_instruction(bottom, &e.instruction))
                .collect();
            let width = rendered.iter().map(|r| style::width(r)).max().unwrap_or(0);
            for (executed, ins) in batch.iter().zip(rendered) {
                match &executed.readout {
                    Some(readout) => writeln!(
                        text,
                        "  {}  {} {}",
                        style::pad(&ins, width),
                        paint(DIM, "->"),
                        render_readout(bottom, &executed.instruction, readout)
                    ),
                    None => writeln!(text, "  {ins}"),
                }
                .expect("writing to a string succeeds");
            }
        }
        text
    }

    fn render_results(&self, instructions: &[Ins], results: &[BasicReadout]) -> String {
        let top = self.top();
        let mut results = results.iter();
        let mut text = String::new();
        for ins in instructions {
            let decl = &top.instructions[ins.instruction_id];
            if decl.observe_count() + decl.flags.len() == 0 {
                continue;
            }
            let rendered = syntax::render_instruction(top, ins);
            let readout = results.next().map_or_else(
                || "<missing result>".to_string(),
                |r| render_readout(top, ins, r),
            );
            writeln!(text, "{rendered} {} {readout}", paint(DIM, "=>"))
                .expect("writing to a string succeeds");
        }
        text
    }

    fn command(&mut self, command: &str) -> Result<Reply, String> {
        let (name, rest) = split_word(command);
        let words: Vec<&str> = rest.split_whitespace().collect();
        let text = match (name, words.as_slice()) {
            ("help" | "h" | "?", []) => HELP.to_string(),
            ("quit" | "q" | "exit", []) => return Ok(Reply::Quit),
            ("reset", []) => {
                self.reset();
                self.shot.clear();
                "Started a new shot.".to_string()
            }
            ("layers", []) => self.render_layers(),
            ("isa", []) => self.render_isa(0),
            ("isa", [which]) => self.render_isa(self.layer_index(which)?),
            ("outcomes", []) => format!("outcomes: {}", self.oracle.borrow().mode),
            ("outcomes", mode) => {
                let mode = match mode {
                    ["zero"] => Outcomes::Zero,
                    ["one"] => Outcomes::One,
                    ["random"] => Outcomes::random(time_seed()),
                    ["random", seed] => Outcomes::random(
                        seed.parse()
                            .map_err(|_| format!("`{seed}` is not a valid seed"))?,
                    ),
                    ["ask"] => Outcomes::Ask,
                    _ => return Err(usage("outcomes")),
                };
                self.oracle.borrow_mut().mode = mode;
                format!("outcomes: {}", self.oracle.borrow().mode)
            }
            ("gadgets", []) => inspect::gadgets(&self.qodec, None),
            ("gadgets", [which]) => inspect::gadgets(&self.qodec, Some(self.layer_index(which)?)),
            ("gadget", [mnemonic]) => inspect::gadget(&self.qodec, mnemonic, None)?,
            ("gadget", [mnemonic, which]) => {
                inspect::gadget(&self.qodec, mnemonic, Some(self.layer_index(which)?))?
            }
            ("codes", []) => inspect::codes(&self.qodec, None),
            ("codes", [which]) => inspect::codes(&self.qodec, Some(self.layer_index(which)?)),
            ("code", [code]) => inspect::code(&self.qodec, code, None)?,
            ("code", [code, which]) => {
                inspect::code(&self.qodec, code, Some(self.layer_index(which)?))?
            }
            ("yaml", []) => bundle::join(self.docs()?),
            ("yaml", [item]) => {
                let docs = self.docs()?;
                bundle::render(&docs[bundle::find(docs, item)?])
            }
            _ => match self.change_command(name, &words, rest)? {
                Some(text) => text,
                None if COMMANDS.contains(&name) || COMMAND_ALIASES.contains(&name) => {
                    return Err(usage(name));
                }
                None => return Err(format!("unknown command `:{command}`; see `:help`")),
            },
        };
        Ok(Reply::Print(text))
    }

    /// Runs `name`, a command that changes or saves the qodec, with arguments
    /// `words`, or `rest` as one; `None` if it is no such command.
    fn change_command(
        &mut self,
        name: &str,
        words: &[&str],
        rest: &str,
    ) -> Result<Option<String>, String> {
        let text = match (name, words) {
            ("edit", [item]) => self.edit(item)?,
            ("set", [item, path, _, ..]) => {
                let value = split_word(split_word(rest).1).1;
                self.change_document(
                    item,
                    |body| bundle::set(body, path, value),
                    &format!("Set `{path}`"),
                )?
            }
            ("unset", [item, path]) => self.change_document(
                item,
                |body| bundle::unset(body, path),
                &format!("Removed `{path}`"),
            )?,
            ("slice", [first, last]) => {
                let (first, last) = (self.layer_index(first)?, self.layer_index(last)?);
                if first > last {
                    return Err(format!(
                        "layer {first} is below layer {last}; name the top one first"
                    ));
                }
                let qodec = self
                    .qodec
                    .slice(first, last + 1)
                    .map_err(|e| e.to_string())?;
                let what = if first == last {
                    format!("Kept layer {first}")
                } else {
                    format!("Kept layers {first} to {last}")
                };
                self.change(qodec, None, None, &what)?
            }
            ("drop", [side @ ("top" | "bottom")]) => {
                let n = self.layers.len();
                if n == 1 {
                    return Err("the qodec has one layer; dropping it leaves nothing".to_string());
                }
                let (first, stop) = if *side == "top" { (1, n) } else { (0, n - 1) };
                let qodec = self.qodec.slice(first, stop).map_err(|e| e.to_string())?;
                self.change(qodec, None, None, &format!("Dropped the {side} layer"))?
            }
            ("stack", [side @ ("above" | "below"), _, ..]) => {
                let path = unquote(split_word(rest).1);
                let other = load(path)?;
                let qodec = stack(&self.qodec, &other, *side == "above")?;
                self.change(qodec, None, None, &format!("Stacked `{path}` {side}"))?
            }
            ("load", [_, ..]) => {
                let path = unquote(rest);
                let qodec = load(path)?;
                self.change(
                    qodec,
                    None,
                    Some(PathBuf::from(path)),
                    &format!("Loaded `{path}`"),
                )?
            }
            ("reload", []) => {
                let path = self
                    .source
                    .clone()
                    .ok_or("the qodec was not loaded from a file; see `:load`")?;
                let qodec = load(&path)?;
                let what = format!("Reloaded `{}`", path.display());
                self.change(qodec, None, Some(path), &what)?
            }
            ("undo", []) => self.undo()?,
            ("save", [_, ..]) => self.save(unquote(rest))?,
            ("replay", []) => self.replay_mode(),
            ("replay", [on @ ("on" | "off")]) => {
                self.replay = *on == "on";
                self.replay_mode()
            }
            _ => return Ok(None),
        };
        Ok(Some(text))
    }

    fn reset(&mut self) {
        self.runtime = instantiate(&self.plan, &self.layers, &self.log, &self.oracle);
        self.log.borrow_mut().clear();
    }

    fn docs(&self) -> Result<&Vec<Doc>, String> {
        self.docs
            .as_ref()
            .map_err(|e| format!("the qodec has no YAML to show or change: {e}"))
    }

    fn replay_mode(&self) -> String {
        if self.replay {
            "replay: on; a change replays this shot's lines".to_string()
        } else {
            "replay: off; a change starts a new shot".to_string()
        }
    }

    /// Edits `item` in the editor until its document loads and plans, or the
    /// user saves it unchanged.
    fn edit(&mut self, item: &str) -> Result<String, String> {
        let docs = self.docs()?.clone();
        let k = bundle::find(&docs, item)?;
        let name = &docs[k].name;
        let mut text = docs[k].body.clone();
        let mut error: Option<String> = None;
        loop {
            let shown = match &error {
                None => text.clone(),
                Some(e) => {
                    let mut comment = String::new();
                    for line in e
                        .lines()
                        .chain(["Fix the document and save it, or save it unchanged to give up."])
                    {
                        writeln!(comment, "{EDIT_COMMENT} {}", style::plain(line))
                            .expect("writing to a string succeeds");
                    }
                    comment + &text
                }
            };
            let edited = (self.edit)(name, &shown)?;
            let edited = edited
                .lines()
                .skip_while(|l| l.starts_with(EDIT_COMMENT))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            if edited.trim_end() == text.trim_end() {
                return Ok(match error {
                    None => "No changes.".to_string(),
                    Some(_) => "Gave up the edit; the qodec is as it was.".to_string(),
                });
            }
            text = edited;
            let mut changed = docs.clone();
            changed[k].body.clone_from(&text);
            match self.change_documents(changed, &format!("Edited `{name}`")) {
                Ok(done) => return Ok(done),
                Err(e) => error = Some(e),
            }
        }
    }

    /// Changes the document of `item` with `change`.
    fn change_document(
        &mut self,
        item: &str,
        change: impl FnOnce(&str) -> Result<String, String>,
        what: &str,
    ) -> Result<String, String> {
        let mut docs = self.docs()?.clone();
        let k = bundle::find(&docs, item)?;
        docs[k].body = change(&docs[k].body)?;
        let what = format!("{what} in `{}`", docs[k].name);
        self.change_documents(docs, &what)
    }

    /// Replaces the qodec with the one `docs` make up.
    fn change_documents(&mut self, docs: Vec<Doc>, what: &str) -> Result<String, String> {
        let qodec = Qodec::from_bundle_str(&bundle::join(&docs))
            .map_err(|e| format!("the qodec does not load: {e}; it is as it was"))?;
        self.change(qodec, Some(docs), self.source.clone(), what)
    }

    /// Replaces the qodec with `qodec`, made up of `docs` if they are known
    /// and loaded from `source`, if it is valid and plans. `what` describes the
    /// change.
    fn change(
        &mut self,
        qodec: Qodec,
        docs: Option<Vec<Doc>>,
        source: Option<PathBuf>,
        what: &str,
    ) -> Result<String, String> {
        let plan = plan(&qodec)?;
        let docs = docs.map_or_else(|| documents(&qodec), Ok);
        let before = Snapshot {
            qodec: mem::replace(&mut self.qodec, qodec),
            docs: mem::replace(&mut self.docs, docs),
            source: mem::replace(&mut self.source, source),
        };
        self.undo.push(before);
        Ok(self.install(plan, what))
    }

    fn undo(&mut self) -> Result<String, String> {
        let before = self.undo.pop().ok_or("there is no change to undo")?;
        // It planned before, so it plans again; it may predate validation.
        let plan = QodecPlan::new(&before.qodec, &FrameDecoding)
            .expect("a qodec that planned before plans again");
        self.qodec = before.qodec;
        self.docs = before.docs;
        self.source = before.source;
        Ok(self.install(plan, "Undid the last change"))
    }

    /// Starts using `plan`, the plan of the current qodec, and replays or
    /// forgets the shot.
    fn install(&mut self, plan: QodecPlan, what: &str) -> String {
        self.plan = plan;
        self.layers = isas(&self.qodec);
        self.reset();
        let lines = mem::take(&mut self.shot);
        let mut text = format!("{what}. The qodec is {}.", self.chain());
        if lines.is_empty() {
            return text;
        }
        if !self.replay {
            text += "\nStarted a new shot.";
            return text;
        }
        write!(
            text,
            "\nReplaying the shot's {} on a new shot:",
            inspect::count(lines.len(), "line")
        )
        .expect("writing to a string succeeds");
        for (k, line) in lines.iter().enumerate() {
            write!(text, "\n{}\n", paint(DIM, &format!("qodec> {line}")))
                .expect("writing to a string succeeds");
            let (out, accepted) = self
                .run(line)
                .unwrap_or_else(|e| (format!("{} {e}", style::error_prefix()), false));
            text += out.trim_end();
            if !accepted {
                write!(
                    text,
                    "\nStopped replaying at line {}; the new shot has the lines before it.",
                    k + 1
                )
                .expect("writing to a string succeeds");
                return text;
            }
            self.shot.push(line.clone());
        }
        text
    }

    /// Writes the qodec to `path`: one bundle file, or, for a directory, one
    /// file per artifact.
    fn save(&self, path: &str) -> Result<String, String> {
        let is_dir = path.ends_with(['/', '\\']) || Path::new(path).is_dir();
        if is_dir {
            let manifest = self
                .qodec
                .save(path)
                .map_err(|e| format!("cannot save to `{path}`: {e}"))?;
            Ok(format!(
                "Saved the qodec in `{path}`; its manifest is `{}`.",
                manifest.display()
            ))
        } else {
            let text = match &self.docs {
                Ok(docs) => bundle::join(docs),
                Err(_) => self.qodec.to_bundle_string().map_err(|e| e.to_string())?,
            };
            std::fs::write(path, text).map_err(|e| format!("cannot write `{path}`: {e}"))?;
            Ok(format!("Saved the qodec to `{path}`."))
        }
    }

    fn layer_index(&self, which: &str) -> Result<usize, String> {
        let last = self.layers.len() - 1;
        match which {
            "top" => Ok(0),
            "bottom" => Ok(last),
            _ => which
                .parse()
                .ok()
                .filter(|&k| k <= last)
                .ok_or_else(|| format!("expected top, bottom, or a layer from 0 to {last}")),
        }
    }

    fn render_layers(&self) -> String {
        let last = self.layers.len() - 1;
        let lines: Vec<String> = self
            .layers
            .iter()
            .enumerate()
            .map(|(k, isa)| {
                let blocks: Vec<String> =
                    isa.blocks.iter().map(|b| paint(BLOCK, &b.name)).collect();
                let role = match k {
                    0 if last == 0 => "  (top, bottom)",
                    0 => "  (top)",
                    k if k == last => "  (bottom)",
                    _ => "",
                };
                format!(
                    "layer {k}: {} [blocks: {}]{}",
                    paint(NAME, &isa.name),
                    blocks.join(", "),
                    paint(DIM, role)
                )
            })
            .collect();
        lines.join("\n")
    }

    fn render_isa(&self, k: usize) -> String {
        let isa = &self.layers[k];
        let mut text = format!("layer {k}: {}", paint(NAME, &isa.name));
        for decl in &isa.instructions {
            write!(text, "\n  {}", syntax::signature(decl)).expect("writing to a string succeeds");
        }
        text
    }
}

/// `text`'s first word, and the rest after it.
fn split_word(text: &str) -> (&str, &str) {
    let text = text.trim();
    text.split_once(char::is_whitespace)
        .map_or((text, ""), |(word, rest)| (word, rest.trim_start()))
}

/// `text` without the double quotes around it, if any.
fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|t| t.strip_suffix('"'))
        .unwrap_or(text)
}

fn usage(command: &str) -> String {
    let usage = match command {
        "help" | "h" | "?" => ":help",
        "quit" | "q" | "exit" => ":quit",
        "reset" => ":reset",
        "layers" => ":layers",
        "isa" => ":isa [<layer>|top|bottom]",
        "outcomes" => ":outcomes [zero|one|random [<seed>]|ask]",
        "gadgets" => ":gadgets [<layer>]",
        "gadget" => ":gadget <mnemonic> [<layer>]",
        "codes" => ":codes [<layer>]",
        "code" => ":code <name> [<layer>]",
        "yaml" => ":yaml [<item>]",
        "edit" => ":edit <item>",
        "set" => ":set <item> <path> <yaml>",
        "unset" => ":unset <item> <path>",
        "slice" => ":slice <first> <last>",
        "drop" => ":drop top|bottom",
        "stack" => ":stack above|below <file>",
        "load" => ":load <file>",
        "reload" => ":reload",
        "undo" => ":undo",
        "save" => ":save <file>|<dir>/",
        "replay" => ":replay [on|off]",
        _ => return format!("unknown command `:{command}`; see `:help`"),
    };
    format!("usage: {usage}")
}

fn load(path: impl AsRef<Path>) -> Result<Qodec, String> {
    let path = path.as_ref();
    Qodec::load(path).map_err(|e| format!("cannot load `{}`: {e}", path.display()))
}

/// Checks `qodec` and plans it.
fn plan(qodec: &Qodec) -> Result<QodecPlan, String> {
    qodec
        .validate()
        .map_err(|e| format!("the qodec is invalid: {e}; it is as it was"))?;
    QodecPlan::new(qodec, &FrameDecoding)
        .map_err(|e| format!("the qodec does not plan: {e}; it is as it was"))
}

/// `qodec` with the layers of `other` above it, or below it. The bottom
/// layer of the upper qodec and the top layer of the lower one must have the
/// same instruction set, and the lower one's stays.
fn stack(qodec: &Qodec, other: &Qodec, above: bool) -> Result<Qodec, String> {
    let (upper, lower) = if above {
        (other, qodec)
    } else {
        (qodec, other)
    };
    let (Some((joint, upper_layers)), Some(top)) =
        (upper.layers().split_last(), lower.layers().first())
    else {
        return Err("a qodec without layers stacks with nothing".to_string());
    };
    if joint.instruction_set != top.instruction_set {
        return Err(format!(
            "the upper qodec's bottom instruction set `{}` is not the lower one's top instruction set `{}`",
            joint.instruction_set.name, top.instruction_set.name
        ));
    }
    let layers = upper_layers.iter().chain(lower.layers()).cloned().collect();
    Ok(Qodec::new(
        qodec.name().map(String::from),
        qodec.description().map(String::from),
        layers,
    ))
}

/// The YAML documents of `qodec`'s bundle.
fn documents(qodec: &Qodec) -> Result<Vec<Doc>, String> {
    let bundle = qodec.to_bundle_string().map_err(|e| e.to_string())?;
    bundle::split(&bundle)
}

/// Every layer's ISA, top first.
fn isas(qodec: &Qodec) -> Vec<Arc<InstructionSet>> {
    qodec
        .layers()
        .iter()
        .map(|l| Arc::clone(&l.instruction_set))
        .collect()
}

fn instantiate(
    plan: &QodecPlan,
    layers: &[Arc<InstructionSet>],
    log: &Rc<RefCell<Log>>,
    oracle: &Rc<RefCell<Oracle>>,
) -> Runtime {
    let runtime: QodecRuntime<'static, String> = plan.instantiate(Recorder {
        isa: Arc::clone(layers.last().expect("a planned qodec has a layer")),
        log: Rc::clone(log),
        oracle: Rc::clone(oracle),
    });
    if plan.layer_plans().is_empty() {
        // Calls go straight to the bottom backend, which decides what a block
        // is.
        Box::new(runtime)
    } else {
        // An ill-typed line is rejected, instead of losing the shot.
        Box::new(Stack::new(TypeChecker::new(plan.top()), runtime))
    }
}

/// Renders `readout`, the result of `ins`, an instruction of `isa`.
fn render_readout(isa: &InstructionSet, ins: &Ins, readout: &BasicReadout) -> String {
    let bits = |bits: &[Bit]| -> Vec<String> { bits.iter().map(|&b| render_bit(b)).collect() };
    let mut text = format!("[{}]", bits(&readout.outcomes).join(", "));
    if !readout.flags.is_empty() {
        let names = &isa.instructions[ins.instruction_id].flags;
        let flags: Vec<String> = readout
            .flags
            .iter()
            .enumerate()
            .map(|(i, &b)| {
                let name = names.get(i).map_or("?", String::as_str);
                format!("{}={}", paint(PARAM, name), render_bit(b))
            })
            .collect();
        write!(text, " flags [{}]", flags.join(", ")).expect("writing to a string succeeds");
    }
    let status = readout.status;
    let conditions: Vec<&str> = [
        (status.rejected, "rejected"),
        (status.inconsistent, "inconsistent"),
        (status.unresolved, "unresolved"),
    ]
    .into_iter()
    .filter_map(|(set, name)| set.then_some(name))
    .collect();
    if !conditions.is_empty() {
        let conditions = format!("({})", conditions.join(", "));
        write!(text, " {}", paint(UNRESOLVED, &conditions)).expect("writing to a string succeeds");
    }
    text
}

fn render_bit(bit: Bit) -> String {
    match bit {
        Some(false) => paint(BIT, "0"),
        Some(true) => paint(BIT, "1"),
        None => paint(UNRESOLVED, "?"),
    }
}

fn time_seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| {
        u64::try_from(d.as_nanos() & u128::from(u64::MAX)).unwrap_or(1)
    })
}
