// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Neutral-atom hardware scheduling pass.
//!
//! Rust equivalent of `_scheduler.py`.  Groups quantum gates into parallel
//! layers constrained by hardware capabilities:
//! - Inserts `__quantum__rt__begin_parallel` / `end_parallel` markers.
//! - Inserts `__quantum__qis__move__body` instructions for atom rearrangement.
//!
//! The scheduling algorithm operates on a single basic block at a time,
//! batching CZ operations into interaction-zone rows and measurements into
//! measurement-zone slots, with appropriate qubit movement.

use pyo3::{PyResult, exceptions::PyValueError, pyfunction};
use qsc_llvm::{
    model::Type,
    model::{Function, Instruction, Module, Param},
    parse_module,
    qir::{self, i64_op, operand_key, qis, qubit_op, rt, void_call},
    write_module_to_string,
};
use rustc_hash::FxHashSet;
use std::collections::BTreeMap;

use super::atom_utils::as_qis_gate;

const MOVE_GROUPS_PER_PARALLEL_SECTION: usize = 1;

fn begin_parallel() -> Instruction {
    void_call(rt::BEGIN_PARALLEL, vec![])
}

fn end_parallel() -> Instruction {
    void_call(rt::END_PARALLEL, vec![])
}

fn move_instr(qubit_id: u32, row: i64, col: i64) -> Instruction {
    void_call(
        qis::MOVE,
        vec![qubit_op(qubit_id), i64_op(row), i64_op(col)],
    )
}

#[derive(Clone)]
struct ZoneInfo {
    row_count: usize,
    offset: usize, // offset in cells (= zone_row_offset * column_count)
    zone_type: ZoneKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ZoneKind {
    Register,
    Interaction,
    Measurement,
}

struct DeviceConfig {
    column_count: usize,
    zones: Vec<ZoneInfo>,
    home_locs: Vec<(i64, i64)>,
}

impl DeviceConfig {
    fn interaction_zone(&self) -> &ZoneInfo {
        self.zones
            .iter()
            .find(|z| z.zone_type == ZoneKind::Interaction)
            .expect("device must have an interaction zone")
    }

    fn measurement_zone(&self) -> &ZoneInfo {
        self.zones
            .iter()
            .find(|z| z.zone_type == ZoneKind::Measurement)
            .expect("device must have a measurement zone")
    }

    fn get_home_loc(&self, q: u32) -> (i64, i64) {
        self.home_locs.get(q as usize).copied().unwrap_or((0, 0))
    }

