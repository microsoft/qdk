// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::sync::Arc;

use qsc_circuit_store::{
    BlockId, BoxBody, ControlFlow, ExprId, ForLoop, IfElse, Literal, LoopIndex, PackedOperation,
    Switch, WhileLoop,
};

use crate::{Circuit, CircuitError};

/// The construct emitted when an authoring body finishes.
#[derive(Clone, Debug)]
pub enum BodyKind {
    IfThen {
        condition: ExprId,
    },
    Else {
        condition: ExprId,
        then_block: BlockId,
    },
    While {
        condition: ExprId,
    },
    For {
        index: LoopIndex,
        values: Vec<Literal>,
    },
    Moment,
    Box {
        label: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    New,
    Entered,
    Finished,
    Cancelled,
}

#[derive(Debug)]
pub(crate) struct OpenScope {
    id: u64,
    block: Option<BlockId>,
    pub(crate) is_loop_body: bool,
}

#[derive(Debug)]
struct Session {
    owner: Arc<()>,
    id: u64,
    state: State,
}

impl Session {
    fn is_current(&self, circuit: &Circuit) -> bool {
        Arc::ptr_eq(&self.owner, &circuit.identity)
            && self.state == State::Entered
            && circuit.open_scopes.last().map(|scope| scope.id) == Some(self.id)
    }

    fn new(circuit: &mut Circuit) -> Self {
        let id = circuit.next_builder;
        circuit.next_builder += 1;
        Self {
            owner: Arc::clone(&circuit.identity),
            id,
            state: State::New,
        }
    }

    fn check_owner(&self, circuit: &Circuit) -> Result<(), CircuitError> {
        if Arc::ptr_eq(&self.owner, &circuit.identity) {
            Ok(())
        } else {
            Err(CircuitError::Invalid(
                "builder belongs to another circuit".into(),
            ))
        }
    }

    fn enter(
        &mut self,
        circuit: &mut Circuit,
        block: Option<BlockId>,
        is_loop_body: bool,
    ) -> Result<(), CircuitError> {
        self.check_owner(circuit)?;
        if self.state != State::New {
            return Err(CircuitError::Invalid("builder is not reentrant".into()));
        }
        circuit.open_scopes.push(OpenScope {
            id: self.id,
            block,
            is_loop_body,
        });
        self.state = State::Entered;
        Ok(())
    }

    fn close(&mut self, circuit: &mut Circuit, cancel: bool) -> Result<(), CircuitError> {
        self.check_owner(circuit)?;
        if self.state != State::Entered
            || circuit.open_scopes.last().map(|scope| scope.id) != Some(self.id)
        {
            return Err(CircuitError::Invalid(
                "control-flow blocks were closed out of order".into(),
            ));
        }
        circuit.open_scopes.pop();
        self.state = if cancel {
            State::Cancelled
        } else {
            State::Finished
        };
        Ok(())
    }
}

/// A single-use body token. It owns no parent pointer; its circuit must be
/// supplied explicitly for each checked transition.
#[derive(Debug)]
pub struct BodyBuilder {
    session: Session,
    block: BlockId,
    kind: BodyKind,
}

impl BodyBuilder {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        matches!(self.session.state, State::Finished | State::Cancelled)
    }

    #[must_use]
    pub fn is_current(&self, circuit: &Circuit) -> bool {
        self.session.is_current(circuit)
    }

    #[must_use]
    pub fn block(&self) -> BlockId {
        self.block
    }

    pub fn enter(&mut self, circuit: &mut Circuit) -> Result<(), CircuitError> {
        self.session.enter(
            circuit,
            Some(self.block),
            matches!(self.kind, BodyKind::While { .. } | BodyKind::For { .. }),
        )
    }

    pub fn cancel(&mut self, circuit: &mut Circuit) -> Result<(), CircuitError> {
        self.session.close(circuit, true)
    }

