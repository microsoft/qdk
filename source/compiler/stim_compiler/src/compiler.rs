// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use qdk_simulators::noise_config::{
    LossPolicy, NoiseConfig, NoiseTable, PauliAndLossString, encode_pauli,
};

use qsc_ast::ast;
use qsc_ast::builder::{
    build_arg_pat, build_assignment_statement, build_attr, build_binary_expr, build_block,
    build_break_stmt, build_call_args_expr, build_call_expr, build_continue_stmt,
    build_expr_array_expr, build_if_expr_then_block, build_implicit_return_stmt, build_index_expr,
    build_lit_bool_expr, build_lit_double_expr, build_lit_int_expr, build_lit_result_expr,
    build_local_decl, build_operation_with_stmts, build_path_ident_expr, build_path_ident_ty,
    build_stmt_semi_from_expr, build_top_level_ns_with_items, build_unary_op_expr,
    build_while_stmt, managed_qubit_alloc_array,
};

use crate::parser::Pauli;
use crate::semantic;
use Pauli::{X, Y, Z};
use miette::Diagnostic;
use qsc_data_structures::span::Span;
use rustc_hash::{FxHashMap, FxHashSet};
use thiserror::Error;

type StimQubitId = u32;
type QubitId = u32;
type ResultId = u32;

/// The name of the qubit register allocated by the entry point: `use qs = Qubit[N];`.
const QUBIT_REGISTER: &str = "qs";

const READOUT_NOISE_INTRINSIC: &str = "__quantum__rt__readout_noise";
const PEEK_LOSS_INTRINSIC: &str = "__quantum__qis__peek_loss__body";

/// `within { within } apply { apply }`, whose value is the value of the `apply` block.
fn build_conjugate_expr(within: ast::Block, apply: ast::Block, span: Span) -> ast::Expr {
    ast::Expr {
        id: ast::NodeId::default(),
        span,
        kind: Box::new(ast::ExprKind::Conjugate(Box::new(within), Box::new(apply))),
    }
}

/// `within { within } apply { apply };`, or just the `apply` statements if `within` is empty.
fn build_conjugate_stmts(
    within: Vec<ast::Stmt>,
    apply: Vec<ast::Stmt>,
    span: Span,
) -> Vec<ast::Stmt> {
    if within.is_empty() {
        return apply;
    }
    let expr = build_conjugate_expr(build_block(within, span), build_block(apply, span), span);
    vec![build_stmt_semi_from_expr(expr)]
}

/// `@{attr}() operation {name}(arg0 : {param_tys[0]}, …) : {output} { body intrinsic; }`
fn build_intrinsic_decl(
    name: &str,
    attr: &str,
    param_tys: &[&str],
    output: &str,
    span: Span,
) -> ast::Item {
    let mut params: Vec<Box<ast::Pat>> = param_tys
        .iter()
        .enumerate()
        .map(|(i, ty)| {
            Box::new(build_arg_pat(
                format!("arg{i}"),
                span,
                build_path_ident_ty(ty),
            ))
        })
        .collect();
    let input_kind = if params.len() == 1 {
        ast::PatKind::Paren(params.remove(0))
    } else {
        ast::PatKind::Tuple(params.into_boxed_slice())
    };
    let input = ast::Pat {
        kind: Box::new(input_kind),
        span,
        ..Default::default()
    };
    let body = ast::CallableBody::Specs(Box::new([Box::new(ast::SpecDecl {
        id: ast::NodeId::default(),
        span,
        spec: ast::Spec::Body,
        body: ast::SpecBody::Gen(ast::SpecGen::Intrinsic),
    })]));
    let decl = ast::CallableDecl {
        id: ast::NodeId::default(),
        span,
        kind: ast::CallableKind::Operation,
        name: Box::new(ast::Ident {
            span,
            name: name.into(),
            ..Default::default()
        }),
        generics: Box::new([]),
        input: Box::new(input),
        output: Box::new(build_path_ident_ty(output)),
        functors: None,
        body: Box::new(body),
    };
    ast::Item {
        span,
        kind: Box::new(ast::ItemKind::Callable(Box::new(decl))),
        attrs: Box::new([Box::new(build_attr(attr, None::<&str>, span))]),
        ..Default::default()
    }
}

/// The Q# standard library operations emitted by the compiler.
#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QSharpOp {
    X,
    Y,
    Z,
    H,
    S,
    SAdj,
    T,
    TAdj,
    SX,
    CNOT,
    CY,
    CZ,
    SWAP,
    CCNOT,
    Rx,
    Ry,
    Rz,
    Rxx,
    Ryy,
    Rzz,
    Reset,
    M,
    MResetZ,
    IsLossResult,
}

impl QSharpOp {
    const fn name(self) -> &'static str {
        match self {
            Self::X => "X",
            Self::Y => "Y",
            Self::Z => "Z",
            Self::H => "H",
            Self::S | Self::SAdj => "S",
            Self::T | Self::TAdj => "T",
            Self::SX => "SX",
            Self::CNOT => "CNOT",
            Self::CY => "CY",
            Self::CZ => "CZ",
            Self::SWAP => "SWAP",
            Self::CCNOT => "CCNOT",
            Self::Rx => "Rx",
            Self::Ry => "Ry",
            Self::Rz => "Rz",
            Self::Rxx => "Rxx",
            Self::Ryy => "Ryy",
            Self::Rzz => "Rzz",
            Self::Reset => "Reset",
            Self::M => "M",
            Self::MResetZ => "MResetZ",
            Self::IsLossResult => "IsLossResult",
        }
    }

    fn adjoint(self) -> Self {
        match self {
            Self::S => Self::SAdj,
            Self::SAdj => Self::S,
            Self::T => Self::TAdj,
            Self::TAdj => Self::T,
            _ => unreachable!("only S and T have adjoint variants"),
        }
    }

    const fn is_adjoint(self) -> bool {
        matches!(self, Self::SAdj | Self::TAdj)
    }
}

/// Each qubit takes 3 bits in a [`PauliAndLossString`].
const MAX_CORRELATED_ERROR_QUBITS: usize = PauliAndLossString::BITS as usize / 3;

type Radians = f64;

#[derive(Clone, Debug, Error, Diagnostic)]
pub enum Error {
    #[error("unsupported instruction: {name}")]
    #[diagnostic(code("Qdk.Stim.Compiler.UnsupportedInstruction"))]
    UnsupportedInstruction {
        name: String,
        #[label]
        span: Span,
    },
    #[error("NOTLEAKED cannot reference a record produced by PEEK_LOSS")]
    #[diagnostic(code("Qdk.Stim.Compiler.NotLeakedOnPeekLoss"))]
    NotLeakedOnPeekLoss {
        #[label]
        span: Span,
    },
    #[error(
        "else_correlated_error must be preceded by a correlated_error or else_correlated_error instruction"
    )]
    #[diagnostic(code("Qdk.Stim.Compiler.OrphanedElseCorrelatedError"))]
    OrphanedElseCorrelatedError {
        #[label]
        span: Span,
    },
    #[error(
        "correlated error chain acts on {num_qubits} qubits, but at most {MAX_CORRELATED_ERROR_QUBITS} are supported"
    )]
    #[diagnostic(code("Qdk.Stim.Compiler.CorrelatedErrorTooManyQubits"))]
    CorrelatedErrorTooManyQubits {
        num_qubits: usize,
        #[label]
        span: Span,
    },
}

