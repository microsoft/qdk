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
from itertools import islice, chain
from typing import Iterable, TypeAlias, Optional
from fractions import Fraction
from functools import lru_cache

QubitId: TypeAlias = Value
Location: TypeAlias = tuple[int, int]
MoveGroupScaleFactor: TypeAlias = tuple[bool | Fraction, bool | Fraction]


@dataclass
class Move:
    qubit_id_ptr: Value
    src_loc: Location
    dst_loc: Location

    def __hash__(self):
        return hash(self.qubit_id_ptr)

    def parity(self):
        return move_parity(self.src_loc, self.dst_loc)

    def direction(self):
        return move_direction(self.src_loc, self.dst_loc)


@dataclass
class PartialMove:
    """A move missing its destination location."""

    qubit_id_ptr: Value
    src_loc: Location

    def into_move(self, dst_loc: Location) -> Move:
        return Move(self.qubit_id_ptr, self.src_loc, dst_loc)


PartialMovePair: TypeAlias = tuple[PartialMove, PartialMove]


def move_parity(source: Location, destination: Location) -> tuple[int, int]:
    """Returns a tuple representing the parities of the source and destination columns of a move."""
    return (source[1] % 2, destination[1] % 2)


def move_direction(source: Location, destination: Location) -> tuple[int, int]:
    """Returns a tuple representing if the move is up or down, and left or right."""
    return (int(source[0] < destination[0]), int(source[1] < destination[1]))


def index_from_parity_and_direction(
    row_parity: int, col_parity, ud: int, lr: int
) -> int:
    major_index = 2 * row_parity + col_parity
    minor_index = 2 * ud + lr
    return 4 * major_index + minor_index


def is_invalid_move_pair(move1: Move, move2: Move) -> bool:
    """
    Returns true if the two moves are incompatible, i.e., if they have the same
    source row then they must have the same destination row, and if they have the
    same source column then they must have the same destination column.
    """

    source_row_diff = move1.src_loc[0] - move2.src_loc[0]
    destination_row_diff = move1.dst_loc[0] - move2.dst_loc[0]
    source_col_diff = move1.src_loc[1] - move2.src_loc[1]
    destination_col_diff = move1.dst_loc[1] - move2.dst_loc[1]

    return (
        (source_row_diff == 0 and destination_row_diff != 0)
        or (source_row_diff != 0 and destination_row_diff == 0)
        or (source_col_diff == 0 and destination_col_diff != 0)
        or (source_col_diff != 0 and destination_col_diff == 0)
    )


@lru_cache(maxsize=1 << 14)
def move_scale_helper(source_diff, destination_diff):
    return True if destination_diff == 0 else Fraction(source_diff, destination_diff)


def scale_factor(move1: Move, move2: Move) -> MoveGroupScaleFactor:
    """
    Returns a tuple of two elements, representing the row displacement ratio and column
    displacement ratio between the moves.
    """

    source_row_diff = move1.src_loc[0] - move2.src_loc[0]
    destination_row_diff = move1.dst_loc[0] - move2.dst_loc[0]
    source_col_diff = move1.src_loc[1] - move2.src_loc[1]
    destination_col_diff = move1.dst_loc[1] - move2.dst_loc[1]

    return move_scale_helper(source_row_diff, destination_row_diff), move_scale_helper(
        source_col_diff, destination_col_diff
    )


class MoveGroupCandidate:
    """
    Represents a group of moves that can be done at the same time.
    It has three fields:
        moves[set]: A set of moves that can be performed in parallel.
        scale_factor[Optional[tuple[Fraction, Fraction]]]: A tuple of fractions
            representing the scale factors in the row and col axes between
            moves. `None`, if there is a single element in the move set.
        ref_move[Move]: A move used as a representative of the group, used
            to test compatibility of other moves with the group.
    """

    def __init__(self, moves: Iterable[Move]):
        self.moves = set(moves)
        self.scale_factor = scale_factor(*moves) if len(self.moves) > 1 else None
        self.ref_move = next(iter(moves))

    def __len__(self) -> int:
        return len(self.moves)

    def add(self, move: Move):
        # A move group with a single move doesn't have an associated move_scale.
        # Therefore, we cannot test if a move is compatible with it, which means
        # we cannot add moves to it.
        assert (
            self.scale_factor
        ), "cannot add to move group candidate with a single move"
        self.moves.add(move)

    def remove(self, move: Move):
        self.moves.remove(move)

    def discard(self, move: Move):
        self.moves.discard(move)


