// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The engine: a program and its outcomes in, a [`Shot`] out.
//!
//! A shot is a pure function of the program's lines and the [`Outcomes`]:
//! the engine runs every line from a fresh runtime each time, so there is no
//! state to remember or to reset, and any edit is seen at once in everything
//! it changes.

use crate::{
    syntax,
    trace::{self, Bench, Journal, Node, Outcomes, SharedJournal, Traced},
};
use qodec::{InstructionSet, Qodec};
use qodec_runtime_recipes::{
    Backend, BasicReadout, Call, FrameDecoding, Isa, QodecModel, QodecPlan, QodecRuntimeError,
    Stack, TypeChecker,
};
use std::sync::{Arc, Mutex};

/// A line of the program: a batch of top-ISA calls, as typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// Its identity, which edits elsewhere do not change.
    pub id: u64,
    /// Its text.
    pub text: String,
}

/// What became of a line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Blank or a comment: nothing to run.
    Blank,
    /// It does not parse; nothing ran.
    Unparsed(String),
    /// The runtime refused it, for example for a block that is not live;
    /// nothing ran, and the shot goes on.
    Rejected(String),
    /// It ran.
    Ran,
    /// It failed while running, which lost the shot.
    Lost(String),
    /// An earlier line lost the shot, so it did not run.
    NotRun,
}

/// A line's run.
#[derive(Clone, Debug)]
pub struct LineRun {
    /// What became of it.
    pub outcome: Outcome,
    /// Its calls, if it parses.
    pub calls: Vec<Call>,
    /// Its batch at the top boundary, if it ran.
    pub root: Option<usize>,
}

/// One call at one boundary, in time order: call `call` of batch `node`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Event {
    /// The batch.
    pub node: usize,
    /// The call's index in the batch.
    pub call: usize,
}

/// Everything a program did.
#[derive(Clone, Debug, Default)]
pub struct Shot {
    /// Each line's run, in program order.
    pub lines: Vec<LineRun>,
    /// Every batch, at every boundary, in the order they were sent.
    pub nodes: Vec<Node>,
    /// Per boundary, top first: every call, in time order.
    pub timelines: Vec<Vec<Event>>,
}

impl Shot {
    /// The traced call of `event`.
    #[must_use]
    pub fn traced(&self, event: Event) -> &trace::TracedCall {
        &self.nodes[event.node].calls[event.call]
    }

    /// The index of the line that caused `event`.
    #[must_use]
    pub fn line_of(&self, event: Event) -> usize {
        self.nodes[event.node].line
    }
}

/// The runtime of one shot, taking batches of top-ISA calls.
type ShotRuntime<'a> =
    Box<dyn Backend<Vec<Call>, Result<Vec<BasicReadout>, QodecRuntimeError<String>>> + 'a>;

/// A qodec, planned once with every layer tapped, ready to run shots.
pub struct Engine {
    qodec: Qodec,
    model: QodecModel,
    plan: QodecPlan,
    journal: SharedJournal,
}

impl Engine {
    /// Reads and plans `qodec`.
    pub fn new(qodec: &Qodec) -> Result<Self, String> {
        let model = qodec_runtime_reader::read(qodec).map_err(|e| e.to_string())?;
        let journal: SharedJournal = Arc::new(Mutex::new(Journal::default()));
        let design = Traced {
            inner: FrameDecoding,
            journal: Arc::clone(&journal),
        };
        let plan = QodecPlan::new(&model, &design).map_err(|e| e.to_string())?;
        Ok(Self {
            qodec: qodec.clone(),
            model,
            plan,
            journal,
        })
    }

    /// The qodec.
    #[must_use]
    pub fn qodec(&self) -> &Qodec {
        &self.qodec
    }

    /// The number of boundaries: one per ISA, top first.
    #[must_use]
    pub fn boundaries(&self) -> usize {
        self.qodec.layers().len()
    }

    /// The instruction set at boundary `b`.
    #[must_use]
    pub fn isa(&self, b: usize) -> &Arc<InstructionSet> {
        &self.qodec.layers()[b].instruction_set
    }

    /// The runtime's model of the instruction set at boundary `b`.
    #[must_use]
    pub fn model_isa(&self, b: usize) -> &Arc<Isa> {
        match b.checked_sub(1) {
            None => &self.model.top,
            Some(k) => &self.model.layers[k].lower,
        }
    }

    /// The top instruction set.
    #[must_use]
    pub fn top(&self) -> &Arc<InstructionSet> {
        self.isa(0)
    }

    /// Runs `program` in a new shot, with bits from `outcomes`.
    pub fn run(&self, program: &[Line], outcomes: &Outcomes) -> Shot {
        trace::lock(&self.journal).reset();
        let bench = Bench {
            isa: Arc::clone(&self.model.bottom),
            journal: Arc::clone(&self.journal),
            outcomes,
        };
        let runtime = qodec_runtime_recipes::runtime(&self.plan, bench);
        let mut runtime: ShotRuntime<'_> = if self.model.layers.is_empty() {
            // Calls go straight to the bottom backend, which has no blocks to
            // check.
            Box::new(runtime)
        } else {
            // An ill-typed line is rejected, instead of losing the shot.
            Box::new(Stack::new(TypeChecker::new(self.plan.top()), runtime))
        };

        let mut lines = Vec::with_capacity(program.len());
        let mut lost = false;
        for (index, line) in program.iter().enumerate() {
            let calls = match syntax::parse_line(self.top(), &line.text) {
                Ok(calls) => calls,
                Err(e) => {
                    lines.push(LineRun {
                        outcome: Outcome::Unparsed(e),
                        calls: Vec::new(),
                        root: None,
                    });
                    continue;
                }
            };
            let outcome = if calls.is_empty() {
                Outcome::Blank
            } else if lost {
                Outcome::NotRun
            } else {
                trace::lock(&self.journal).begin_line(index, line.id);
                match runtime.execute(calls.clone()) {
                    Ok(_) => Outcome::Ran,
                    Err(QodecRuntimeError::Rejected(fault)) => Outcome::Rejected(fault.to_string()),
                    Err(e) => {
                        lost = true;
                        Outcome::Lost(e.to_string())
                    }
                }
            };
            let journal = trace::lock(&self.journal);
            let root = journal
                .nodes()
                .iter()
                .position(|n| n.line == index && n.parent.is_none());
            lines.push(LineRun {
                outcome,
                calls,
                root,
            });
        }
        drop(runtime);

        let nodes = trace::lock(&self.journal).take();
        let mut timelines = vec![Vec::new(); self.boundaries()];
        for (id, node) in nodes.iter().enumerate() {
            if let Some(timeline) = timelines.get_mut(node.boundary) {
                timeline.extend((0..node.calls.len()).map(|call| Event { node: id, call }));
            }
        }
        Shot {
            lines,
            nodes,
            timelines,
        }
    }
}
