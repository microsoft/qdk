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
    Value,
)
from .._device import Device
from dataclasses import dataclass
from itertools import combinations, islice, chain
from typing import Iterable, TypeAlias, Optional, Tuple
from fractions import Fraction
from functools import lru_cache

QubitId: TypeAlias = Value
Location: TypeAlias = tuple[int, int]
MoveScale: TypeAlias = tuple[bool | Fraction, bool | Fraction]
Move: TypeAlias = tuple[QubitId, Location, Location]


@dataclass
class PartialMove:
    qubit_id_ptr: Value
    src_loc: Location


PartialMovePair: TypeAlias = tuple[PartialMove, PartialMove]


def partial_move_parity(source: Location) -> int:
    """Returns the parity of the source column."""
    return source[1] % 2


def move_parity(source: Location, destination: Location) -> tuple[int, int]:
    """Returns a tuple representing the parities of the source and destination columns."""
    return (source[1] % 2, destination[1] % 2)


def partial_move_direction(source: Location) -> int:
    """Return an int representing if the move is up or down."""
    # TODO: this should be a function of the Device.
    return int(source[0] < 17)


def move_direction(source: Location, destination: Location) -> tuple[int, int]:
    """Returns a tuple representing if the move is up or down, and left or right."""
    return (int(source[0] < destination[0]), int(source[1] < destination[1]))


def index_from_parity_and_direction(
    row_parity: int, col_parity, ud: int, lr: int
) -> int:
    major_index = 2 * row_parity + col_parity
    minor_index = 2 * ud + lr
    return 4 * major_index + minor_index


def move_qubit_id(move: Move) -> int:
    """Returns the qubit id of the move."""
    id = qubit_id(move[0])
    assert id is not None, "Qubit id should be known"
    return id


def is_invalid_move_pair(move1: Move, move2: Move) -> bool:
    """
    Returns true if the two moves are incompatible, i.e., if they have the same
    source row then they must have the same destination row, and if they have the
    same source column then they must have the same destination column.
    """

    source_row_diff = move1[1][0] - move2[1][0]
    destination_row_diff = move1[2][0] - move2[2][0]
    source_col_diff = move1[1][1] - move2[1][1]
    destination_col_diff = move1[2][1] - move2[2][1]

    return (
        (source_row_diff == 0 and destination_row_diff != 0)
        or (source_row_diff != 0 and destination_row_diff == 0)
        or (source_col_diff == 0 and destination_col_diff != 0)
        or (source_col_diff != 0 and destination_col_diff == 0)
    )


@lru_cache(maxsize=1 << 14)
def move_scale_helper(source_diff, destination_diff):
    return True if destination_diff == 0 else Fraction(source_diff, destination_diff)


def move_scale(move1: Move, move2: Move) -> MoveScale:
    """
    Returns a tuple of two elements, representing the row displacement ratio and column
    displacement ratio between the moves.
    """
    source_row_diff = move1[1][0] - move2[1][0]
    destination_row_diff = move1[2][0] - move2[2][0]
    source_col_diff = move1[1][1] - move2[1][1]
    destination_col_diff = move1[2][1] - move2[2][1]
    return move_scale_helper(source_row_diff, destination_row_diff), move_scale_helper(
        source_col_diff, destination_col_diff
    )


class ParallelCandidate:
    """
    Represents a parallel move set candidate. It has three fields:
        moves[set]: A set of moves that can be performed in parallel.
        move_scale[Optional[tuple[Fraction, Fraction]]]: A tuple of fractions
            representing the scale factors in the row and col axes between
            moves. `None`, if there is a single element in the move set.
    """

    def __init__(self, moves: Iterable[Move]):
        self.moves = set(moves)
        self.move_scale = move_scale(*moves) if len(self.moves) > 1 else None
        self.ref_move = next(iter(moves))

    def __len__(self) -> int:
        return len(self.moves)

    def add(self, move: Move):
        if not self.move_scale:
            raise Exception("this parallel candidate is an individual move")
        self.moves.add(move)

    def remove(self, move: Move):
        self.moves.remove(move)

    def discard(self, move: Move):
        self.moves.discard(move)