    fn get_ordering(&self, q: u32) -> u32 {
        let (row, col) = self.get_home_loc(q);
        #[allow(clippy::cast_sign_loss)]
        let val = row as u32 * self.column_count as u32 + col as u32;
        val
    }
}

type Location = (i64, i64);

struct MoveOp {
    qubit_id: u32,
    src_loc: Location,
    dst_loc: Location,
}

/// Schedule moves for a set of qubits into a target zone.
/// Returns groups of moves that can be executed in parallel.
///
/// This is a simplified version of the Python `MoveScheduler` that focuses
/// on assigning destinations and grouping, without the sophisticated
/// scale-factor-based move-group optimization (which can be added later
/// for optimal parallelism).
fn schedule_moves(
    device: &DeviceConfig,
    zone: &ZoneInfo,
    qubits_to_move: &[QubitToMove],
) -> Vec<Vec<MoveOp>> {
    let zone_row_offset = zone.offset / device.column_count;
    let mut available: BTreeMap<Location, ()> = BTreeMap::new();
    for row in zone_row_offset..(zone_row_offset + zone.row_count) {
        for col in 0..device.column_count {
            available.insert((row as i64, col as i64), ());
        }
    }

    let mut all_moves: Vec<MoveOp> = Vec::new();

    for qtm in qubits_to_move {
        match qtm {
            QubitToMove::Single(q) => {
                let src = device.get_home_loc(*q);
                // Prefer straight-up/down move (same column).
                let mut dst = None;
                for row in zone_row_offset..(zone_row_offset + zone.row_count) {
                    let loc = (row as i64, src.1);
                    if available.contains_key(&loc) {
                        dst = Some(loc);
                        break;
                    }
                }
                if dst.is_none() {
                    // Fallback: any available.
                    dst = available.keys().next().copied();
                }
                if let Some(d) = dst {
                    available.remove(&d);
                    all_moves.push(MoveOp {
                        qubit_id: *q,
                        src_loc: src,
                        dst_loc: d,
                    });
                }
            }
            QubitToMove::Pair(q1, q2) => {
                let src1 = device.get_home_loc(*q1);
                // CZ pair: place on adjacent even/odd columns in same row.
                let mut dst1 = None;
                let mut dst2 = None;
                let src_col = if src1.1 % 2 == 0 { src1.1 } else { src1.1 - 1 };
                for row in zone_row_offset..(zone_row_offset + zone.row_count) {
                    let loc1 = (row as i64, src_col);
                    let loc2 = (row as i64, src_col + 1);
                    if available.contains_key(&loc1) && available.contains_key(&loc2) {
                        dst1 = Some(loc1);
                        dst2 = Some(loc2);
                        break;
                    }
                }
                // Fallback: find any adjacent pair.
                if dst1.is_none() {
                    for row in zone_row_offset..(zone_row_offset + zone.row_count) {
                        for col in (0..device.column_count).step_by(2) {
                            let loc1 = (row as i64, col as i64);
                            let loc2 = (row as i64, col as i64 + 1);
                            if available.contains_key(&loc1) && available.contains_key(&loc2) {
                                dst1 = Some(loc1);
                                dst2 = Some(loc2);
                                break;
                            }
                        }
                        if dst1.is_some() {
                            break;
                        }
                    }
                }
                if let (Some(d1), Some(d2)) = (dst1, dst2) {
                    available.remove(&d1);
                    available.remove(&d2);
                    all_moves.push(MoveOp {
                        qubit_id: *q1,
                        src_loc: device.get_home_loc(*q1),
                        dst_loc: d1,
                    });
                    all_moves.push(MoveOp {
                        qubit_id: *q2,
                        src_loc: device.get_home_loc(*q2),
                        dst_loc: d2,
                    });
                }
            }
        }
    }

    // Group moves: for simplicity, put all moves in one group sorted by qubit ID.
    // The Python code uses a sophisticated MoveGroupPool; this simplified version
    // just returns a single group, which is correct but less optimal for parallelism.
    if all_moves.is_empty() {
        Vec::new()
    } else {
        all_moves.sort_by_key(|m| m.qubit_id);
        vec![all_moves]
    }
}

#[derive(Clone)]
enum QubitToMove {
    Single(u32),
    Pair(u32, u32),
}

struct SchedulerState<'a> {
    device: &'a DeviceConfig,
    num_qubits: usize,
    single_qubit_ops: Vec<Vec<(Instruction, String)>>, // per-qubit queued ops
    curr_cz_ops: Vec<Instruction>,
    measurements: Vec<(Instruction, String)>, // (instr, gate_name)
    pending_qubits_to_move: Vec<QubitToMove>,
    pending_moves: Vec<Vec<MoveOp>>,
    vals_used_in_cz: FxHashSet<String>,
    vals_used_in_meas: FxHashSet<String>,
    output: Vec<Instruction>,
}

fn get_call_used_values(instr: &Instruction) -> (Vec<String>, Vec<String>) {
    let mut vals = Vec::new();
    let mut meas = Vec::new();
    if let Instruction::Call { callee, args, .. } = instr {
        match callee.as_str() {
            s if s == qis::MRESETZ || s == qis::M || s == qis::MZ => {
                if let Some(first) = args.first() {
                    vals.push(operand_key(&first.1));
                }
                for a in args.iter().skip(1) {
                    meas.push(operand_key(&a.1));
                }
            }
            _ => {
                for a in args {
                    vals.push(operand_key(&a.1));
                }
            }
        }
    }
    (vals, meas)
}

impl<'a> SchedulerState<'a> {
    fn new(device: &'a DeviceConfig) -> Self {
        let n = device.home_locs.len();
        Self {
            device,
            num_qubits: n,
            single_qubit_ops: vec![Vec::new(); n],
            curr_cz_ops: Vec::new(),
            measurements: Vec::new(),
            pending_qubits_to_move: Vec::new(),
            pending_moves: Vec::new(),
            vals_used_in_cz: FxHashSet::default(),
            vals_used_in_meas: FxHashSet::default(),
            output: Vec::new(),
        }
    }

    fn any_pending_sq(&self) -> bool {
        self.single_qubit_ops.iter().any(|ops| !ops.is_empty())
    }