    pub fn finish(&mut self, circuit: &mut Circuit) -> Result<(), CircuitError> {
        self.session.close(circuit, false)?;
        let control = match &self.kind {
            BodyKind::IfThen { condition } => ControlFlow::IfElse(IfElse {
                condition: *condition,
                then_block: self.block,
                else_block: None,
            }),
            BodyKind::Else {
                condition,
                then_block,
            } => {
                let expected = circuit.current_instructions().last().is_some_and(|instruction| {
                    matches!(&instruction.operation, PackedOperation::ControlFlow(control)
                        if matches!(control.as_ref(), ControlFlow::IfElse(previous)
                            if previous.condition == *condition && previous.then_block == *then_block && previous.else_block.is_none()))
                });
                if !expected {
                    self.session.state = State::Cancelled;
                    return Err(CircuitError::Invalid(
                        "the conditional must immediately precede its alternative branch".into(),
                    ));
                }
                ControlFlow::IfElse(IfElse {
                    condition: *condition,
                    then_block: *then_block,
                    else_block: Some(self.block),
                })
            }
            BodyKind::While { condition } => ControlFlow::While(WhileLoop {
                condition: *condition,
                body: self.block,
            }),
            BodyKind::For { index, values } => ControlFlow::For(ForLoop {
                index: Some(*index),
                values: values.clone(),
                body: self.block,
            }),
            BodyKind::Moment => ControlFlow::Moment(self.block),
            BodyKind::Box { label } => ControlFlow::Box(BoxBody {
                body: self.block,
                duration: None,
                label: label.clone(),
            }),
        };
        // Validate and append before removing the old conditional, so a failed
        // alternative never loses the already completed then branch.
        if let Err(error) = circuit.append_control_flow(control) {
            self.session.state = State::Cancelled;
            return Err(error.into());
        }
        if matches!(self.kind, BodyKind::Else { .. }) {
            let replacement = circuit.pop_last().expect("just appended");
            circuit.pop_last();
            let PackedOperation::ControlFlow(control) = replacement.operation else {
                unreachable!("just appended control flow")
            };
            circuit.append_control_flow(*control)?;
        }
        Ok(())
    }

    pub fn otherwise(&self, circuit: &mut Circuit) -> Result<Self, CircuitError> {
        self.session.check_owner(circuit)?;
        let BodyKind::IfThen { condition } = self.kind else {
            return Err(CircuitError::Invalid(
                "only a conditional has an alternative branch".into(),
            ));
        };
        if self.session.state != State::Finished {
            return Err(CircuitError::Invalid(
                "the conditional body must finish before its alternative branch".into(),
            ));
        }
        Ok(circuit.body(BodyKind::Else {
            condition,
            then_block: self.block,
        }))
    }
}

/// A switch session. Case tokens remain dependent on this session until closed.
#[derive(Debug)]
pub struct SwitchBuilder {
    session: Session,
    selector: ExprId,
    cases: Vec<(Vec<Literal>, BlockId)>,
    default: Option<BlockId>,
}

#[derive(Debug)]
pub struct SwitchCaseBuilder {
    session: Session,
    switch: u64,
    block: BlockId,
    matched: Option<Vec<Literal>>,
}