class ParallelMoves:
    """
    A data structure that organizes moves into parallelizable sets.
    It provides an `is_empty()` method to check if there are any moves
    left, and a `try_take(n)` method to take up to `n` parallelizable
    moves from the data structure.
    """

    def __init__(
        self,
        moves: list[Move],
        parity: Optional[tuple[int, int]] = None,
        direction: Optional[tuple[int, int]] = None,
    ):
        self.moves = set(moves)
        self.parallel_candidates: dict[Optional[MoveScale], list[ParallelCandidate]] = {
            None: []
        }

        # Edge case with no moves
        if len(moves) == 0:
            self.parity = parity
            self.direction = direction
            return

        self.parity = move_parity(moves[0][1], moves[0][2])
        self.direction = move_direction(moves[0][1], moves[0][2])

        # Edge case in which there is a single move.
        if len(moves) == 1:
            self.parallel_candidates = {None: [ParallelCandidate(moves)]}
            return

        for pair in combinations(moves, 2):
            if is_invalid_move_pair(pair[0], pair[1]):
                continue
            s = move_scale(pair[0], pair[1])
            scaled_pairs = self.parallel_candidates.get(s, [])
            for pc in scaled_pairs:
                if pair[0] in pc.moves:
                    pc.moves.add(pair[1])
                    break
                elif pair[1] in pc.moves:
                    pc.moves.add(pair[0])
                    break
                elif s == move_scale(pair[0], pc.ref_move):
                    pc.moves.add(pair[0])
                    pc.moves.add(pair[1])
                    break
            else:
                scaled_pairs.append(ParallelCandidate(pair))
                self.parallel_candidates[s] = scaled_pairs

        remaining_moves = self.moves.copy()
        for pc in self.parallel_candidates_iter():
            remaining_moves -= pc.moves

        for move in remaining_moves:
            for pc in self.parallel_candidates_iter():

                if pc.move_scale == move_scale(move, pc.ref_move):
                    pc.moves.add(move)
                    break
            else:
                self.parallel_candidates[None].append(ParallelCandidate([move]))

    def parallel_candidates_iter(self) -> Iterable[ParallelCandidate]:
        return chain(*self.parallel_candidates.values())

    def is_empty(self) -> bool:
        return not any(s.moves for s in self.parallel_candidates_iter())

    def largest_parallel_candidate(self) -> Optional[ParallelCandidate]:
        try:
            return max(self.parallel_candidates_iter(), key=len)
        except ValueError:
            return None

    def largest_parallel_candidate_len(self) -> int:
        try:
            return len(max(self.parallel_candidates_iter(), key=len).moves)
        except ValueError:
            return 0

    def push(self, move: Move):
        move_added = False
        for move2 in self.moves:
            pair = (move, move2)
            if is_invalid_move_pair(pair[0], pair[1]):
                continue
            s = move_scale(pair[0], pair[1])
            scaled_pairs = self.parallel_candidates.get(s, [])
            for pc in scaled_pairs:
                if pair[0] in pc.moves:
                    pc.moves.add(pair[1])
                    move_added = True
                    break
                elif pair[1] in pc.moves:
                    pc.moves.add(pair[0])
                    move_added = True
                    break
                elif s == move_scale(pair[0], pc.ref_move):
                    pc.moves.add(pair[0])
                    pc.moves.add(pair[1])
                    move_added = True
                    break
            else:
                scaled_pairs.append(ParallelCandidate(pair))
                self.parallel_candidates[s] = scaled_pairs
                move_added = True

        if not move_added:
            self.parallel_candidates[None].append(ParallelCandidate([move]))

        self.moves.add(move)

    def try_take(self, number_of_moves: int) -> list[Move]:
        # Take `number_of_moves` from the largest parallel candidate.
        if largest_parallel_candidate := self.largest_parallel_candidate():
            moves = list(islice(largest_parallel_candidate.moves, number_of_moves))
            moves_set = set(moves)
            self.moves -= moves_set
            # Remove the taken moves from all parallel candidates.
            for parallel_candidate in self.parallel_candidates_iter():
                parallel_candidate.moves -= moves_set
            return moves
        else:
            return []


