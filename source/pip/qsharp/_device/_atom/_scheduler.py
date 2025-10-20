# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ._utils import as_qis_gate, get_used_values, uses_any_value
from pyqir import (
    Call,
    Instruction,
    Function,
    QirModuleVisitor,
    FunctionType,
    Type,
    Linkage,
    qubit_type,
    qubit_id,
    IntType,
)
from .._device import Device


class Schedule(QirModuleVisitor):
    """
    Schedule instructions within a block, adding appropriate moves to the interaction zone to perform operations
    """

    begin_func: Function
    end_func: Function
    move_func: Function

    def __init__(self, device: Device):
        super().__init__()
        self.device = device
        self.num_qubits = len(self.device.home_locs)

    def _on_module(self, module):
        i64_ty = IntType(module.context, 64)
        # Find or create the necessary runtime functions.
        for func in module.functions:
            if func.name == "__quantum__rt__begin_parallel":
                self.begin_func = func
            elif func.name == "__quantum__rt__end_parallel":
                self.end_func = func
            elif func.name == "__quantum__qis__move__body":
                self.move_func = func
        if not hasattr(self, "begin_func"):
            self.begin_func = Function(
                FunctionType(
                    Type.void(module.context),
                    [],
                ),
                Linkage.EXTERNAL,
                "__quantum__rt__begin_parallel",
                module,
            )
        if not hasattr(self, "end_func"):
            self.end_func = Function(
                FunctionType(
                    Type.void(module.context),
                    [],
                ),
                Linkage.EXTERNAL,
                "__quantum__rt__end_parallel",
                module,
            )
        if not hasattr(self, "move_func"):
            self.move_func = Function(
                FunctionType(
                    Type.void(module.context),
                    [qubit_type(module.context), i64_ty, i64_ty],
                ),
                Linkage.EXTERNAL,
                "__quantum__qis__move__body",
                module,
            )
        super()._on_module(module)

    def _on_block(self, block):
        # Use only the first interaction and measurement zone; more could be supported in future.
        interaction_zone = self.device.get_interaction_zones()[0]
        interaction_zone_row_offset = (
            interaction_zone.offset // self.device.column_count
        )
        measurement_zone = self.device.get_measurement_zones()[0]
        measurement_zone_row_offset = (
            measurement_zone.offset // self.device.column_count
        )
        iz_pairs_per_row = self.device.column_count // 2
        max_measurements = self.device.column_count * measurement_zone.row_count

        # Track pending/queued single qubit operations by qubit id.
        self.single_qubit_ops = [[] for i in range(self.num_qubits)]

        # Track pending CZ operations by interaction zone row.
        self.cz_ops_by_row = [[] for i in range(interaction_zone.row_count)]

        # Track pending measurements.
        self.measurements = []

        # Track pending moves (qubit, (row, col)).
        self.pending_moves = []

        # Track values used in CZ ops and measurements to avoid putting operations on the
        # same qubit in the same batch.
        self.vals_used_in_cz_ops = set()
        self.vals_used_in_measurements = set()

        instructions = [instr for instr in block.instructions]
        for instr in instructions:
            gate = as_qis_gate(instr)
            if (
                gate != {}
                and len(gate["qubit_args"]) == 1
                and len(gate["result_args"]) == 0
            ):
                # This is a single qubit gate; queue it up for later execution when this qubit is needed for CZ or measurement.

                # If this qubit is involved in pending moves, that implies a CZ or measurement is pending, so flush now.
                if len(self.pending_moves) > 0 and any(
                    [
                        gate["qubit_args"][0] == qubit_id(q)
                        for q, _ in self.pending_moves
                    ]
                ):
                    self.flush_pending(instr)

                # Remove the instruction from the block and queue by the qubit id.
                instr.remove()
                self.single_qubit_ops[gate["qubit_args"][0]].append((instr, gate))

            elif gate != {} and len(gate["qubit_args"]) == 2:
                # This is a CZ gate; queue it up to be executed in the next available interaction zone row.

                # Pick next available interaction zone pair for these qubits. If none, flush the current set and start a fresh set.
                # Create move instructions to move qubits to interaction zone and save them in pending moves for later insertion.
                assert isinstance(instr, Call)
                (vals_used, _) = get_used_values(instr)
                if (
                    len(self.measurements) > 0
                    or uses_any_value(vals_used, self.vals_used_in_cz_ops)
                    or len(self.cz_ops_by_row[-1]) >= iz_pairs_per_row
                ):
                    self.flush_pending(instr)
                instr.remove()
                row = 0
                while row < interaction_zone.row_count:
                    if len(self.cz_ops_by_row[row]) < iz_pairs_per_row:
                        self.cz_ops_by_row[row].append((instr, gate))
                        self.vals_used_in_cz_ops.update(vals_used)
                        break
                    row += 1
                assert (
                    row < interaction_zone.row_count
                ), "Should have found a row for CZ operation"

                # Compute the column of the interaction zone pair location for each qubit.
                col1 = (len(self.cz_ops_by_row[row]) - 1) * 2
                col2 = col1 + 1
                loc1 = (row + interaction_zone_row_offset, col1)
                loc2 = (row + interaction_zone_row_offset, col2)
                self.pending_moves.append((instr.args[0], loc1))
                self.pending_moves.append((instr.args[1], loc2))

            elif gate != {} and len(gate["result_args"]) == 1:
                # This is a measurement; queue it up to be executed in the measurement zone.

                # Pick next available measurement zone location for this qubit. If none, flush the current set and start a fresh set.
                # Create move instructions to move qubit to measurement zone and save them in pending moves for later insertion.
                assert isinstance(instr, Call)
                (vals_used, _) = get_used_values(instr)
                if (
                    len(self.measurements) == 0
                    or len(self.measurements) >= max_measurements
                    or uses_any_value(vals_used, self.vals_used_in_measurements)
                ):
                    self.flush_pending(instr)
                if len(self.single_qubit_ops[gate["qubit_args"][0]]) > 0:
                    # There are still pending single qubits ops for the qubit we want to measure,
                    # so trigger another flush.
                    # We need to cache and restore the measurements and pending moves that have already
                    # been queued so that this flush affects the single qubit ops but not the measurements.
                    temp_meas = self.measurements
                    self.measurements = []
                    temp_moves = self.pending_moves
                    self.pending_moves = []
                    self.flush_pending(instr)
                    self.measurements = temp_meas
                    self.pending_moves = temp_moves

                # Remove the measurement from the block and queue it.
                instr.remove()
                idx = len(self.measurements)
                row = idx // self.device.column_count
                col = idx % self.device.column_count
                loc = (row + measurement_zone_row_offset, col)
                self.measurements.append((instr, gate))
                self.vals_used_in_measurements.update(vals_used)
                self.pending_moves.append((instr.args[0], loc))
            else:
                # This is not a gate or measurement; flush any pending operations and leave the instruction in place.
                # This uses a while loop to ensure all pending operations are flushed before the instruction.
                while (
                    any(len(q_ops) > 0 for q_ops in self.single_qubit_ops)
                    or any(len(row) > 0 for row in self.cz_ops_by_row)
                    or len(self.measurements) > 0
                ):
                    self.flush_pending(instr)

    def flush_pending(self, insert_before: Instruction):
        self.builder.insert_before(insert_before)
        # If cz ops pending, insert accumulated moves, single qubits ops matching cz rows, then the cz ops, then move back.
        if any(len(cz_row) > 0 for cz_row in self.cz_ops_by_row):
            self.insert_moves()
            all_cz_ops = []
            for row_ops in self.cz_ops_by_row:
                targets_in_row = []
                for cz_op, cz_gate in row_ops:
                    targets_in_row.append(cz_gate["qubit_args"][0])
                    targets_in_row.append(cz_gate["qubit_args"][1])
                    all_cz_ops.append(cz_op)
                self.flush_single_qubit_ops(targets_in_row)
            self.builder.call(self.begin_func, [])
            for cz_op in all_cz_ops:
                self.builder.instr(cz_op)
            self.builder.call(self.end_func, [])
            self.cz_ops_by_row = [
                [] for i in range(self.device.get_interaction_zones()[0].row_count)
            ]
            self.insert_moves_back()
            self.pending_moves = []
            self.vals_used_in_cz_ops = set()
            return
        # If measurements pending, insert accumulated moves, then measurements, then move back.
        elif len(self.measurements) > 0:
            self.insert_moves()
            self.builder.call(self.begin_func, [])
            for meas_op, meas_gate in self.measurements:
                self.builder.instr(meas_op)
            self.builder.call(self.end_func, [])
            self.measurements = []
            self.vals_used_in_measurements = set()
            self.insert_moves_back()
            self.pending_moves = []
            return
        # Else, create movements for remaining single qubit ops to the first interaction zone,
        # insert those moves, then the ops, then move back.
        else:
            interaction_zone = self.device.get_interaction_zones()[0]
            interaction_zone_row_offset = (
                interaction_zone.offset // self.device.column_count
            )
            while any(len(q_ops) > 0 for q_ops in self.single_qubit_ops):
                target_qubits_by_row = [[] for i in range(interaction_zone.row_count)]
                curr_row = 0
                for q in range(self.num_qubits):
                    if len(self.single_qubit_ops[q]) > 0:
                        target_qubits_by_row[curr_row].append(q)
                        if (
                            len(target_qubits_by_row[curr_row])
                            >= self.device.column_count
                        ):
                            curr_row += 1
                            if curr_row >= interaction_zone.row_count:
                                break
                for row, target_qubits in enumerate(target_qubits_by_row):
                    for i, q in enumerate(target_qubits):
                        col = i
                        loc = (row + interaction_zone_row_offset, col)
                        qubit = self.single_qubit_ops[q][0][0].args[0]
                        if self.single_qubit_ops[q][0][1]["gate"] == "rz":
                            qubit = self.single_qubit_ops[q][0][0].args[1]
                        self.pending_moves.append((qubit, loc))
                self.insert_moves()
                for target_qubits in target_qubits_by_row:
                    self.flush_single_qubit_ops(target_qubits)
                self.insert_moves_back()
                self.pending_moves = []
            return

    def insert_moves(self):
        self.builder.call(self.begin_func, [])
        # For each pending move, insert a call to the move function that moves the
        # given qubit to the given (row, col) location.
        for id, loc in self.pending_moves:
            self.builder.call(
                self.move_func,
                [
                    id,
                    loc[0],
                    loc[1],
                ],
            )
        self.builder.call(self.end_func, [])

    def insert_moves_back(self):
        self.builder.call(self.begin_func, [])
        # For each pending move, insert a call to the move function that moves the
        # given qubit back to its home location.
        for id, loc in self.pending_moves:
            q_id = qubit_id(id)
            assert q_id is not None, "Qubit id should be known"
            home_loc = self.device.get_home_loc(q_id)
            self.builder.call(
                self.move_func,
                [
                    id,
                    home_loc[0],
                    home_loc[1],
                ],
            )
        self.builder.call(self.end_func, [])

    def flush_single_qubit_ops(self, target_qubits):
        # Flush all pending single qubit ops for the given target qubits, combining
        # consecutive ops of the same type into a single parallel region by row in
        # the interaction zone.
        ops_to_flush = []
        for q in target_qubits:
            ops_to_flush.append(list(reversed(self.single_qubit_ops[q])))
            self.single_qubit_ops[q] = []
        while any(len(q_ops) > 0 for q_ops in ops_to_flush):
            rz_ops = []
            for q_ops in ops_to_flush:
                if len(q_ops) == 0:
                    continue
                if q_ops[-1][1]["gate"] == "rz":
                    rz_ops.append(q_ops.pop()[0])
            if len(rz_ops) > 0:
                self.builder.call(self.begin_func, [])
                for rz_op in rz_ops:
                    self.builder.instr(rz_op)
                self.builder.call(self.end_func, [])
            sx_ops = []
            for q_ops in ops_to_flush:
                if len(q_ops) == 0:
                    continue
                if q_ops[-1][1]["gate"] == "sx":
                    sx_ops.append(q_ops.pop()[0])
            if len(sx_ops) > 0:
                self.builder.call(self.begin_func, [])
                for sx_op in sx_ops:
                    self.builder.instr(sx_op)
                self.builder.call(self.end_func, [])
