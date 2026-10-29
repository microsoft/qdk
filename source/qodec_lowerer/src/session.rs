// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! A REPL session: one qodec, and the runtime of the current shot over a
//! recording bottom backend.

use crate::{
    recorder::{AskFn, Log, Oracle, Outcomes, Recorder},
    syntax::{self, Ins},
};
use qodec::{InstructionSet, Qodec};
use qodec_runtime::{
    Backend, BasicReadout, Bit, FrameDecoder, OwnedParam, PlanError, QodecError, QodecPlan,
    QodecRuntime, QodecRuntimeError,
};
use std::{
    cell::RefCell,
    fmt::Write,
    rc::Rc,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const HELP: &str = r#"Type top-ISA instructions to lower them to the bottom ISA:
  <mnemonic> <block>... [<parameter>=<value>]...
Separate instructions with `;` to lower them as one batch, e.g.
  prepare_z 0; idle 0; measure 0
Blocks are ids, one per operand. Values are 0/1/false/true for bit and boolean
parameters, numbers, text (quote it with "..." if it has spaces, e.g.
p="X_0 Z_1"), or lists such as [0, 1].

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
Lines starting with `#` are comments."#;

/// What the REPL does with a line.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    /// Prints this text, if any.
    Print(String),
    /// Exits.
    Quit,
}

type Runtime = QodecRuntime<'static, usize, OwnedParam, BasicReadout, String>;

pub struct Session {
    name: String,
    /// Every layer's ISA, top first.
    layers: Vec<Arc<InstructionSet>>,
    plan: QodecPlan<BasicReadout>,
    log: Rc<RefCell<Log>>,
    oracle: Rc<RefCell<Oracle>>,
    runtime: Runtime,
}

impl Session {
    /// Plans `qodec` and starts its first shot. `ask` supplies bits in
    /// `:outcomes ask` mode.
    pub fn new(qodec: &Qodec, ask: AskFn) -> Result<Self, PlanError> {
        let plan = QodecPlan::new(qodec, &FrameDecoder)?;
        let layers: Vec<_> = qodec
            .layers()
            .iter()
            .map(|l| Arc::clone(&l.instruction_set))
            .collect();
        let log = Rc::default();
        let oracle = Rc::new(RefCell::new(Oracle {
            mode: Outcomes::Zero,
            ask,
        }));
        let runtime = instantiate(&plan, &layers, &log, &oracle);
        Ok(Self {
            name: qodec.name().unwrap_or("unnamed").to_string(),
            layers,
            plan,
            log,
            oracle,
            runtime,
        })
    }

    pub fn top(&self) -> &Arc<InstructionSet> {
        &self.layers[0]
    }

    fn bottom(&self) -> &Arc<InstructionSet> {
        self.layers.last().expect("a planned qodec has a layer")
    }

    pub fn banner(&self) -> String {
        let chain: Vec<&str> = self.layers.iter().map(|l| l.name.as_str()).collect();
        format!(
            "Loaded qodec `{}`: {}\nType top-ISA instructions to lower them, or `:help`.",
            self.name,
            chain.join(" -> ")
        )
    }