    fn any_pending_cz(&self) -> bool {
        !self.curr_cz_ops.is_empty()
    }

    fn any_pending_meas(&self) -> bool {
        !self.measurements.is_empty()
    }

    fn any_pending(&self) -> bool {
        self.any_pending_cz() || self.any_pending_sq() || self.any_pending_meas()
    }

    fn insert_moves(&mut self) {
        let mut group_id = 0usize;
        for group in &self.pending_moves {
            if group_id == 0 {
                self.output.push(begin_parallel());
            }
            for m in group {
                self.output
                    .push(move_instr(m.qubit_id, m.dst_loc.0, m.dst_loc.1));
            }
            group_id += 1;
            if group_id >= MOVE_GROUPS_PER_PARALLEL_SECTION {
                group_id = 0;
                self.output.push(end_parallel());
            }
        }
        if group_id != 0 {
            self.output.push(end_parallel());
        }
    }

    fn insert_moves_back(&mut self) {
        let mut group_id = 0usize;
        for group in &self.pending_moves {
            if group_id == 0 {
                self.output.push(begin_parallel());
            }
            for m in group {
                self.output
                    .push(move_instr(m.qubit_id, m.src_loc.0, m.src_loc.1));
            }
            group_id += 1;
            if group_id >= MOVE_GROUPS_PER_PARALLEL_SECTION {
                group_id = 0;
                self.output.push(end_parallel());
            }
        }
        if group_id != 0 {
            self.output.push(end_parallel());
        }
        self.pending_moves.clear();
    }

    fn target_qubits_by_row(&self, zone: &ZoneInfo) -> Vec<Vec<u32>> {
        let zone_row_offset = zone.offset / self.device.column_count;
        let mut by_row: Vec<Vec<u32>> = vec![Vec::new(); zone.row_count];
        for group in &self.pending_moves {
            for m in group {
                let row_idx = (m.dst_loc.0 as usize).saturating_sub(zone_row_offset);
                if row_idx < zone.row_count {
                    by_row[row_idx].push(m.qubit_id);
                }
            }
        }
        for row in &mut by_row {
            row.sort_unstable();
        }
        by_row
    }

    fn flush_single_qubit_ops(&mut self, target_qubits: &[u32]) {
        // Gather ops to flush.
        let mut ops_to_flush: Vec<Vec<(Instruction, String)>> = Vec::new();
        for &q in target_qubits {
            let idx = q as usize;
            if idx < self.num_qubits {
                let mut ops = std::mem::take(&mut self.single_qubit_ops[idx]);
                ops.reverse();
                ops_to_flush.push(ops);
            } else {
                ops_to_flush.push(Vec::new());
            }
        }

        while ops_to_flush.iter().any(|ops| !ops.is_empty()) {
            // Collect rz ops.
            let mut rz_ops = Vec::new();
            for q_ops in &mut ops_to_flush {
                if let Some(last) = q_ops.last() {
                    if last.1 == "rz" {
                        rz_ops.push(q_ops.pop().expect("just checked").0);
                    }
                }
            }
            if !rz_ops.is_empty() {
                self.output.push(begin_parallel());
                self.output.extend(rz_ops);
                self.output.push(end_parallel());
            }

            // Collect sx ops.
            let mut sx_ops = Vec::new();
            for q_ops in &mut ops_to_flush {
                if let Some(last) = q_ops.last() {
                    if last.1 == "sx" {
                        sx_ops.push(q_ops.pop().expect("just checked").0);
                    }
                }
            }
            if !sx_ops.is_empty() {
                self.output.push(begin_parallel());
                self.output.extend(sx_ops);
                self.output.push(end_parallel());
            }
        }
    }

    fn schedule_pending_moves(&mut self, zone: &ZoneInfo) {
        let moves = schedule_moves(self.device, zone, &self.pending_qubits_to_move);
        self.pending_moves.extend(moves);
        self.pending_qubits_to_move.clear();
    }