impl SwitchBuilder {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        matches!(self.session.state, State::Finished | State::Cancelled)
    }

    #[must_use]
    pub fn is_current(&self, circuit: &Circuit) -> bool {
        self.session.is_current(circuit)
    }

    pub fn enter(&mut self, circuit: &mut Circuit) -> Result<(), CircuitError> {
        self.session.enter(circuit, None, false)
    }

    pub fn case(
        &self,
        circuit: &mut Circuit,
        matched: Option<Vec<Literal>>,
    ) -> Result<SwitchCaseBuilder, CircuitError> {
        self.session.check_owner(circuit)?;
        if self.session.state != State::Entered
            || circuit.open_scopes.last().map(|s| s.id) != Some(self.session.id)
        {
            return Err(CircuitError::Invalid(
                "a case requires an open switch".into(),
            ));
        }
        if matched.as_ref().is_some_and(Vec::is_empty) {
            return Err(CircuitError::Invalid(
                "a case matches at least one value".into(),
            ));
        }
        Ok(SwitchCaseBuilder {
            session: Session::new(circuit),
            switch: self.session.id,
            block: circuit.store.new_block(),
            matched,
        })
    }

    pub fn cancel(&mut self, circuit: &mut Circuit) -> Result<(), CircuitError> {
        self.session.close(circuit, true)
    }

    pub fn finish(&mut self, circuit: &mut Circuit) -> Result<(), CircuitError> {
        self.session.close(circuit, false)?;
        if self.cases.is_empty() && self.default.is_none() {
            self.session.state = State::Cancelled;
            return Err(CircuitError::Invalid(
                "a switch needs at least one case".into(),
            ));
        }
        circuit.append_control_flow(ControlFlow::Switch(Switch {
            selector: self.selector,
            cases: self.cases.clone(),
            default: self.default,
        }))?;
        Ok(())
    }
}

impl SwitchCaseBuilder {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        matches!(self.session.state, State::Finished | State::Cancelled)
    }

    #[must_use]
    pub fn is_current(&self, circuit: &Circuit) -> bool {
        self.session.is_current(circuit)
    }

    #[must_use]
    pub fn block(&self) -> BlockId {
        self.block
    }

    pub fn enter(&mut self, circuit: &mut Circuit) -> Result<(), CircuitError> {
        if circuit.open_scopes.last().map(|scope| scope.id) != Some(self.switch) {
            return Err(CircuitError::Invalid(
                "a case requires its enclosing switch to be open".into(),
            ));
        }
        self.session.enter(circuit, Some(self.block), false)
    }

    pub fn cancel(&mut self, circuit: &mut Circuit) -> Result<(), CircuitError> {
        self.session.close(circuit, true)
    }

    pub fn finish(
        &mut self,
        circuit: &mut Circuit,
        switch: &mut SwitchBuilder,
    ) -> Result<(), CircuitError> {
        switch.session.check_owner(circuit)?;
        if self.switch != switch.session.id || switch.session.state != State::Entered {
            return Err(CircuitError::Invalid(
                "case belongs to another or completed switch".into(),
            ));
        }
        self.session.close(circuit, false)?;
        match &self.matched {
            Some(values) => switch.cases.push((values.clone(), self.block)),
            None if switch.default.is_some() => {
                self.session.state = State::Cancelled;
                return Err(CircuitError::Invalid(
                    "a switch has one default case".into(),
                ));
            }
            None => switch.default = Some(self.block),
        }
        Ok(())
    }
}

impl Circuit {
    pub fn body(&mut self, kind: BodyKind) -> BodyBuilder {
        BodyBuilder {
            session: Session::new(self),
            block: self.store.new_block(),
            kind,
        }
    }

    pub fn switch(&mut self, selector: ExprId) -> SwitchBuilder {
        SwitchBuilder {
            session: Session::new(self),
            selector,
            cases: Vec::new(),
            default: None,
        }
    }

    pub(crate) fn current_block(&self) -> Option<BlockId> {
        self.open_scopes.iter().rev().find_map(|scope| scope.block)
    }

    fn current_instructions(&self) -> Vec<&qsc_circuit_store::Instruction> {
        match self.current_block() {
            Some(block) => self
                .store
                .block(block)
                .expect("open block exists")
                .instructions
                .iter()
                .collect(),
            None => self.store.instructions().collect(),
        }
    }

    fn pop_last(&mut self) -> Option<qsc_circuit_store::Instruction> {
        match self.current_block() {
            Some(block) => self.store.pop_last_instruction_from_block(block),
            None => self.store.pop_last_instruction(),
        }
    }
}