struct IdMap {
    qubit_map: FxHashMap<StimQubitId, QubitId>,
    name_counters: FxHashMap<&'static str, u32>, // prefix -> next index
    record_count: u32,                           // number of allocated measurement records
    peek_loss_record_ids: FxHashSet<u32>,        // record ids produced by PEEK_LOSS
}

impl IdMap {
    fn new() -> Self {
        Self {
            qubit_map: FxHashMap::default(),
            name_counters: FxHashMap::default(),
            record_count: 0,
            peek_loss_record_ids: FxHashSet::default(),
        }
    }

    fn fresh_name(&mut self, prefix: &'static str) -> String {
        let counter = self.name_counters.entry(prefix).or_insert(0);
        let id = *counter;
        *counter += 1;
        format!("{prefix}_{id}")
    }

    fn allocate_record(&mut self) -> ResultId {
        let id = self.record_count;
        self.record_count += 1;
        id
    }

    fn allocate_qubit(&mut self, stim_qubit: StimQubitId) -> QubitId {
        let next_id = self.qubit_map.len() as u32;
        *self.qubit_map.entry(stim_qubit).or_insert(next_id)
    }

    fn num_qubits(&self) -> u32 {
        self.qubit_map.len() as u32
    }
}

struct CorrelatedRow {
    faults: Vec<semantic::Fault>,
    probability: f64,
    span: Span,
}

#[derive(PartialEq, Eq, Hash)]
struct NoiseKey {
    qubits: u32,
    pauli_strings: Vec<u64>,
    probability_bits: Vec<u64>,
    on_loss: u32,
}

impl NoiseKey {
    fn from_table(table: &NoiseTable<f64>) -> Self {
        Self {
            qubits: table.qubits,
            pauli_strings: table.pauli_strings.clone(),
            probability_bits: table.probabilities.iter().map(|p| p.to_bits()).collect(),
            on_loss: table.on_loss.as_u32(),
        }
    }
}

/// Combines all faults acting on the same qubit into a single fault. Paulis are
/// multiplied (ignoring the global phase). Loss absorbs any Pauli on the same qubit.
/// Qubits whose faults cancel to the identity are omitted from the result.
fn combine_faults_per_qubit(faults: &[semantic::Fault]) -> Vec<semantic::Fault> {
    use semantic::FaultKind::{Loss, X, Y, Z};

    let mut combined = FxHashMap::<StimQubitId, Option<semantic::FaultKind>>::default();
    for fault in faults {
        let kind = combined.entry(fault.q).or_default();
        *kind = match (*kind, fault.kind) {
            (None, new) => Some(new),
            (Some(Loss), _) | (_, Loss) => Some(Loss),
            (Some(X), X) | (Some(Y), Y) | (Some(Z), Z) => None,
            (Some(X), Y) | (Some(Y), X) => Some(Z),
            (Some(Y), Z) | (Some(Z), Y) => Some(X),
            (Some(X), Z) | (Some(Z), X) => Some(Y),
        };
    }
    combined
        .into_iter()
        .filter_map(|(q, kind)| Some(semantic::Fault { kind: kind?, q }))
        .collect()
}

struct NoiseAccumulator<'noise> {
    config: &'noise mut NoiseConfig<f64, f64>,
    intrinsic_ids: FxHashMap<NoiseKey, u32>,
    current_correlated_group: Option<Vec<CorrelatedRow>>,
}

impl<'noise> NoiseAccumulator<'noise> {
    fn new(config: &'noise mut NoiseConfig<f64, f64>) -> Self {
        Self {
            config,
            intrinsic_ids: FxHashMap::default(),
            current_correlated_group: None,
        }
    }

    fn get_or_insert_intrinsic(&mut self, noise_table: NoiseTable<f64>) -> String {
        let key = NoiseKey::from_table(&noise_table);
        let Some(id) = self.intrinsic_ids.get(&key) else {
            let next_id = self.config.intrinsics.len() as u32;
            self.intrinsic_ids.insert(key, next_id);
            self.config.intrinsics.insert(next_id, noise_table);
            return format!("noise_intrinsic_{next_id}");
        };
        format!("noise_intrinsic_{id}")
    }

    fn push_correlated_row(&mut self, row: CorrelatedRow) {
        self.current_correlated_group
            .get_or_insert_with(Vec::new)
            .push(row);
    }

    fn build_noise_table(
        &self,
        num_qubits: u32,
        pauli_strings: Vec<PauliAndLossString>,
        probabilities: Vec<f64>,
    ) -> NoiseTable<f64> {
        NoiseTable {
            qubits: num_qubits,
            pauli_strings,
            probabilities,
            on_loss: LossPolicy::Skip, // required field; Skip is the default policy
        }
    }

    fn flush_correlated_group(
        &mut self,
    ) -> Result<(NoiseTable<f64>, Vec<StimQubitId>, Span), Error> {
        let rows: Vec<CorrelatedRow> = self
            .current_correlated_group
            .take()
            .expect("a correlated group must be present to flush") // this is a compiler invariant
            .into_iter()
            .map(|row| CorrelatedRow {
                faults: combine_faults_per_qubit(&row.faults),
                ..row
            })
            .collect();

        let span = Span {
            lo: rows[0].span.lo,
            hi: rows[rows.len() - 1].span.hi,
        };

        let qubits = self.collect_qubits(&rows);
        let pauli_string_width = qubits.len();
        if pauli_string_width > MAX_CORRELATED_ERROR_QUBITS {
            return Err(Error::CorrelatedErrorTooManyQubits {
                num_qubits: pauli_string_width,
                span,
            });
        }
        let column_of_qubit = self.build_column_map(&qubits);

        let mut pauli_strings = Vec::new();
        let mut probabilities = Vec::new();
        let mut outcome_indices = FxHashMap::default();
        let mut remaining_probability = 1.0;
        for row in rows {
            // Faults that cancel to the identity are a no-op outcome, but they still
            // consume probability so that later ELSE_CORRELATED_ERROR rows are not triggered.
            if row.faults.is_empty() {
                remaining_probability *= 1.0 - row.probability;
                continue;
            }
            let mut pauli_string_chars = vec!["I"; pauli_string_width];
            for fault in row.faults {
                pauli_string_chars[column_of_qubit[&fault.q]] = fault.kind.as_str();
            }
            let pauli_string = encode_pauli(&pauli_string_chars.concat());
            let index = *outcome_indices.entry(pauli_string).or_insert_with(|| {
                pauli_strings.push(pauli_string);
                probabilities.push(0.0);
                probabilities.len() - 1
            });
            // Merge duplicates so keyed noise tables cannot overwrite their probabilities.
            probabilities[index] += remaining_probability * row.probability;
            remaining_probability *= 1.0 - row.probability;
        }
        let noise_table =
            self.build_noise_table(pauli_string_width as u32, pauli_strings, probabilities);
        Ok((noise_table, qubits, span))
    }

    fn collect_qubits(&self, rows: &[CorrelatedRow]) -> Vec<StimQubitId> {
        let mut qubits: Vec<StimQubitId> = rows
            .iter()
            .flat_map(|row| row.faults.iter().map(|fault| fault.q))
            .collect();
        qubits.sort_unstable();
        qubits.dedup();
        qubits
    }

    fn build_column_map(&self, qubits: &[StimQubitId]) -> FxHashMap<StimQubitId, usize> {
        qubits
            .iter()
            .enumerate()
            .map(|(column, &qubit)| (qubit, column))
            .collect()
    }
}

struct Compiler<'noise> {
    errors: Vec<Error>,
    id_map: IdMap,
    noise_accumulator: NoiseAccumulator<'noise>,
    used_intrinsics: FxHashMap<String, ast::Item>, // name -> declaration
}