    /// Evaluates one line.
    pub fn eval(&mut self, line: &str) -> Reply {
        self.try_eval(line)
            .unwrap_or_else(|e| Reply::Print(format!("error: {e}")))
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
        let top = self.top();
        let instructions = syntax::statements(line)?
            .into_iter()
            .map(|s| syntax::parse_instruction(top, s))
            .collect::<Result<Vec<_>, _>>()?;
        self.log.borrow_mut().clear();
        let result = self.runtime.execute_batch(&instructions);
        let mut text = self.render_log();
        match result {
            Ok(batch) => {
                text += &self.render_results(&instructions, &batch.results);
                Ok(text)
            }
            Err(e) => {
                let hint = match e {
                    QodecRuntimeError::Layer {
                        error: QodecError::Poisoned,
                        ..
                    } => "; use `:reset` to start a new shot".to_string(),
                    QodecRuntimeError::Layer {
                        layer: 0,
                        error: QodecError::InvalidInstruction,
                    } => self.usage_hint(&instructions),
                    _ => String::new(),
                };
                Ok(format!("{text}error: {e}{hint}"))
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
            return "  (no bottom instructions)\n".to_string();
        }
        let mut text = String::new();
        for (k, batch) in log.iter().enumerate() {
            if log.len() > 1 {
                writeln!(text, "  -- lower batch {} of {} --", k + 1, log.len())
                    .expect("writing to a string succeeds");
            }
            let rendered: Vec<String> = batch
                .iter()
                .map(|e| syntax::render_instruction(bottom, &e.instruction))
                .collect();
            let width = rendered.iter().map(String::len).max().unwrap_or(0);
            for (executed, ins) in batch.iter().zip(rendered) {
                match &executed.readout {
                    Some(readout) => writeln!(
                        text,
                        "  {ins:width$}  -> {}",
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
            writeln!(text, "{rendered} => {readout}").expect("writing to a string succeeds");
        }
        text
    }

    fn command(&mut self, command: &str) -> Result<Reply, String> {
        let words: Vec<&str> = command.split_whitespace().collect();
        let text = match words.as_slice() {
            ["help" | "h" | "?"] => HELP.to_string(),
            ["quit" | "q" | "exit"] => return Ok(Reply::Quit),
            ["reset"] => {
                self.reset();
                "Started a new shot.".to_string()
            }
            ["layers"] => self.render_layers(),
            ["isa"] => self.render_isa(0),
            ["isa", which] => self.render_isa(self.layer_index(which)?),
            ["outcomes"] => format!("outcomes: {}", self.oracle.borrow().mode),
            ["outcomes", mode @ ..] => {
                let mode = match mode {
                    ["zero"] => Outcomes::Zero,
                    ["one"] => Outcomes::One,
                    ["random"] => Outcomes::random(time_seed()),
                    ["random", seed] => Outcomes::random(
                        seed.parse()
                            .map_err(|_| format!("`{seed}` is not a valid seed"))?,
                    ),
                    ["ask"] => Outcomes::Ask,
                    _ => return Err("usage: :outcomes [zero|one|random [<seed>]|ask]".into()),
                };
                self.oracle.borrow_mut().mode = mode;
                format!("outcomes: {}", self.oracle.borrow().mode)
            }
            _ => return Err(format!("unknown command `:{command}`; see `:help`")),
        };
        Ok(Reply::Print(text))
    }

    fn reset(&mut self) {
        self.runtime = instantiate(&self.plan, &self.layers, &self.log, &self.oracle);
        self.log.borrow_mut().clear();
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
                let blocks: Vec<&str> = isa.blocks.iter().map(|b| b.name.as_str()).collect();
                let role = match k {
                    0 if last == 0 => "  (top, bottom)",
                    0 => "  (top)",
                    k if k == last => "  (bottom)",
                    _ => "",
                };
                format!(
                    "layer {k}: {} [blocks: {}]{role}",
                    isa.name,
                    blocks.join(", ")
                )
            })
            .collect();
        lines.join("\n")
    }

    fn render_isa(&self, k: usize) -> String {
        let isa = &self.layers[k];
        let mut text = format!("layer {k}: {}", isa.name);
        for decl in &isa.instructions {
            write!(text, "\n  {}", syntax::signature(decl)).expect("writing to a string succeeds");
        }
        text
    }
}

fn instantiate(
    plan: &QodecPlan<BasicReadout>,
    layers: &[Arc<InstructionSet>],
    log: &Rc<RefCell<Log>>,
    oracle: &Rc<RefCell<Oracle>>,
) -> Runtime {
    plan.instantiate(Recorder {
        isa: Arc::clone(layers.last().expect("a planned qodec has a layer")),
        log: Rc::clone(log),
        oracle: Rc::clone(oracle),
    })
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
                format!("{name}={}", render_bit(b))
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
        write!(text, " ({})", conditions.join(", ")).expect("writing to a string succeeds");
    }
    text
}

fn render_bit(bit: Bit) -> String {
    match bit {
        Some(false) => "0".to_string(),
        Some(true) => "1".to_string(),
        None => "?".to_string(),
    }
}

fn time_seed() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| {
        u64::try_from(d.as_nanos() & u128::from(u64::MAX)).unwrap_or(1)
    })
}