class MoveScheduler:

    def __init__(
        self, qubits_to_move: list[QubitId | tuple[QubitId, QubitId]], device: Device
    ):
        self.device = device
        self.available_iz_locations = self.build_iz_locations()
        self.pending_partial_moves = self.qubits_to_partial_moves(qubits_to_move)
        self.disjoint_groups: list[ParallelMoves] = [
            ParallelMoves([], (row_parity, col_parity), (ud, lr))
            for row_parity in (0, 1)
            for col_parity in (0, 1)
            for ud in (0, 1)
            for lr in (0, 1)
        ]

    def build_iz_locations(self) -> dict[Location, None]:
        interaction_zone = self.device.get_interaction_zones()[0]
        interaction_zone_row_offset = (
            interaction_zone.offset // self.device.column_count
        )
        # We use a dict with None values instead of a set to preserve order.
        return {
            (row, col): None
            for row in range(
                interaction_zone_row_offset,
                interaction_zone_row_offset + interaction_zone.row_count,
            )
            for col in range(self.device.column_count)
        }

    def qubits_to_partial_moves(
        self, qubits_to_move: list[QubitId | tuple[QubitId, QubitId]]
    ) -> list[PartialMove | PartialMovePair]:
        partial_moves = []
        for elt in qubits_to_move:
            if isinstance(elt, tuple):
                q_id1 = qubit_id(elt[0])
                q_id2 = qubit_id(elt[1])
                assert q_id1 is not None
                assert q_id2 is not None
                mov1 = PartialMove(elt[0], self.device.get_home_loc(q_id1))
                mov2 = PartialMove(elt[1], self.device.get_home_loc(q_id2))
                partial_moves.append((mov1, mov2))
            else:
                q_id = qubit_id(elt)
                assert q_id is not None
                mov = PartialMove(elt, self.device.get_home_loc(q_id))
                partial_moves.append(mov)

        def sort_key(partial_move: PartialMove | PartialMovePair):
            if isinstance(partial_move, PartialMove):
                return partial_move.src_loc
            else:
                return partial_move[0].src_loc

        return sorted(partial_moves, key=sort_key)

    def split_partial_moves_by_parity_and_direction(
        self, qubits_to_move: list[QubitId]
    ) -> list[list[PartialMove]]:
        partial_moves_by_parity_and_direction = [[] for _ in range(4)]
        for id in qubits_to_move:
            q_id = qubit_id(id)
            assert q_id is not None, "Qubit id should be known"
            source = self.device.get_home_loc(q_id)
            parity = partial_move_parity(source)
            direction = partial_move_direction(source)
            index = 2 * parity + direction
            partial_moves_by_parity_and_direction[index].append((id, source))
        return partial_moves_by_parity_and_direction

    def pending_partial_moves_is_empty(self):
        return not bool(self.pending_partial_moves)

    def next_pending_partial_move(
        self,
    ) -> Optional[PartialMove | PartialMovePair]:
        try:
            return self.pending_partial_moves.pop(0)
        except IndexError:
            return None

    def is_empty(self):
        """
        Returns `True` if all pending moves were scheduled.
        That is, all disjoint groups are empty, and all
        parallel candidates are empty.
        """
        return self.pending_partial_moves_is_empty() and all(
            s.is_empty() for s in self.disjoint_groups
        )

    def largest_parallel_moves(self) -> ParallelMoves:
        return max(
            self.disjoint_groups, key=lambda x: x.largest_parallel_candidate_len()
        )

    def try_take(self, n: int, pm: Optional[ParallelMoves] = None) -> list[Move]:
        if not pm:
            pm = self.largest_parallel_moves()
        return pm.try_take(n)

    def sorted_parallel_moves(self):
        return sorted(
            self.disjoint_groups, key=lambda x: x.largest_parallel_candidate_len()
        )

    def push_to_largest_compatible_parallel_moves(
        self, partial_move: PartialMove
    ) -> ParallelMoves:
        row_parity = partial_move_parity(partial_move.src_loc)
        ud_direction = partial_move_direction(partial_move.src_loc)
        compatible_parallel_moves = [
            self.disjoint_groups[
                index_from_parity_and_direction(
                    row_parity, col_parity, ud_direction, lr_direction
                )
            ]
            for col_parity in (0, 1)
            for lr_direction in (0, 1)
        ]
        compatible_parallel_moves.sort(
            key=lambda pm: pm.largest_parallel_candidate_len(), reverse=True
        )
        for pm in compatible_parallel_moves:
            if move := self.get_compatible_pending_move(pm, partial_move):
                pm.push(move)
                # print(f"pushed move: {move}")
                return pm
        print(self.available_iz_locations, len(self.pending_partial_moves))
        raise Exception("not enough IZ space to schedule all moves")

    def push_pair_to_largest_compatible_parallel_moves(
        self, partial_move_pair: PartialMovePair
    ) -> ParallelMoves:
        partial_move = partial_move_pair[0]
        row_parity = partial_move_parity(partial_move.src_loc)
        ud_direction = partial_move_direction(partial_move.src_loc)
        compatible_parallel_moves = [
            self.disjoint_groups[
                index_from_parity_and_direction(
                    row_parity, col_parity, ud_direction, lr_direction
                )
            ]
            for col_parity in (0, 1)
            for lr_direction in (0, 1)
        ]
        compatible_parallel_moves.sort(
            key=lambda pm: pm.largest_parallel_candidate_len(), reverse=True
        )
        for pm in compatible_parallel_moves:
            if move1 := self.get_compatible_pending_move_for_pair(pm, partial_move):
                # Push the move corresponding to the first qubit of the CZ pair.
                pm.push(move1)

                # Build the move corresponding to the second qubit of the CZ pair.
                dest2 = (move1[2][0], move1[2][1] + 1)
                move2 = (
                    partial_move_pair[1].qubit_id_ptr,
                    partial_move_pair[1].src_loc,
                    dest2,
                )

                self.disjoint_groups[
                    index_from_parity_and_direction(
                        *move_parity(*move2[1:]), *move_direction(*move2[1:])
                    )
                ].push(move2)

                # print(f"pushed move: {move}")
                return pm
        print(self.available_iz_locations, len(self.pending_partial_moves))
        raise Exception("not enough IZ space to schedule all moves")

    def get_compatible_pending_move(
        self, parallel_moves: ParallelMoves, partial_move: Optional[PartialMove] = None
    ) -> Optional[Move]:
        if partial_move:
            id = partial_move.qubit_id_ptr
            source = partial_move.src_loc
            for destination in self.available_iz_locations:
                if parallel_moves.parity == move_parity(
                    source, destination
                ) and parallel_moves.direction == move_direction(source, destination):
                    del self.available_iz_locations[destination]
                    return (id, source, destination)
        else:
            raise NotImplementedError
            # for partial_move in self.pending_partial_moves:
            #     id, source = partial_move
            #     for destination in self.available_iz_locations:
            #         if parallel_moves.parity == move_parity(
            #             source, destination
            #         ) and parallel_moves.direction == move_direction(
            #             source, destination
            #         ):
            #             del self.available_iz_locations[destination]
            #             self.pending_partial_moves.remove(partial_move)
            #             return (id, source, destination)

    def get_compatible_pending_move_for_pair(
        self, parallel_moves: ParallelMoves, partial_move: Optional[PartialMove] = None
    ) -> Optional[Move]:
        if partial_move:
            id = partial_move.qubit_id_ptr
            source = partial_move.src_loc
            for destination in self.available_iz_locations:
                if (
                    destination[1] % 2 == 0
                    and parallel_moves.parity == move_parity(source, destination)
                    and parallel_moves.direction == move_direction(source, destination)
                ):
                    del self.available_iz_locations[destination]
                    return (id, source, destination)

    def __iter__(self):
        return self

    def __next__(self) -> list[Move]:
        # If there are no moves left to schedule, stop the iteration.
        if self.is_empty():
            raise StopIteration

        # Should I step through the pending moves and try to push them
        # in the best parallel candidate? Or step through the largest
        # parallel candidates and try to fetch pending moves for them?

        # Stepping through the pending moves looks like this.
        # Stats: 15336 steps AND 30s
        # Before: ~16k steps and 22s
        while partial_move := self.next_pending_partial_move():
            if isinstance(partial_move, PartialMove):
                pm = self.push_to_largest_compatible_parallel_moves(partial_move)
            else:
                pm = self.push_pair_to_largest_compatible_parallel_moves(partial_move)
            if pm.largest_parallel_candidate_len() >= 36:
                return self.try_take(36, pm)

        # On the other hand, steping through the
        # largest_parallel_candidates looks like this.
        # Stats: 27016 steps AND 11m 33s
        # while not self.pending_partial_moves_is_empty():
        #     for pm in self.sorted_parallel_moves():
        #         pc = pm.largest_parallel_candidate()
        #         if pc and len(pc) >= 36:
        #             return self.try_take(36, pm)
        #         if move := self.get_compatible_pending_move(pm):
        #             pm.push(move)
        #             break

        # Once pending moves are exhausted, we try_get(36)
        # from the largest parallel candidate.
        return self.try_take(36)