impl<'noise> Compiler<'noise> {
    fn new(noise: &'noise mut NoiseConfig<f64, f64>) -> Self {
        Self {
            errors: Vec::new(),
            id_map: IdMap::new(),
            noise_accumulator: NoiseAccumulator::new(noise),
            used_intrinsics: FxHashMap::default(),
        }
    }

    fn into_package(mut self, circuit: &semantic::Circuit) -> Result<ast::Package, Vec<Error>> {
        let pkg = self.compile_circuit(circuit);
        if self.errors.is_empty() {
            Ok(pkg)
        } else {
            Err(self.errors)
        }
    }

    fn compile_circuit(&mut self, circuit: &semantic::Circuit) -> ast::Package {
        let mut body = Vec::new();
        for item in &circuit.items {
            body.extend(self.compile_item(item));
        }
        body.extend(self.finish_correlated_noise());

        let span = circuit.span;
        let record_ids = 0..self.id_map.record_count;

        // use qs = Qubit[N];, omitted when the circuit uses no qubits.
        let num_qubits = self.id_map.num_qubits();
        let mut stmts = Vec::new();
        if num_qubits > 0 {
            stmts.push(managed_qubit_alloc_array(
                QUBIT_REGISTER,
                num_qubits,
                span,
                span,
                span,
                ast::QubitSource::Fresh,
            ));
        }

        // mutable r_k = Zero;
        stmts.extend(record_ids.clone().map(|r| Self::declare_record(r, span)));

        stmts.extend(body);

        // [r_0, …, r_{n-1}]
        let records = record_ids.map(|r| Self::record_expr(r, span)).collect();
        stmts.push(build_implicit_return_stmt(build_expr_array_expr(
            records, span,
        )));

        let result_array_ty = ast::Ty {
            kind: Box::new(ast::TyKind::Array(Box::new(build_path_ident_ty("Result")))),
            ..Default::default()
        };
        let main =
            build_operation_with_stmts("Main", vec![], result_array_ty, stmts, circuit.span, true);

        // The intrinsics are declared after `Main`, sorted so the output is deterministic.
        let mut declarations: Vec<_> = self.used_intrinsics.drain().collect();
        declarations.sort_by(|(a, _), (b, _)| a.cmp(b));
        let mut items = vec![main];
        items.extend(declarations.into_iter().map(|(_, declaration)| declaration));

        ast::Package {
            nodes: Box::new([build_top_level_ns_with_items(circuit.span, "Stim", items)]),
            ..Default::default()
        }
    }

    fn compile_item(&mut self, item: &semantic::Item) -> Vec<ast::Stmt> {
        match item {
            semantic::Item::Block(block) => self.compile_block(block),
            semantic::Item::Instruction(instruction) => self.compile_instruction(instruction),
        }
    }

    fn compile_block(&mut self, block: &semantic::Block) -> Vec<ast::Stmt> {
        let mut stmts = Vec::new();

        // Block boundaries interrupt E/ELSE_CORRELATED_ERROR chains.
        if let Some(stmt) = self.finish_correlated_noise() {
            stmts.push(stmt);
        }
        match block {
            semantic::Block::RepeatBlock { count, body } => {
                stmts.extend(self.compile_repeat_block(*count, body))
            }
            semantic::Block::SelectBlock { body } => {
                stmts.extend(self.compile_select_block(body, Span::default()))
            }
        }
        stmts
    }

    /// Unroll a repeat block into repeated statements
    fn compile_repeat_block(&mut self, count: u32, body: &[semantic::Item]) -> Vec<ast::Stmt> {
        let error_count_before_repeat = self.errors.len();
        let mut stmts = Vec::new();
        for _ in 0..count {
            for item in body {
                stmts.extend(self.compile_item(item));
            }
            // Block boundaries interrupt E/ELSE_CORRELATED_ERROR chains.
            if let Some(stmt) = self.finish_correlated_noise() {
                stmts.push(stmt);
            }
            // Avoid repeating error reporting
            if self.errors.len() > error_count_before_repeat {
                break;
            }
        }
        stmts
    }

    /// Compile a select block into a while true loop with a break at the end
    fn compile_select_block(&mut self, body: &[semantic::Item], span: Span) -> Vec<ast::Stmt> {
        let mut body_stmts = Vec::new();
        for item in body {
            body_stmts.extend(self.compile_item(item));
        }

        // Block boundaries interrupt E/ELSE_CORRELATED_ERROR chains.
        body_stmts.extend(self.finish_correlated_noise());

        // Without a REQUIRE or NOTLEAKED the block never restarts, so it needs no loop.
        if !Self::select_block_can_restart(body) {
            return body_stmts;
        }

        body_stmts.push(build_break_stmt(span));
        vec![build_while_stmt(
            build_lit_bool_expr(true, span),
            build_block(body_stmts, span),
            span,
        )]
    }

    fn select_block_can_restart(body: &[semantic::Item]) -> bool {
        body.iter().any(|item| match item {
            semantic::Item::Instruction(instruction) => matches!(
                instruction.kind,
                semantic::InstructionKind::Require { .. }
                    | semantic::InstructionKind::NotLeaked { .. }
            ),
            semantic::Item::Block(semantic::Block::RepeatBlock { count, body }) => {
                *count > 0 && Self::select_block_can_restart(body)
            }
            semantic::Item::Block(semantic::Block::SelectBlock { .. }) => false,
        })
    }

    fn compile_instruction(&mut self, instruction: &semantic::Instruction) -> Vec<ast::Stmt> {
        use QSharpOp as Op;
        let mut stmts = Vec::new();
        let continues_correlated_error = matches!(
            &instruction.kind,
            semantic::InstructionKind::Noise(semantic::Noise::CorrelatedError {
                kind: semantic::CorrelatedErrorKind::Else,
                ..
            })
        );
        if self.noise_accumulator.current_correlated_group.is_some()
            && !continues_correlated_error
            && let Some(stmt) = self.finish_correlated_noise()
        {
            stmts.push(stmt);
        }

        let span = instruction.span;

        let instruction_stmts = match &instruction.kind {
            semantic::InstructionKind::Reset { q, basis } => self.compile_reset(*q, *basis, span),
            semantic::InstructionKind::SingleQubitGate { q, gate } => {
                self.compile_single_qubit_gate(*q, *gate, span)
            }
            semantic::InstructionKind::TwoQubitGate { q0, q1, gate } => {
                self.compile_two_qubit_gate(*q0, *q1, *gate, span)
            }
            semantic::InstructionKind::ThreeQubitGate { q0, q1, q2, gate } => {
                self.compile_three_qubit_gate(*q0, *q1, *q2, *gate, span)
            }
            semantic::InstructionKind::PauliProductGate { product, gate } => {
                self.compile_pauli_product_gate(product, *gate, span)
            }
            semantic::InstructionKind::ClassicallyControlledPauli {
                control,
                target,
                pauli,
            } => self.compile_classical_control(*control, *target, *pauli, span),
            semantic::InstructionKind::Noise(noise) => self.compile_noise(instruction.span, noise),
            semantic::InstructionKind::SingleQubitMeasurement {
                reset,
                observable,
                readout_noise,
                negated,
                q,
            } => self.compile_single_qubit_measurement(
                *reset,
                *observable,
                *readout_noise,
                *negated,
                *q,
                span,
            ),
            semantic::InstructionKind::TwoQubitMeasurement {
                readout_noise,
                observable,
                negated,
                q0,
                q1,
            } => self.compile_two_qubit_measurement(
                *readout_noise,
                *observable,
                *negated,
                *q0,
                *q1,
                span,
            ),
            semantic::InstructionKind::PauliProductMeasurement {
                readout_noise,
                product,
            } => self.compile_pauli_product_measurement(*readout_noise, product, span),
            semantic::InstructionKind::PeekLoss { readout_noise, q } => {
                self.compile_peek_loss(*readout_noise, *q, span)
            }
            semantic::InstructionKind::Require { records } => self.compile_require(records, span),
            semantic::InstructionKind::NotLeaked { records } => {
                self.compile_not_leaked(records, span)
            }
            semantic::InstructionKind::Annotation(semantic::Annotation::MeasurementPadding {
                ..
            }) => {
                self.unsupported("MPAD", instruction.span);
                vec![]
            }
            semantic::InstructionKind::Annotation(_) => vec![],
            semantic::InstructionKind::SingleQubitRotation { axis, angle, q } => {
                let op = match axis {
                    X => Op::Rx,
                    Y => Op::Ry,
                    Z => Op::Rz,
                };
                vec![self.rotation_call(op, *angle, &[*q], span)]
            }
            semantic::InstructionKind::TwoQubitRotation {
                axis,
                angle,
                q0,
                q1,
            } => {
                let op = match axis {
                    semantic::PauliPair::XX => Op::Rxx,
                    semantic::PauliPair::YY => Op::Ryy,
                    semantic::PauliPair::ZZ => Op::Rzz,
                };
                vec![self.rotation_call(op, *angle, &[*q0, *q1], span)]
            }
            semantic::InstructionKind::U3 {
                theta,
                phi,
                lambda,
                q,
            } => {
                vec![
                    self.rotation_call(Op::Rz, *lambda, &[*q], span),
                    self.rotation_call(Op::Ry, *theta, &[*q], span),
                    self.rotation_call(Op::Rz, *phi, &[*q], span),
                ]
            }
            semantic::InstructionKind::PauliProductRotation { angle, product } => {
                self.compile_pauli_product_rotation(*angle, product, span)
            }
        };
        stmts.extend(instruction_stmts);
        stmts
    }