class MoveGroupPool:
    """
    A data structure that takes individual moves as input and organizes them
    into groups of moves that can be executed in parallel.

    It provides:
        - An `add(move)` method to add moves to the data structure.
        - A `try_take(n)` that draws a group of up to n moves from the data structure.
        - An `is_empty()` method to check if there are any moves left.
    """

    def __init__(self, parity: tuple[int, int], direction: tuple[int, int]):
        self.moves = set()
        self.move_group_candidates: dict[
            Optional[MoveGroupScaleFactor], list[MoveGroupCandidate]
        ] = {None: []}
        self.parity = parity
        self.direction = direction

    def move_group_candidates_iter(self) -> Iterable[MoveGroupCandidate]:
        return chain(*self.move_group_candidates.values())

    def is_empty(self) -> bool:
        return not any(s.moves for s in self.move_group_candidates_iter())

    def largest_move_group_candidate(self) -> Optional[MoveGroupCandidate]:
        try:
            return max(self.move_group_candidates_iter(), key=len)
        except ValueError:
            return None

    def largest_move_group_candidate_len(self) -> int:
        try:
            return len(max(self.move_group_candidates_iter(), key=len).moves)
        except ValueError:
            return 0

    def add(self, move: Move):
        move_added = False
        for move2 in self.moves:
            pair = (move, move2)
            if is_invalid_move_pair(pair[0], pair[1]):
                continue
            s = scale_factor(pair[0], pair[1])
            candidates_with_same_move_scale = self.move_group_candidates.get(s, [])
            for group in candidates_with_same_move_scale:
                if pair[0] in group.moves:
                    group.moves.add(pair[1])
                    move_added = True
                    break
                elif pair[1] in group.moves:
                    group.moves.add(pair[0])
                    move_added = True
                    break
                elif s == scale_factor(pair[0], group.ref_move):
                    group.moves.add(pair[0])
                    group.moves.add(pair[1])
                    move_added = True
                    break
            else:
                candidates_with_same_move_scale.append(MoveGroupCandidate(pair))
                self.move_group_candidates[s] = candidates_with_same_move_scale
                move_added = True

        # This case triggers if `move` is not compatible with any move in `self.moves`.
        if not move_added:
            self.move_group_candidates[None].append(MoveGroupCandidate([move]))

        self.moves.add(move)

    def try_take(self, number_of_moves: int) -> list[Move]:
        """Take up to `number_of_moves` from the largest move group candidate."""

        if largest_move_group_candidate := self.largest_move_group_candidate():
            moves = list(islice(largest_move_group_candidate.moves, number_of_moves))
            moves_set = set(moves)
            self.moves -= moves_set
            # Remove the taken moves from all candidates.
            for group in self.move_group_candidates_iter():
                group.moves -= moves_set
            return moves
        else:
            return []