class Schedule(QirModuleVisitor):
    """
    Schedule instructions within a block, adding appropriate moves to the interaction zone to perform operations
    """

    begin_func: Function
    end_func: Function
    move_funcs: list[Function]

    def __init__(self, device: Device):
        super().__init__()
        self.device = device
        self.num_qubits = len(self.device.home_locs)
        self.pending_moves_back: list[list[Move]] = []

    def _on_module(self, module):
        i64_ty = IntType(module.context, 64)
        # Find or create the necessary runtime functions.
        for func in module.functions:
            if func.name == "__quantum__rt__begin_parallel":
                self.begin_func = func
            elif func.name == "__quantum__rt__end_parallel":
                self.end_func = func
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
        self.move_funcs = [
            Function(
                FunctionType(
                    Type.void(module.context),
                    [qubit_type(module.context), i64_ty, i64_ty],
                ),
                Linkage.EXTERNAL,
                "__quantum__qis__move1__body",
                module,
            ),
            Function(
                FunctionType(
                    Type.void(module.context),
                    [qubit_type(module.context), i64_ty, i64_ty],
                ),
                Linkage.EXTERNAL,
                "__quantum__qis__move2__body",
                module,
            ),
            Function(
                FunctionType(
                    Type.void(module.context),
                    [qubit_type(module.context), i64_ty, i64_ty],
                ),
                Linkage.EXTERNAL,
                "__quantum__qis__move3__body",
                module,
            ),
            Function(
                FunctionType(
                    Type.void(module.context),
                    [qubit_type(module.context), i64_ty, i64_ty],
                ),
                Linkage.EXTERNAL,
                "__quantum__qis__move4__body",
                module,
            ),
        ]
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
        self.pending_moves: list[QubitId | tuple[QubitId, QubitId]] = []

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
                    (
                        gate["qubit_args"][0] == qubit_id(q)
                        if isinstance(q, QubitId)
                        else (
                            gate["qubit_args"][0] == qubit_id(q[0])
                            or gate["qubit_args"][0] == qubit_id(q[1])
                        )
                    )
                    for q in self.pending_moves
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

                # Prefer using matching relative column ordering to home locations to reduce move crossings.
                if (
                    self.device.get_home_loc(gate["qubit_args"][0])[1]
                    > self.device.get_home_loc(gate["qubit_args"][1])[1]
                ):
                    self.pending_moves.append((instr.args[1], instr.args[0]))
                else:
                    self.pending_moves.append((instr.args[0], instr.args[1]))

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
                self.pending_moves.append(instr.args[0])
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
                        self.pending_moves.append(qubit)
                self.insert_moves()
                for target_qubits in target_qubits_by_row:
                    self.flush_single_qubit_ops(target_qubits)
                self.insert_moves_back()
                self.pending_moves = []
            return

    def parallelize_pending_moves(self) -> Iterable[list[Move]]:
        qubits_to_move = self.pending_moves
        return MoveScheduler(qubits_to_move, self.device)

    def insert_moves(self):
        """
        For each pending move, insert a call to the move function that moves the
        given qubit to the given (row, col) location.
        """

        move_set_id = 0
        for parallel_set in self.parallelize_pending_moves():
            # Schedule the same moves back, so that we don't have to
            # recompute the parallel moves when moving the qubits
            # back to their home location.
            self.pending_moves_back.append(parallel_set)

            # We can execute 4 movement sets in parallel, if
            # this is the first one, start a parallel section.
            if move_set_id == 0:
                self.builder.call(self.begin_func, [])

            # Move all the moves in a parallel_set using the same
            # move function.
            for id, _, loc in parallel_set:
                self.builder.call(self.move_funcs[move_set_id], [id, loc[0], loc[1]])

            # There 4 move sets, so we increment the id modulo 4.
            move_set_id = (move_set_id + 1) % 4

            # We can execute 4 movement sets in parallel, if
            # this is the fourth one, end the parallel section.
            if move_set_id == 0:
                self.builder.call(self.end_func, [])

        # End the parallel section if it hasn't been ended.
        if move_set_id != 0:
            self.builder.call(self.end_func, [])

    def insert_moves_back(self):
        move_set_id = 0
        for parallel_set in self.pending_moves_back:
            # We can execute 4 movement sets in parallel, if
            # this is the first one, start a parallel section.
            if move_set_id == 0:
                self.builder.call(self.begin_func, [])

            # Move all the moves in a parallel_set using the same
            # move function.
            for id, home_loc, _ in parallel_set:
                self.builder.call(
                    self.move_funcs[move_set_id], [id, home_loc[0], home_loc[1]]
                )

            # There 4 move sets, so we increment the id modulo 4.
            move_set_id = (move_set_id + 1) % 4

            # We can execute 4 movement sets in parallel, if
            # this is the fourth one, end the parallel section.
            if move_set_id == 0:
                self.builder.call(self.end_func, [])

        # End the parallel section if it hasn't been ended.
        if move_set_id != 0:
            self.builder.call(self.end_func, [])

        # Clear pending moves back.
        self.pending_moves_back = []

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