    fn compile_reset(&mut self, qubit: StimQubitId, basis: Pauli, span: Span) -> Vec<ast::Stmt> {
        // Stim decomposition (into H, S, CX, M, R): R; then H for Z -> X; then S for X -> Y.
        use QSharpOp as Op;
        let ops: &[QSharpOp] = match basis {
            Z => &[Op::Reset],
            X => &[Op::Reset, Op::H],
            Y => &[Op::Reset, Op::H, Op::S],
        };
        ops.iter()
            .map(|&op| self.gate_call(op, &[qubit], span))
            .collect()
    }

    fn compile_single_qubit_gate(
        &mut self,
        qubit: StimQubitId,
        gate: semantic::SingleQubitGateKind,
        span: Span,
    ) -> Vec<ast::Stmt> {
        use QSharpOp as Op;
        use semantic::SingleQubitGateKind as G;

        // Decompositions are from Stim's (into H, S, CX, M, R), simplified using SX, X, Y, Z.
        let ops: &[QSharpOp] = match gate {
            G::I => &[],
            G::X => &[Op::X],
            G::Y => &[Op::Y],
            G::Z => &[Op::Z],
            G::H => &[Op::H],
            G::S => &[Op::S],
            G::S_DAG => &[Op::SAdj],
            G::T => &[Op::T],
            G::T_DAG => &[Op::TAdj],
            G::SQRT_X => &[Op::SX],
            G::SQRT_X_DAG => &[Op::S, Op::H, Op::S], // S 0; H 0; S 0
            G::SQRT_Y => &[Op::Z, Op::H],            // S 0; S 0; H 0
            G::SQRT_Y_DAG => &[Op::H, Op::Z],        // H 0; S 0; S 0
            G::C_NXYZ => &[Op::SAdj, Op::H, Op::Z],  // S 0; S 0; S 0; H 0; S 0; S 0
            G::C_NZYX => &[Op::Z, Op::H, Op::SAdj],  // S 0; S 0; H 0; S 0; S 0; S 0
            G::C_XNYZ => &[Op::S, Op::H],            // S 0; H 0
            G::C_XYNZ => &[Op::S, Op::H, Op::Z],     // S 0; H 0; S 0; S 0
            G::C_XYZ => &[Op::SAdj, Op::H],          // S 0; S 0; S 0; H 0
            G::C_ZNYX => &[Op::H, Op::SAdj],         // H 0; S 0; S 0; S 0
            G::C_ZYNX => &[Op::Z, Op::H, Op::S],     // S 0; S 0; H 0; S 0
            G::C_ZYX => &[Op::H, Op::S],             // H 0; S 0
            G::H_NXY => &[Op::S, Op::X],             // S 0; H 0; S 0; S 0; H 0
            G::H_NXZ => &[Op::Z, Op::H, Op::Z],      // S 0; S 0; H 0; S 0; S 0
            G::H_NYZ => &[Op::Z, Op::SX],            // S 0; S 0; H 0; S 0; H 0
            G::H_XY => &[Op::X, Op::S],              // H 0; S 0; S 0; H 0; S 0
            G::H_YZ => &[Op::SX, Op::Z],             // H 0; S 0; H 0; S 0; S 0
        };
        ops.iter()
            .map(|&op| self.gate_call(op, &[qubit], span))
            .collect()
    }