    fn flush_pending(&mut self) {
        let iz = self.device.interaction_zone().clone();
        let mz = self.device.measurement_zone().clone();

        if self.any_pending_cz() {
            self.schedule_pending_moves(&iz);
            self.insert_moves();
            let qubits_by_row = self.target_qubits_by_row(&iz);
            for row_qubits in &qubits_by_row {
                self.flush_single_qubit_ops(row_qubits);
            }
            self.output.push(begin_parallel());
            let cz_ops = std::mem::take(&mut self.curr_cz_ops);
            self.output.extend(cz_ops);
            self.output.push(end_parallel());
            self.insert_moves_back();
            self.vals_used_in_cz.clear();
        } else if self.any_pending_meas() {
            self.schedule_pending_moves(&mz);
            self.insert_moves();
            self.output.push(begin_parallel());
            let meas = std::mem::take(&mut self.measurements);
            for (instr, _) in meas {
                self.output.push(instr);
            }
            self.output.push(end_parallel());
            self.vals_used_in_meas.clear();
            self.insert_moves_back();
        } else {
            // Single-qubit ops only: move to IZ, execute, move back.
            while self.any_pending_sq() {
                let mut target_qubits_by_row: Vec<Vec<u32>> = vec![Vec::new(); iz.row_count];
                let mut curr_row = 0usize;
                for q in 0..self.num_qubits {
                    if !self.single_qubit_ops[q].is_empty() {
                        target_qubits_by_row[curr_row].push(q as u32);
                        if target_qubits_by_row[curr_row].len() >= self.device.column_count {
                            curr_row += 1;
                            if curr_row >= iz.row_count {
                                break;
                            }
                        }
                    }
                }
                for target_qs in &target_qubits_by_row {
                    for &q in target_qs {
                        let idx = q as usize;
                        if idx < self.num_qubits && !self.single_qubit_ops[idx].is_empty() {
                            // Determine qubit operand from the first instruction.
                            let gate_name = &self.single_qubit_ops[idx][0].1;
                            let _qubit_arg_idx = if gate_name == "rz" { 1 } else { 0 };
                            self.pending_qubits_to_move.push(QubitToMove::Single(q));
                        }
                    }
                }
                self.schedule_pending_moves(&iz);
                self.insert_moves();
                let qubits_by_row = self.target_qubits_by_row(&iz);
                for row_qubits in &qubits_by_row {
                    self.flush_single_qubit_ops(row_qubits);
                }
                self.insert_moves_back();
            }
        }
    }

    fn schedule_block(&mut self, instrs: Vec<Instruction>) {
        let iz = self.device.interaction_zone().clone();
        let mz = self.device.measurement_zone().clone();
        let max_iz_pairs = (self.device.column_count / 2) * iz.row_count;
        let max_measurements = self.device.column_count * mz.row_count;

        self.single_qubit_ops = vec![Vec::new(); self.num_qubits];
        self.curr_cz_ops.clear();
        self.measurements.clear();
        self.pending_qubits_to_move.clear();
        self.vals_used_in_cz.clear();
        self.vals_used_in_meas.clear();

        for instr in instrs {
            if let Instruction::Call { callee, args, .. } = &instr {
                if let Some(gate) = as_qis_gate(callee, args) {
                    // Single-qubit gate (no result args).
                    if gate.qubit_args.len() == 1 && gate.result_args.is_empty() {
                        let q = gate.qubit_args[0];

                        // Check if qubit is involved in pending moves.
                        let involved_in_moves =
                            self.pending_qubits_to_move.iter().any(|qtm| match qtm {
                                QubitToMove::Single(id) => *id == q,
                                QubitToMove::Pair(id1, id2) => *id1 == q || *id2 == q,
                            });
                        if involved_in_moves {
                            self.flush_pending();
                        }

                        if (q as usize) < self.num_qubits {
                            self.single_qubit_ops[q as usize].push((instr, gate.gate));
                        } else {
                            self.output.push(instr);
                        }
                        continue;
                    }

                    // Two-qubit gate (CZ after decomposition).
                    if gate.qubit_args.len() == 2 {
                        let (vals, _) = get_call_used_values(&instr);
                        let val_set: FxHashSet<_> = vals.into_iter().collect();
                        if self.any_pending_meas()
                            || val_set.iter().any(|v| self.vals_used_in_cz.contains(v))
                            || self.curr_cz_ops.len() >= max_iz_pairs
                        {
                            self.flush_pending();
                        }
                        self.curr_cz_ops.push(instr.clone());
                        self.vals_used_in_cz.extend(val_set);

                        let q0 = gate.qubit_args[0];
                        let q1 = gate.qubit_args[1];
                        let home0 = self.device.get_home_loc(q0);
                        let home1 = self.device.get_home_loc(q1);
                        if home0.1 > home1.1 {
                            self.pending_qubits_to_move.push(QubitToMove::Pair(q1, q0));
                        } else {
                            self.pending_qubits_to_move.push(QubitToMove::Pair(q0, q1));
                        }
                        continue;
                    }

                    // Measurement.
                    if !gate.result_args.is_empty() {
                        let (vals, _) = get_call_used_values(&instr);
                        let val_set: FxHashSet<_> = vals.into_iter().collect();
                        if !self.measurements.is_empty()
                            && (self.measurements.len() >= max_measurements
                                || val_set.iter().any(|v| self.vals_used_in_meas.contains(v)))
                        {
                            self.flush_pending();
                        }

                        // Flush pending single-qubit ops for qubit being measured.
                        let q = gate.qubit_args[0];
                        if (q as usize) < self.num_qubits
                            && !self.single_qubit_ops[q as usize].is_empty()
                        {
                            let temp_meas = std::mem::take(&mut self.measurements);
                            let temp_moves = std::mem::take(&mut self.pending_qubits_to_move);
                            self.flush_pending();
                            self.measurements = temp_meas;
                            self.pending_qubits_to_move = temp_moves;
                        }

                        self.measurements.push((instr.clone(), gate.gate));
                        self.vals_used_in_meas.extend(val_set);
                        self.pending_qubits_to_move
                            .push(QubitToMove::Single(gate.qubit_args[0]));
                        continue;
                    }
                }
            }

            // Non-gate instruction: flush everything, then emit.
            while self.any_pending() {
                self.flush_pending();
            }
            self.output.push(instr);
        }
    }
}