class MoveScheduler:

    def __init__(
        self, device: Device, qubits_to_move: list[QubitId | tuple[QubitId, QubitId]]
    ):
        self.device = device
        self.available_iz_locations = self.build_iz_locations()
        self.partial_moves = self.qubits_to_partial_moves(qubits_to_move)
        self.disjoint_pools: list[MoveGroupPool] = [
            MoveGroupPool((row_parity, col_parity), (ud, lr))
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

    def partial_moves_is_empty(self):
        return not bool(self.partial_moves)

    def next_partial_move(
        self,
    ) -> Optional[PartialMove | PartialMovePair]:
        try:
            return self.partial_moves.pop(0)
        except IndexError:
            return None

    def is_empty(self):
        """
        Returns `True` if all moves were scheduled.
        That is, there are no partial moves and all disjoint pools are empty.
        """
        return self.partial_moves_is_empty() and all(
            s.is_empty() for s in self.disjoint_pools
        )

    def largest_move_group_pool(self) -> MoveGroupPool:
        return max(
            self.disjoint_pools, key=lambda x: x.largest_move_group_candidate_len()
        )

    def add_to_largest_compatible_move_group(
        self, partial_move: PartialMove
    ) -> MoveGroupPool:
        interaction_zone = self.device.get_interaction_zones()[0]
        interaction_zone_row_offset = (
            interaction_zone.offset // self.device.column_count
        )
        src_col_parity = partial_move.src_loc[1] % 2
        ud_direction = int(partial_move.src_loc[0] < interaction_zone_row_offset)
        compatible_move_group_pools = [
            self.disjoint_pools[
                index_from_parity_and_direction(
                    src_col_parity, dst_col_parity, ud_direction, lr_direction
                )
            ]
            for dst_col_parity in (0, 1)
            for lr_direction in (0, 1)
        ]
        compatible_move_group_pools.sort(
            key=lambda pool: pool.largest_move_group_candidate_len(), reverse=True
        )
        for pool in compatible_move_group_pools:
            if move := self.get_compatible_move(pool, partial_move):
                pool.add(move)
                return pool
        raise Exception("not enough IZ space to schedule all moves")

    def add_pair_to_largest_compatible_move_group(
        self, partial_move_pair: PartialMovePair
    ) -> MoveGroupPool:
        interaction_zone = self.device.get_interaction_zones()[0]
        interaction_zone_row_offset = (
            interaction_zone.offset // self.device.column_count
        )
        partial_move = partial_move_pair[0]
        src_col_parity = partial_move.src_loc[1] % 2
        ud_direction = int(partial_move.src_loc[0] < interaction_zone_row_offset)
        compatible_move_group_pools = [
            self.disjoint_pools[
                index_from_parity_and_direction(
                    src_col_parity, dst_col_parity, ud_direction, lr_direction
                )
            ]
            for dst_col_parity in (0, 1)
            for lr_direction in (0, 1)
        ]
        compatible_move_group_pools.sort(
            key=lambda pool: pool.largest_move_group_candidate_len(), reverse=True
        )
        for pool in compatible_move_group_pools:
            if move1 := self.get_compatible_move(pool, partial_move, is_pair=True):
                # Push the move corresponding to the first qubit of the CZ pair.
                pool.add(move1)

                # Build the move corresponding to the second qubit of the CZ pair.
                dest2 = (move1.dst_loc[0], move1.dst_loc[1] + 1)
                move2 = partial_move_pair[1].into_move(dest2)
                self.disjoint_pools[
                    index_from_parity_and_direction(*move2.parity(), *move2.direction())
                ].add(move2)

                return pool
        raise Exception("not enough IZ space to schedule all moves")

    def get_destination(
        self,
        partial_move: PartialMove,
        scale_factor: MoveGroupScaleFactor,
        group: MoveGroupCandidate,
    ) -> Optional[Location]:
        """
        Returns an available destination location that would make `partial_move`
        fit in the given group, or `None` if no such location exists.
        """
        row_scale_factor, col_scale_factor = scale_factor

        if row_scale_factor is True:
            dst_row = group.ref_move.dst_loc[0]
        else:
            # We compute the destination row by solving this equation for `dst_row`:
            # src_row_diff / (group.ref_move.dst_loc[0] - dst_row) == row_scale_factor
            src_row_diff = group.ref_move.src_loc[0] - partial_move.src_loc[0]
            dst_row = group.ref_move.dst_loc[0] - src_row_diff / row_scale_factor
            assert isinstance(dst_row, Fraction)
            if dst_row.denominator == 1:
                dst_row = dst_row.numerator
            else:
                return None

        if col_scale_factor is True:
            dst_col = group.ref_move.dst_loc[1]
        else:
            # We compute the destination col by solving this equation for `dst_col`:
            # src_col_diff / (group.ref_move.dst_loc[1] - dst_col) == col_scale_factor
            src_col_diff = group.ref_move.src_loc[1] - partial_move.src_loc[1]
            dst_col = group.ref_move.dst_loc[1] - src_col_diff / col_scale_factor
            assert isinstance(dst_col, Fraction)
            if dst_col.denominator == 1:
                dst_col = dst_col.numerator
            else:
                return None

        loc = (dst_row, dst_col)
        if loc in self.available_iz_locations:
            return loc

    def get_compatible_move(
        self,
        pool: MoveGroupPool,
        partial_move: PartialMove,
        is_pair=False,
    ) -> Optional[Move]:
        source = partial_move.src_loc

        # First, try finding a large enough group to place the partial movement in.
        GROUP_SIZE_THRESHOLD = self.device.column_count // 4
        best_destination: Optional[Location] = None
        best_destination_group_len = 0
        for scale, groups in pool.move_group_candidates.items():
            if not scale:
                continue
            for group in sorted(groups, key=len, reverse=True):
                if (
                    len(group) < GROUP_SIZE_THRESHOLD
                    or len(group) < best_destination_group_len
                ):
                    break
                if destination := self.get_destination(partial_move, scale, group):
                    if (not is_pair) or destination[1] % 2 == 0:
                        best_destination = destination
                        best_destination_group_len = len(group)
                        break
        if best_destination:
            del self.available_iz_locations[best_destination]
            return partial_move.into_move(best_destination)

        # If we didn't find a group to place the partial_move in,
        # just pick the next available IZ location.
        for destination in self.available_iz_locations:
            if (
                ((not is_pair) or destination[1] % 2 == 0)
                and pool.parity == move_parity(source, destination)
                and pool.direction == move_direction(source, destination)
            ):
                del self.available_iz_locations[destination]
                return partial_move.into_move(destination)

    def __iter__(self):
        return self

    def __next__(self) -> list[Move]:
        # If there are no moves left to schedule, stop the iteration.
        if self.is_empty():
            raise StopIteration

        # Step through the partial moves and push them to the largest
        # candidate they are compatible with.
        while partial_move := self.next_partial_move():
            if isinstance(partial_move, PartialMove):
                pool = self.add_to_largest_compatible_move_group(partial_move)
            else:
                pool = self.add_pair_to_largest_compatible_move_group(partial_move)
            if pool.largest_move_group_candidate_len() >= self.device.column_count:
                return pool.try_take(self.device.column_count)

        # Once partial moves are exhausted, we try_get from the largest candidate.
        return self.largest_move_group_pool().try_take(self.device.column_count)


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

    def schedule_pending_moves(self) -> Iterable[list[Move]]:
        return MoveScheduler(self.device, self.pending_moves)

    def insert_moves(self):
        """
        For each pending move, insert a call to the move function that moves the
        given qubit to the given (row, col) location.
        """

        move_group_id = 0
        for move_group in self.schedule_pending_moves():
            # Schedule the same moves back, so that we don't have to
            # recompute the parallel moves when moving the qubits
            # back to their home location.
            self.pending_moves_back.append(move_group)

            # We can execute 4 move groups in parallel, if
            # this is the first one, start a parallel section.
            if move_group_id == 0:
                self.builder.call(self.begin_func, [])

            # Insert all the moves in a group using the same move function.
            for move in move_group:
                self.builder.call(
                    self.move_funcs[move_group_id], (move.qubit_id_ptr, *move.dst_loc)
                )

            # There 4 move groups, so we increment the id modulo 4.
            move_group_id = (move_group_id + 1) % 4

            # We can execute 4 move groups in parallel, if
            # this is the fourth one, end the parallel section.
            if move_group_id == 0:
                self.builder.call(self.end_func, [])

        # End the parallel section if it hasn't been ended.
        if move_group_id != 0:
            self.builder.call(self.end_func, [])

    def insert_moves_back(self):
        move_group_id = 0
        for move_group in self.pending_moves_back:
            # We can execute 4 move groups in parallel, if
            # this is the first one, start a parallel section.
            if move_group_id == 0:
                self.builder.call(self.begin_func, [])

            # Insert all the moves in a group using the same move function.
            for move in move_group:
                self.builder.call(
                    self.move_funcs[move_group_id], (move.qubit_id_ptr, *move.src_loc)
                )

            # There 4 move groups, so we increment the id modulo 4.
            move_group_id = (move_group_id + 1) % 4

            # We can execute 4 move groups in parallel, if
            # this is the fourth one, end the parallel section.
            if move_group_id == 0:
                self.builder.call(self.end_func, [])

        # End the parallel section if it hasn't been ended.
        if move_group_id != 0:
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