    fn compile_two_qubit_gate(
        &mut self,
        q0: StimQubitId,
        q1: StimQubitId,
        gate: semantic::TwoQubitGateKind,
        span: Span,
    ) -> Vec<ast::Stmt> {
        use QSharpOp as Op;
        use semantic::TwoQubitGateKind as G;

        // Decompositions are Stim's (into H, S, CX, M, R), with `S S S` written as `SAdj`.
        let steps: &[(QSharpOp, &[StimQubitId])] = match gate {
            G::II => &[],
            G::CX => &[(Op::CNOT, &[q0, q1])],
            G::CY => &[(Op::CY, &[q0, q1])],
            G::CZ => &[(Op::CZ, &[q0, q1])],
            G::SWAP => &[(Op::SWAP, &[q0, q1])],
            G::XCZ => &[(Op::CNOT, &[q1, q0])],
            G::CXSWAP => &[(Op::CNOT, &[q1, q0]), (Op::CNOT, &[q0, q1])],
            G::SWAPCX => &[(Op::CNOT, &[q0, q1]), (Op::CNOT, &[q1, q0])],
            G::CZSWAP => &[
                (Op::H, &[q0]),
                (Op::CNOT, &[q0, q1]),
                (Op::CNOT, &[q1, q0]),
                (Op::H, &[q1]),
            ],
            G::ISWAP => &[
                (Op::H, &[q0]),
                (Op::CNOT, &[q0, q1]),
                (Op::CNOT, &[q1, q0]),
                (Op::H, &[q1]),
                (Op::S, &[q1]),
                (Op::S, &[q0]),
            ],
            G::ISWAP_DAG => &[
                (Op::SAdj, &[q0]),
                (Op::SAdj, &[q1]),
                (Op::H, &[q1]),
                (Op::CNOT, &[q1, q0]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q0]),
            ],
            G::SQRT_XX => &[
                (Op::H, &[q0]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q1]),
                (Op::S, &[q0]),
                (Op::S, &[q1]),
                (Op::H, &[q0]),
                (Op::H, &[q1]),
            ],
            G::SQRT_XX_DAG => &[
                (Op::H, &[q0]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q1]),
                (Op::SAdj, &[q0]),
                (Op::SAdj, &[q1]),
                (Op::H, &[q0]),
                (Op::H, &[q1]),
            ],
            G::SQRT_YY => &[
                (Op::SAdj, &[q0]),
                (Op::SAdj, &[q1]),
                (Op::H, &[q0]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q1]),
                (Op::S, &[q0]),
                (Op::S, &[q1]),
                (Op::H, &[q0]),
                (Op::H, &[q1]),
                (Op::S, &[q0]),
                (Op::S, &[q1]),
            ],
            G::SQRT_YY_DAG => &[
                (Op::SAdj, &[q0]),
                (Op::S, &[q1]),
                (Op::H, &[q0]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q1]),
                (Op::S, &[q0]),
                (Op::S, &[q1]),
                (Op::H, &[q0]),
                (Op::H, &[q1]),
                (Op::S, &[q0]),
                (Op::SAdj, &[q1]),
            ],
            G::SQRT_ZZ => &[
                (Op::H, &[q1]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q1]),
                (Op::S, &[q0]),
                (Op::S, &[q1]),
            ],
            G::SQRT_ZZ_DAG => &[
                (Op::H, &[q1]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q1]),
                (Op::SAdj, &[q0]),
                (Op::SAdj, &[q1]),
            ],
            G::XCX => &[(Op::H, &[q0]), (Op::CNOT, &[q0, q1]), (Op::H, &[q0])],
            G::XCY => &[
                (Op::H, &[q0]),
                (Op::SAdj, &[q1]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q0]),
                (Op::S, &[q1]),
            ],
            G::YCX => &[
                (Op::SAdj, &[q0]),
                (Op::H, &[q1]),
                (Op::CNOT, &[q1, q0]),
                (Op::S, &[q0]),
                (Op::H, &[q1]),
            ],
            G::YCY => &[
                (Op::SAdj, &[q0]),
                (Op::SAdj, &[q1]),
                (Op::H, &[q0]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q0]),
                (Op::S, &[q0]),
                (Op::S, &[q1]),
            ],
            G::YCZ => &[(Op::SAdj, &[q0]), (Op::CNOT, &[q1, q0]), (Op::S, &[q0])],
            G::CH => return vec![self.controlled_gate_call(Op::H, &[q0], q1, span)],
        };
        steps
            .iter()
            .map(|&(op, qubits)| self.gate_call(op, qubits, span))
            .collect()
    }

    fn compile_three_qubit_gate(
        &mut self,
        q0: StimQubitId,
        q1: StimQubitId,
        q2: StimQubitId,
        gate: semantic::ThreeQubitGateKind,
        span: Span,
    ) -> Vec<ast::Stmt> {
        use QSharpOp as Op;
        use semantic::ThreeQubitGateKind as G;

        let steps: &[(QSharpOp, &[StimQubitId])] = match gate {
            G::CCX => &[(Op::CCNOT, &[q0, q1, q2])],
            // Clifft decomposition: H 2; CCX 0 1 2; H 2
            G::CCZ => &[(Op::H, &[q2]), (Op::CCNOT, &[q0, q1, q2]), (Op::H, &[q2])],
        };
        steps
            .iter()
            .map(|&(op, qubits)| self.gate_call(op, qubits, span))
            .collect()
    }

    fn compile_pauli_product_gate(
        &mut self,
        product: &semantic::PauliProduct,
        gate: semantic::PauliProductGateKind,
        span: Span,
    ) -> Vec<ast::Stmt> {
        use QSharpOp as Op;
        use semantic::PauliProductGateKind as G;

        if product.factors.is_empty() {
            return vec![];
        }
        let op = match gate {
            G::S => Op::S,
            G::S_DAG => Op::SAdj,
            G::T => Op::T,
            G::T_DAG => Op::TAdj,
        };

        let op = if product.negated { op.adjoint() } else { op };
        self.conjugate_gate(product, op, span)
    }

    fn compile_pauli_product_rotation(
        &mut self,
        angle: Radians,
        product: &semantic::PauliProduct,
        span: Span,
    ) -> Vec<ast::Stmt> {
        if product.factors.is_empty() {
            return vec![];
        }
        let angle = if product.negated { -angle } else { angle };
        self.conjugate_rotation(product, angle, span)
    }

    fn compile_classical_control(
        &mut self,
        control: semantic::MeasurementRecord,
        target: StimQubitId,
        pauli: Pauli,
        span: Span,
    ) -> Vec<ast::Stmt> {
        let Some(result_id) = self.resolve_record(control) else {
            return vec![];
        };

        vec![self.apply_pauli_conditionally(pauli, result_id, target, span)]
    }

    fn compile_noise(&mut self, instruction_span: Span, noise: &semantic::Noise) -> Vec<ast::Stmt> {
        match noise {
            semantic::Noise::CorrelatedError {
                kind,
                probability,
                faults,
            } => match kind {
                // The chain's noise call is emitted when the chain is flushed.
                semantic::CorrelatedErrorKind::Initial => {
                    self.accumulate_correlated_noise(instruction_span, *probability, faults);
                    vec![]
                }
                semantic::CorrelatedErrorKind::Else => {
                    self.continue_correlated_noise(instruction_span, *probability, faults);
                    vec![]
                }
            },
            semantic::Noise::Depolarize2 {
                probability,
                q0,
                q1,
            } => {
                let table = self.noise_accumulator.build_noise_table(
                    2,
                    [
                        "IX", "IY", "IZ", "XI", "XX", "XY", "XZ", "YI", "YX", "YY", "YZ", "ZI",
                        "ZX", "ZY", "ZZ",
                    ]
                    .map(encode_pauli)
                    .to_vec(),
                    vec![*probability / 15.0; 15],
                );
                vec![self.noise_table_call(table, &[*q0, *q1], instruction_span)]
            }
            semantic::Noise::HeraldedErase { .. } => {
                self.unsupported("HERALDED_ERASE", instruction_span);
                vec![]
            }
            semantic::Noise::HeraldedPauliChannel1 { .. } => {
                self.unsupported("HERALDED_PAULI_CHANNEL_1", instruction_span);
                vec![]
            }
            semantic::Noise::PauliChannel1 { probabilities, q } => {
                let table = self.noise_accumulator.build_noise_table(
                    1,
                    ["X", "Y", "Z"].map(encode_pauli).to_vec(),
                    probabilities.to_vec(),
                );
                vec![self.noise_table_call(table, &[*q], instruction_span)]
            }
            semantic::Noise::PauliChannel2 {
                probabilities,
                q0,
                q1,
            } => {
                let table = self.noise_accumulator.build_noise_table(
                    2,
                    [
                        "IX", "IY", "IZ", "XI", "XX", "XY", "XZ", "YI", "YX", "YY", "YZ", "ZI",
                        "ZX", "ZY", "ZZ",
                    ]
                    .map(encode_pauli)
                    .to_vec(),
                    probabilities.to_vec(),
                );
                vec![self.noise_table_call(table, &[*q0, *q1], instruction_span)]
            }
            semantic::Noise::SingleQubitNoise {
                kind,
                probability,
                q,
            } => match kind {
                semantic::SingleQubitNoiseKind::Depolarize => {
                    let table = self.noise_accumulator.build_noise_table(
                        1,
                        ["X", "Y", "Z"].map(encode_pauli).to_vec(),
                        vec![*probability / 3.0; 3],
                    );
                    vec![self.noise_table_call(table, &[*q], instruction_span)]
                }
                semantic::SingleQubitNoiseKind::Fault(kind) => {
                    let table = self.noise_accumulator.build_noise_table(
                        1,
                        vec![encode_pauli(kind.as_str())],
                        vec![*probability],
                    );
                    vec![self.noise_table_call(table, &[*q], instruction_span)]
                }
            },
        }
    }

    fn compile_single_qubit_measurement(
        &mut self,
        reset: bool,
        observable: Pauli,
        readout_noise: f64,
        negated: bool,
        qubit: StimQubitId,
        span: Span,
    ) -> Vec<ast::Stmt> {
        let op = if reset {
            QSharpOp::MResetZ
        } else {
            QSharpOp::M
        };

        let mut stmts = self.rotate_to_z_basis(observable, qubit, span);
        let (result_id, measurement) = self.measure(op, qubit, negated, span);
        stmts.extend(measurement);
        stmts.extend(self.rotate_from_z_basis(observable, qubit, span));
        if readout_noise > 0.0 {
            stmts.push(self.readout_noise_call(readout_noise, result_id, span));
        }
        stmts
    }

    fn compile_two_qubit_measurement(
        &mut self,
        readout_noise: f64,
        observable: semantic::PauliPair,
        negated: bool,
        q0: StimQubitId,
        q1: StimQubitId,
        span: Span,
    ) -> Vec<ast::Stmt> {
        use QSharpOp as Op;
        use semantic::PauliPair as P;

        // Stim's decompositions (into H, S, CX, M, R), with `S S` written as `Z`.
        let steps: &[(QSharpOp, &[StimQubitId])] = match observable {
            // CX 0 1; H 0; M 0; H 0; CX 0 1
            P::XX => &[
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q0]),
                (Op::M, &[q0]),
                (Op::H, &[q0]),
                (Op::CNOT, &[q0, q1]),
            ],
            // S 0; S 1; CX 0 1; H 0; M 0; S 1; S 1; H 0; CX 0 1; S 0; S 1
            P::YY => &[
                (Op::S, &[q0]),
                (Op::S, &[q1]),
                (Op::CNOT, &[q0, q1]),
                (Op::H, &[q0]),
                (Op::M, &[q0]),
                (Op::Z, &[q1]),
                (Op::H, &[q0]),
                (Op::CNOT, &[q0, q1]),
                (Op::S, &[q0]),
                (Op::S, &[q1]),
            ],
            // CX 0 1; M 1; CX 0 1
            P::ZZ => &[(Op::CNOT, &[q0, q1]), (Op::M, &[q1]), (Op::CNOT, &[q0, q1])],
        };

        let mut stmts = Vec::new();
        for &(op, qubits) in steps {
            if op == Op::M {
                let (result_id, measurement) = self.measure(op, qubits[0], negated, span);
                stmts.extend(measurement);
                if readout_noise > 0.0 {
                    stmts.push(self.readout_noise_call(readout_noise, result_id, span));
                }
            } else {
                stmts.push(self.gate_call(op, qubits, span));
            }
        }
        stmts
    }

    fn compile_pauli_product_measurement(
        &mut self,
        readout_noise: f64,
        product: &semantic::PauliProduct,
        span: Span,
    ) -> Vec<ast::Stmt> {
        if product.factors.is_empty() {
            return vec![];
        }

        self.conjugate_measure(product, readout_noise, span)
    }

    fn compile_peek_loss(
        &mut self,
        readout_noise: f64,
        qubit: StimQubitId,
        span: Span,
    ) -> Vec<ast::Stmt> {
        let mut stmts = Vec::new();
        let (result_id, peek_loss_call) = self.peek_loss(qubit, span);
        stmts.push(peek_loss_call);
        if readout_noise > 0.0 {
            stmts.push(self.readout_noise_call(readout_noise, result_id, span));
        }
        stmts
    }

    fn compile_require(
        &mut self,
        records: &[semantic::NegatableMeasurementRecord],
        span: Span,
    ) -> Vec<ast::Stmt> {
        let Some(result_ids) = records
            .iter()
            .map(|negatable_record| self.resolve_record(negatable_record.record))
            .collect::<Option<Vec<_>>>()
        else {
            return vec![];
        };

        // loss = IsLossResult(r_a) or IsLossResult(r_b) or …
        let losses: Vec<ast::Expr> = result_ids
            .iter()
            .map(|&r| Self::is_loss_expr(r, span))
            .collect();
        let loss_expr = Self::fold(ast::BinOp::OrL, losses, span);

        // parity = (r_a == One) != (r_b == One) != …, where `!rec[..]` compares with `Zero`
        let record_bits: Vec<ast::Expr> = result_ids
            .iter()
            .zip(records)
            .map(|(&r, record)| Self::record_bit_expr(r, record.negated, span))
            .collect();
        let parity_expr = Self::fold(ast::BinOp::Neq, record_bits, span);

        let (loss, loss_stmt) = self.bind_local("loss", loss_expr, span);
        let (parity, parity_stmt) = self.bind_local("parity", parity_expr, span);
        let restart_expr = build_binary_expr(
            false,
            ast::BinOp::OrL,
            build_path_ident_expr(&loss, span, span),
            build_path_ident_expr(&parity, span, span),
            span,
        );
        let (restart, restart_stmt) = self.bind_local("restart", restart_expr, span);

        vec![
            loss_stmt,
            parity_stmt,
            restart_stmt,
            Self::continue_if(&restart, span),
        ]
    }

    fn compile_not_leaked(
        &mut self,
        records: &[semantic::MeasurementRecord],
        span: Span,
    ) -> Vec<ast::Stmt> {
        let Some(result_ids) = records
            .iter()
            .map(|record| self.resolve_record(*record))
            .collect::<Option<Vec<_>>>()
        else {
            return vec![];
        };

        let mut has_error = false;
        for (&result_id, record) in result_ids.iter().zip(records) {
            if self.id_map.peek_loss_record_ids.contains(&result_id) {
                self.push_error(Error::NotLeakedOnPeekLoss { span: record.span });
                has_error = true;
            }
        }
        if has_error {
            return vec![];
        }

        // loss = IsLossResult(r_a) or IsLossResult(r_b) or …
        let losses: Vec<ast::Expr> = result_ids
            .iter()
            .map(|&r| Self::is_loss_expr(r, span))
            .collect();
        let loss_expr = Self::fold(ast::BinOp::OrL, losses, span);

        let (loss, loss_stmt) = self.bind_local("loss", loss_expr, span);
        vec![loss_stmt, Self::continue_if(&loss, span)]
    }

    fn accumulate_correlated_noise(
        &mut self,
        instruction_span: Span,
        probability: f64,
        faults: &[semantic::Fault],
    ) {
        self.noise_accumulator.push_correlated_row(CorrelatedRow {
            probability,
            faults: faults.to_vec(),
            span: instruction_span,
        });
    }

    fn continue_correlated_noise(
        &mut self,
        instruction_span: Span,
        probability: f64,
        faults: &[semantic::Fault],
    ) {
        if self.noise_accumulator.current_correlated_group.is_none() {
            self.push_error(Error::OrphanedElseCorrelatedError {
                span: instruction_span,
            });
            return;
        }
        self.accumulate_correlated_noise(instruction_span, probability, faults);
    }

    fn finish_correlated_noise(&mut self) -> Option<ast::Stmt> {
        self.noise_accumulator.current_correlated_group.as_ref()?;
        let (noise_table, qubits, span) = match self.noise_accumulator.flush_correlated_group() {
            Ok(flushed) => flushed,
            Err(error) => {
                self.push_error(error);
                return None;
            }
        };
        if qubits.is_empty() {
            // Every outcome cancelled to the identity, so there is no noise to apply.
            return None;
        }
        Some(self.noise_table_call(noise_table, &qubits, span))
    }

    fn resolve_record(&self, record: semantic::MeasurementRecord) -> Option<ResultId> {
        self.id_map.record_count.checked_sub(record.offset)
    }

    fn unsupported(&mut self, instruction_name: &str, instruction_span: Span) {
        if self.errors.iter().any(|error| {
            matches!(
                error,
                Error::UnsupportedInstruction {
                    name: existing_name,
                    span,
                } if existing_name == instruction_name && *span == instruction_span
            )
        }) {
            return;
        }

        self.push_error(Error::UnsupportedInstruction {
            name: instruction_name.to_string(),
            span: instruction_span,
        });
    }

    /// `set r_k = op(q);` for `M` or `MResetZ`. A negated measurement flips the qubit with `X`
    /// before measuring, and flips it back afterwards unless the measurement reset it.
    fn measure(
        &mut self,
        op: QSharpOp,
        qubit: StimQubitId,
        negated: bool,
        span: Span,
    ) -> (ResultId, Vec<ast::Stmt>) {
        let mut stmts = Vec::new();
        if negated {
            stmts.push(self.gate_call(QSharpOp::X, &[qubit], span));
        }
        let measure_call = self.call_expr(op, &[qubit], span);
        let (result_id, record_assignment) = self.record_result(measure_call, span);
        stmts.push(record_assignment);
        if negated && op != QSharpOp::MResetZ {
            stmts.push(self.gate_call(QSharpOp::X, &[qubit], span));
        }
        (result_id, stmts)
    }

    fn peek_loss(&mut self, qubit: StimQubitId, span: Span) -> (ResultId, ast::Stmt) {
        let args = self.qubit_args(&[qubit], span);
        let declaration = build_intrinsic_decl(
            PEEK_LOSS_INTRINSIC,
            "Measurement",
            &["Qubit"],
            "Result",
            span,
        );
        self.declare_intrinsic(PEEK_LOSS_INTRINSIC, declaration);
        let peek_loss_call = self.intrinsic_expr(PEEK_LOSS_INTRINSIC, args, span);
        let (result_id, assignment) = self.record_result(peek_loss_call, span);
        self.id_map.peek_loss_record_ids.insert(result_id);
        (result_id, assignment)
    }

    /// `within { <product to Z on its first qubit> } apply { op(q_0); }`
    fn conjugate_gate(
        &mut self,
        product: &semantic::PauliProduct,
        op: QSharpOp,
        span: Span,
    ) -> Vec<ast::Stmt> {
        let within = self.map_pauli_product_to_z(&product.factors, span);
        let apply = vec![self.gate_call(op, &[product.factors[0].q], span)];
        build_conjugate_stmts(within, apply, span)
    }

    /// `within { <product to Z on its first qubit> } apply { Rz(angle, q_0); }`
    fn conjugate_rotation(
        &mut self,
        product: &semantic::PauliProduct,
        angle: Radians,
        span: Span,
    ) -> Vec<ast::Stmt> {
        let within = self.map_pauli_product_to_z(&product.factors, span);
        let apply = vec![self.rotation_call(QSharpOp::Rz, angle, &[product.factors[0].q], span)];
        build_conjugate_stmts(within, apply, span)
    }

    /// `within` { <product to Z on its first qubit> } apply { r_k = M(q_0); }
    /// optionally adds readout noise to the measurement result
    fn conjugate_measure(
        &mut self,
        product: &semantic::PauliProduct,
        readout_noise: f64,
        span: Span,
    ) -> Vec<ast::Stmt> {
        let within = self.map_pauli_product_to_z(&product.factors, span);
        let (result_id, mut measure_stmts) =
            self.measure(QSharpOp::M, product.factors[0].q, product.negated, span);
        if readout_noise > 0.0 {
            measure_stmts.push(self.readout_noise_call(readout_noise, result_id, span));
        }
        build_conjugate_stmts(within, measure_stmts, span)
    }

    /// Rotates each factor to the Z basis, then folds their parity onto the first qubit with
    /// CNOTs, so that the product acts as Z on the first qubit.
    fn map_pauli_product_to_z(
        &mut self,
        factors: &[semantic::PauliFactor],
        span: Span,
    ) -> Vec<ast::Stmt> {
        let focus = factors[0].q;
        let mut stmts: Vec<ast::Stmt> = factors
            .iter()
            .flat_map(|factor| self.rotate_to_z_basis(factor.pauli, factor.q, span))
            .collect();
        stmts.extend(
            factors[1..]
                .iter()
                .map(|factor| self.gate_call(QSharpOp::CNOT, &[factor.q, focus], span)),
        );
        stmts
    }

    fn rotate_to_z_basis(
        &mut self,
        pauli: Pauli,
        qubit: StimQubitId,
        span: Span,
    ) -> Vec<ast::Stmt> {
        use QSharpOp as Op;
        let ops: &[QSharpOp] = match pauli {
            X => &[Op::H],
            Y => &[Op::SAdj, Op::H],
            Z => &[],
        };
        ops.iter()
            .map(|&op| self.gate_call(op, &[qubit], span))
            .collect()
    }

    fn rotate_from_z_basis(
        &mut self,
        pauli: Pauli,
        qubit: StimQubitId,
        span: Span,
    ) -> Vec<ast::Stmt> {
        use QSharpOp as Op;
        let ops: &[QSharpOp] = match pauli {
            X => &[Op::H],
            Y => &[Op::H, Op::S],
            Z => &[],
        };
        ops.iter()
            .map(|&op| self.gate_call(op, &[qubit], span))
            .collect()
    }

    /// `{op}(qs[a], qs[b], …);`
    fn gate_call(&mut self, op: QSharpOp, qubits: &[StimQubitId], span: Span) -> ast::Stmt {
        let args = self.qubit_args(qubits, span);
        Self::call_stmt(op, false, args, span)
    }

    /// `Controlled {op}([controls…], target);`
    fn controlled_gate_call(
        &mut self,
        op: QSharpOp,
        controls: &[StimQubitId],
        target: StimQubitId,
        span: Span,
    ) -> ast::Stmt {
        let controls = build_expr_array_expr(self.qubit_args(controls, span), span);
        let target = Self::qubit_expr(self.id_map.allocate_qubit(target), span);
        Self::call_stmt(op, true, vec![controls, target], span)
    }

    /// `if r_k == One { {pauli}(qs[a]); }`
    fn apply_pauli_conditionally(
        &mut self,
        pauli: Pauli,
        result_id: ResultId,
        target: StimQubitId,
        span: Span,
    ) -> ast::Stmt {
        let op = match pauli {
            X => QSharpOp::X,
            Y => QSharpOp::Y,
            Z => QSharpOp::Z,
        };
        let condition = build_binary_expr(
            false,
            ast::BinOp::Eq,
            Self::record_expr(result_id, span),
            build_lit_result_expr(ast::Result::One, span),
            span,
        );
        let then_block = build_block(vec![self.gate_call(op, &[target], span)], span);
        build_stmt_semi_from_expr(build_if_expr_then_block(condition, then_block, span))
    }

    /// `{op}(angle, qs[a], …);`
    fn rotation_call(
        &mut self,
        op: QSharpOp,
        angle: Radians,
        qubits: &[StimQubitId],
        span: Span,
    ) -> ast::Stmt {
        let mut args = vec![build_lit_double_expr(angle, span)];
        args.extend(self.qubit_args(qubits, span));
        Self::call_stmt(op, false, args, span)
    }

    /// `{op}(qs[a], qs[b], …)` as an expression, e.g. `M(qs[0])`.
    fn call_expr(&mut self, op: QSharpOp, qubits: &[StimQubitId], span: Span) -> ast::Expr {
        let args = self.qubit_args(qubits, span);
        Self::call(op, false, args, span)
    }

    /// `[Adjoint | Controlled] {op}(args…);`
    fn call_stmt(op: QSharpOp, controlled: bool, args: Vec<ast::Expr>, span: Span) -> ast::Stmt {
        build_stmt_semi_from_expr(Self::call(op, controlled, args, span))
    }

    /// `[Adjoint | Controlled] {op}(args…)`
    fn call(op: QSharpOp, controlled: bool, args: Vec<ast::Expr>, span: Span) -> ast::Expr {
        let mut callee = build_path_ident_expr(op.name(), span, span);
        if op.is_adjoint() {
            callee = build_unary_op_expr(ast::UnOp::Functor(ast::Functor::Adj), callee, span);
        }
        if controlled {
            callee = build_unary_op_expr(ast::UnOp::Functor(ast::Functor::Ctl), callee, span);
        }
        let args = build_call_args_expr(args);
        build_call_expr(callee, args, span)
    }

    /// `let {prefix}_n = value;` with a fresh name. Returns the name, to refer to the binding
    /// later, and the statement.
    fn bind_local(
        &mut self,
        prefix: &'static str,
        value: ast::Expr,
        span: Span,
    ) -> (String, ast::Stmt) {
        let name = self.id_map.fresh_name(prefix);
        let stmt = build_local_decl(&name, ast::Mutability::Immutable, value, span, span);
        (name, stmt)
    }

    /// `r_k = value;` for a newly allocated record `r_k`.
    fn record_result(&mut self, value: ast::Expr, span: Span) -> (ResultId, ast::Stmt) {
        let result_id = self.id_map.allocate_record();
        let stmt = build_assignment_statement(Self::record_expr(result_id, span), value, span);
        (result_id, stmt)
    }

    /// `{name}(args…);` for a QDK intrinsic, e.g. a noise intrinsic.
    fn intrinsic_stmt(&mut self, name: &str, args: Vec<ast::Expr>, span: Span) -> ast::Stmt {
        build_stmt_semi_from_expr(self.intrinsic_expr(name, args, span))
    }

    /// `{name}(args…)` for a QDK intrinsic, e.g. `__quantum__qis__peek_loss__body(qs[0])`.
    fn intrinsic_expr(&mut self, name: &str, args: Vec<ast::Expr>, span: Span) -> ast::Expr {
        let callee = build_path_ident_expr(name, span, span);
        let args = build_call_args_expr(args);
        build_call_expr(callee, args, span)
    }

    /// Records the declaration of the intrinsic `name`, unless one is already recorded.
    fn declare_intrinsic(&mut self, name: &str, declaration: ast::Item) {
        self.used_intrinsics
            .entry(name.to_string())
            .or_insert(declaration);
    }

    /// `__quantum__rt__readout_noise(p, p, r_k);`
    fn readout_noise_call(
        &mut self,
        probability: f64,
        result_id: ResultId,
        span: Span,
    ) -> ast::Stmt {
        let args = vec![
            build_lit_double_expr(probability, span),
            build_lit_double_expr(probability, span),
            Self::record_expr(result_id, span),
        ];
        let declaration = build_intrinsic_decl(
            READOUT_NOISE_INTRINSIC,
            "NoiseIntrinsic",
            &["Double", "Double", "Result"],
            "Unit",
            span,
        );
        self.declare_intrinsic(READOUT_NOISE_INTRINSIC, declaration);
        self.intrinsic_stmt(READOUT_NOISE_INTRINSIC, args, span)
    }

    /// `r_k` #TODO change to r[k] when mutable arrays are supported
    fn record_name(result_id: ResultId) -> String {
        format!("r_{result_id}")
    }

    /// `r_k` as an expression.
    fn record_expr(result_id: ResultId, span: Span) -> ast::Expr {
        build_path_ident_expr(Self::record_name(result_id), span, span)
    }

    /// `mutable r_k = Zero;`
    fn declare_record(result_id: ResultId, span: Span) -> ast::Stmt {
        let zero = build_lit_result_expr(ast::Result::Zero, span);
        build_local_decl(
            Self::record_name(result_id),
            ast::Mutability::Mutable,
            zero,
            span,
            span,
        )
    }

    /// `IsLossResult(r_k)`
    fn is_loss_expr(result_id: ResultId, span: Span) -> ast::Expr {
        Self::call(
            QSharpOp::IsLossResult,
            false,
            vec![Self::record_expr(result_id, span)],
            span,
        )
    }

    /// `r_k == One`, or `r_k == Zero` for a negated record.
    fn record_bit_expr(result_id: ResultId, negated: bool, span: Span) -> ast::Expr {
        let expected = if negated {
            ast::Result::Zero
        } else {
            ast::Result::One
        };
        build_binary_expr(
            false,
            ast::BinOp::Eq,
            Self::record_expr(result_id, span),
            build_lit_result_expr(expected, span),
            span,
        )
    }

    /// Combines the expressions left to right with `op`: `a op b op c …`. `op` must be `or` or
    /// `!=`, whose identity is `false`, so an empty list folds to `false`.
    fn fold(op: ast::BinOp, exprs: Vec<ast::Expr>, span: Span) -> ast::Expr {
        let mut exprs = exprs.into_iter();
        let first = exprs
            .next()
            .unwrap_or_else(|| build_lit_bool_expr(false, span));
        exprs.fold(first, |acc, expr| {
            build_binary_expr(false, op, acc, expr, span)
        })
    }

    /// `if {name} { continue; }`
    fn continue_if(name: &str, span: Span) -> ast::Stmt {
        let then_block = build_block(vec![build_continue_stmt(span)], span);
        build_stmt_semi_from_expr(build_if_expr_then_block(
            build_path_ident_expr(name, span, span),
            then_block,
            span,
        ))
    }

    /// Allocates each Stim qubit and returns `qs[k]` for it.
    fn qubit_args(&mut self, qubits: &[StimQubitId], span: Span) -> Vec<ast::Expr> {
        qubits
            .iter()
            .map(|&q| Self::qubit_expr(self.id_map.allocate_qubit(q), span))
            .collect()
    }

    /// `qs[k]`
    fn qubit_expr(qubit: QubitId, span: Span) -> ast::Expr {
        build_index_expr(
            build_path_ident_expr(QUBIT_REGISTER, span, span),
            build_lit_int_expr(i64::from(qubit), span),
            span,
        )
    }

    fn noise_table_call(
        &mut self,
        table: NoiseTable<f64>,
        qubits: &[StimQubitId],
        span: Span,
    ) -> ast::Stmt {
        let name = self.noise_accumulator.get_or_insert_intrinsic(table);
        let args = self.qubit_args(qubits, span);
        let param_tys = vec!["Qubit"; qubits.len()];
        let declaration = build_intrinsic_decl(&name, "NoiseIntrinsic", &param_tys, "Unit", span);
        self.declare_intrinsic(&name, declaration);
        self.intrinsic_stmt(&name, args, span)
    }

    fn push_error(&mut self, error: Error) {
        self.errors.push(error);
    }
}

pub fn compile_to_qsharp_ast(
    circuit: &semantic::Circuit,
    noise: &mut NoiseConfig<f64, f64>,
) -> Result<ast::Package, Vec<Error>> {
    Compiler::new(noise).into_package(circuit)
}