fn schedule_module(module: &mut Module, device: &DeviceConfig) {
    // Ensure declarations for parallel markers and move function.
    super::atom_decomp::ensure_declaration(module, rt::BEGIN_PARALLEL);
    super::atom_decomp::ensure_declaration(module, rt::END_PARALLEL);
    // Add move function declaration with correct signature.
    if !module.functions.iter().any(|f| f.name == qis::MOVE) {
        module.functions.push(Function {
            name: qis::MOVE.to_string(),
            return_type: Type::Void,
            params: vec![
                Param {
                    ty: Type::NamedPtr(qir::QUBIT_TYPE_NAME.to_string()),
                    name: None,
                },
                Param {
                    ty: Type::Integer(64),
                    name: None,
                },
                Param {
                    ty: Type::Integer(64),
                    name: None,
                },
            ],
            is_declaration: true,
            attribute_group_refs: Vec::new(),
            basic_blocks: Vec::new(),
        });
    }

    for func in &mut module.functions {
        if func.is_declaration {
            continue;
        }
        for bb in &mut func.basic_blocks {
            let instrs = std::mem::take(&mut bb.instructions);
            let mut state = SchedulerState::new(device);
            state.schedule_block(instrs);
            bb.instructions = state.output;
        }
    }
}

/// Schedule quantum operations for neutral-atom hardware.
///
/// Parameters:
/// - `ir`: LLVM IR text.
/// - `column_count`: device column count.
/// - `zone_row_counts`: list of row counts for each zone.
/// - `zone_types`: list of zone types (0=register, 1=interaction, 2=measurement).
/// - `home_locs`: flat list of (row, col) tuples for each qubit's home location.
#[pyfunction]
pub fn atom_schedule(
    ir: &str,
    column_count: usize,
    zone_row_counts: Vec<usize>,
    zone_types: Vec<u32>,
    home_locs: Vec<(i64, i64)>,
) -> PyResult<String> {
    let mut module =
        parse_module(ir).map_err(|e| PyValueError::new_err(format!("parse error: {e}")))?;

    // Build zones.
    let mut zones = Vec::new();
    let mut offset = 0usize;
    for (rc, zt) in zone_row_counts.iter().zip(zone_types.iter()) {
        let kind = match zt {
            0 => ZoneKind::Register,
            1 => ZoneKind::Interaction,
            2 => ZoneKind::Measurement,
            _ => {
                return Err(PyValueError::new_err(format!("unknown zone type: {zt}")));
            }
        };
        zones.push(ZoneInfo {
            row_count: *rc,
            offset: offset * column_count,
            zone_type: kind,
        });
        offset += rc;
    }

    let device = DeviceConfig {
        column_count,
        zones,
        home_locs,
    };

    schedule_module(&mut module, &device);
    Ok(write_module_to_string(&module))
}
