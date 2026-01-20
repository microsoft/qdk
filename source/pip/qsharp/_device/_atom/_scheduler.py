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
from .._device import Device, Zone, ZoneType
from collections import defaultdict
from dataclasses import dataclass
from itertools import chain
from typing import Iterable, TypeAlias, Optional
from fractions import Fraction
from functools import lru_cache

QubitId: TypeAlias = Value
Location: TypeAlias = tuple[int, int]
MoveGroupScaleFactor: TypeAlias = tuple[bool | Fraction, bool | Fraction]
MOVE_GROUPS_PER_PARALLEL_SECTION = 1


@dataclass
class Move:
    __slots__ = ("qubit_id_ptr", "src_loc", "dst_loc")

    qubit_id_ptr: Value
    src_loc: Location
    dst_loc: Location

    def __hash__(self):
        return hash(self.qubit_id_ptr)

    def __str__(self):
        return f"Move Qubit({self.qubit_id}): {self.src_loc} -> {self.dst_loc}"

    def __repr__(self):
        return self.__str__()

    @property
    def qubit_id(self) -> int:
        q_id = qubit_id(self.qubit_id_ptr)
        assert q_id is not None, "Qubit id should be known"
        return q_id

    def parity(self):
        return move_parity(self.src_loc, self.dst_loc)

    def direction(self):
        return move_direction(self.src_loc, self.dst_loc)


@dataclass
class PartialMove:
    """A move missing its destination location."""

    __slots__ = ("qubit_id_ptr", "src_loc")

    qubit_id_ptr: Value
    src_loc: Location

    @property
    def qubit_id(self) -> int:
        q_id = qubit_id(self.qubit_id_ptr)
        assert q_id is not None, "Qubit id should be known"
        return q_id

    def into_move(self, dst_loc: Location) -> Move:
        return Move(self.qubit_id_ptr, self.src_loc, dst_loc)


PartialMovePair: TypeAlias = tuple[PartialMove, PartialMove]


def move_parity(source: Location, destination: Location) -> tuple[int, int]:
    """Returns a tuple representing the parities of the source and destination columns of a move."""
    return (source[1] % 2, destination[1] % 2)


def move_direction(source: Location, destination: Location) -> tuple[int, int]:
    """Returns a tuple representing if the move is up or down, and left or right."""
    return (int(source[0] < destination[0]), int(source[1] < destination[1]))


def index_from_parity_and_direction(ud: int, lr: int) -> int:
    return 2 * ud + lr


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
def scale_factor_helper(source_diff, destination_diff):
    if destination_diff == 0:
        return True
    if (s := Fraction(source_diff, destination_diff)) >= 0:
        return s


def scale_factor(move1: Move, move2: Move) -> Optional[MoveGroupScaleFactor]:
    """
    Returns a tuple of two elements, representing the row displacement ratio and column
    displacement ratio between the moves.
    """

    if is_invalid_move_pair(move1, move2):
        return None

    source_row_diff = move1.src_loc[0] - move2.src_loc[0]
    destination_row_diff = move1.dst_loc[0] - move2.dst_loc[0]
    source_col_diff = move1.src_loc[1] - move2.src_loc[1]
    destination_col_diff = move1.dst_loc[1] - move2.dst_loc[1]
    row_scale_factor = scale_factor_helper(source_row_diff, destination_row_diff)
    col_scale_factor = scale_factor_helper(source_col_diff, destination_col_diff)

    if row_scale_factor is not None and col_scale_factor is not None:
        return row_scale_factor, col_scale_factor


class MoveGroup:
    """
    Represents a group of moves that can be done at the same time.

    Attributes:
        moves (set): A set of moves that can be performed in parallel.
        scale_factor (Optional[tuple[Fraction, Fraction]]): A tuple of fractions
            representing the scale factors in the row and col axes between
            moves. `None`, if there is a single element in the move set.
        ref_move (Move): A move used as a representative of the group, used
            to test compatibility of other moves with the group.
    """

    __slots__ = ("moves", "scale_factor", "ref_move")

    def __init__(self, moves: Iterable[Move]):
        self.moves = set(moves)
        self.scale_factor = scale_factor(*moves) if len(self.moves) > 1 else None
        self.ref_move = next(iter(moves))

    def __len__(self) -> int:
        return len(self.moves)

    def add(self, move: Move):
        """
        Adds a move to this move group.

        Args:
            move (Move): The move to add.
        """

        # A move group with a single move doesn't have an associated scale factor.
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
    """A data structure that takes individual moves as input and organizes them
    into groups of moves that can be executed in parallel.

    Attributes:
        moves: A set containing all the moves in the move-group pool.
        move_group_candidates: A dict organizing the move-group candidates
            by scale factor.
        parity: The parity of source and destination columns of all the moves
            in this pool.
        direction: The up/down and left/right direction of all the moves
            in this pool.
    """

    def __init__(self):
        """Initializes a move-group pool for moves of the given `parity` and `direction`.
        Args:
            parity: The parity of source and destination columns of all the moves
                in this pool.
            direction: The up/down and left/right direction of all the moves
                in this pool.
        """
        self.moves: Optional[list[Move]] = []
        self.move_group_candidates: dict[MoveGroupScaleFactor, list[MoveGroup]] = (
            defaultdict(list)
        )
        self.single_moves: set[Move] | list[Move] = set()

    def move_group_candidates_iter(self) -> Iterable[MoveGroup]:
        return chain(*self.move_group_candidates.values())

    def is_empty(self) -> bool:
        """Returns `True` if there are no moves left, `False` otherwise."""
        return (
            not any(s.moves for s in self.move_group_candidates_iter())
            and not self.single_moves
        )

    def largest_move_group_candidate(self) -> Optional[MoveGroup]:
        try:
            return max(self.move_group_candidates_iter(), key=len)
        except ValueError:
            return None

    def add(self, move: Move):
        """Adds a move to the move-group pool.
        Args:
            move: The move to add. It must be of the same parity and direction as
                the rest of the moves in this pool.
        """
        assert self.moves is not None

        move_added = False

        # Add the move to all the groups it is compatible with
        for group_scale_factor, groups in self.move_group_candidates.items():
            for group in groups:
                if scale_factor(move, group.ref_move) == group_scale_factor:
                    group.add(move)
                    move_added = True

        # Build a table organizing the moves by scale factor with respect to `move`.
        moves_by_scale: dict[MoveGroupScaleFactor, list[Move]] = defaultdict(list)
        for move2 in self.moves:
            s = scale_factor(move, move2)
            if s is None:
                continue
            moves_by_scale[s].append(move2)

        # Try to create new candidates having the new move as the ref_move.
        for s, moves in moves_by_scale.items():
            candidates_with_same_scale_factor = self.move_group_candidates[s]
            for move2 in moves:
                for group in candidates_with_same_scale_factor:
                    if move2 in group.moves:
                        # This pair already belongs to an existing move group candidate,
                        # so we don't need to create a new one.
                        break
                else:
                    # Create a new move group candidate.
                    new_candidate = MoveGroup((move, move2))

                    # Add previous moves to the new candidate.
                    new_candidate.moves.update(moves_by_scale[s])

                    candidates_with_same_scale_factor.append(new_candidate)
                    move_added = True

        # This case triggers if `move` is not compatible with any move in `self.moves`.
        if not move_added:
            assert isinstance(self.single_moves, set)
            self.single_moves.add(move)

        self.moves.append(move)

    def try_take(self, number_of_moves: int) -> list[Move]:
        """Take up to `number_of_moves` from the largest move group candidate.
        Args:
            number_of_moves: The number of moves to take from this pool.
        """
        # Once we start taking moves from the MoveGroupPool, we don't need to add
        # new moves. So we set `self.moves` to `None` as a safety measure.
        if self.moves is not None:
            self.moves = None

        if largest_move_group_candidate := self.largest_move_group_candidate():
            # Ensure moves are sorted by qubit ID to have a deterministic order.
            moves = sorted(
                largest_move_group_candidate.moves, key=lambda m: m.qubit_id
            )[:number_of_moves]
            moves_set = set(moves)
            # Remove the taken moves from all candidates.
            for group in self.move_group_candidates_iter():
                group.moves -= moves_set
            assert isinstance(self.single_moves, set)
            self.single_moves -= moves_set
            return moves
        else:
            if isinstance(self.single_moves, set):
                self.single_moves = sorted(
                    self.single_moves, key=lambda m: m.qubit_id, reverse=True
                )
            if m := self.single_moves.pop():
                return [m]
            else:
                return []

    def take_largest_candidate(self) -> list[Move]:
        """Take all the moves from the largest move group candidate."""
        # Once we start taking moves from the MoveGroupPool, we don't need to add
        # new moves. So we set `self.moves` to `None` as a safety measure.
        if self.moves is not None:
            self.moves = None

        if largest_move_group_candidate := self.largest_move_group_candidate():
            # Ensure moves are sorted by qubit ID to have a deterministic order.
            moves = sorted(largest_move_group_candidate.moves, key=lambda m: m.qubit_id)
            moves_set = largest_move_group_candidate.moves
            # Remove the taken moves from all candidates.
            for group in self.move_group_candidates_iter():
                if group is not largest_move_group_candidate:
                    group.moves -= moves_set
            assert isinstance(self.single_moves, set)
            self.single_moves -= moves_set
            moves_set.clear()
            return moves
        else:
            if isinstance(self.single_moves, set):
                self.single_moves = sorted(
                    self.single_moves, key=lambda m: m.qubit_id, reverse=True
                )
            if m := self.single_moves.pop():
                return [m]
            else:
                return []


class MoveScheduler:
    """
    Takes a device, a target zone, and a list of qubits to move to that
    target zone and builds an iterator that returns groups of moves
    that can be executed in parallel.

    Attributes:
        device: An object containing information about the device.
        zone: The zone the moves will be scheduled to.
        available_dst_locations: The available destinations in the `zone`.
        partial_moves: The moves that haven't been assigned a destination location.
        disjoint_pools: A list containing one pool of move-groups for each parity and direction.
    """

    def __init__(
        self,
        device: Device,
        zone: Zone,
        qubits_to_move: list[QubitId | tuple[QubitId, QubitId]],
    ):
        """Initializes the move scheduler from a device, a target zone,
        and a list of qubits to move to that target zone.
        Args:
            device: An object containing information about the device.
            zone: The zone the moves will be scheduled to.
            qubits_to_move: A list of qubits to move.
        """
        self.device = device
        self.zone = zone
        self.available_dst_locations = self.build_zone_locations(zone)
        self.move_group_pool = MoveGroupPool()

        # Step through the partial moves and push them to the largest
        # candidate they are compatible with.
        partial_moves = self.qubits_to_partial_moves(qubits_to_move)
        for partial_move in partial_moves:
            if isinstance(partial_move, PartialMove):
                self.add_to_largest_compatible_move_group(partial_move)
            else:
                self.add_pair_to_largest_compatible_move_group(partial_move)

    def build_zone_locations(self, zone: Zone) -> dict[Location, None]:
        zone_row_offset = zone.offset // self.device.column_count
        # We use a dict with None values instead of a set to preserve order.
        return {
            (row, col): None
            for row in range(
                zone_row_offset,
                zone_row_offset + zone.row_count,
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
                return self.device.get_ordering(partial_move.qubit_id)
            else:
                return self.device.get_ordering(partial_move[0].qubit_id)

        return sorted(partial_moves, key=sort_key)

    def is_empty(self):
        """
        Returns `True` if all moves were scheduled.
        That is, there are no partial moves and all disjoint pools are empty.
        """
        return self.move_group_pool.is_empty()

    def largest_move_group_pool(self) -> MoveGroupPool:
        return self.move_group_pool

    def add_to_largest_compatible_move_group(
        self, partial_move: PartialMove
    ) -> MoveGroupPool:
        zone_row_offset = self.zone.offset // self.device.column_count

        # Heuristic: Prefer moves that are straight up or down.
        for row in range(zone_row_offset, zone_row_offset + self.zone.row_count):
            dst_loc = (row, partial_move.src_loc[1])
            if dst_loc in self.available_dst_locations:
                move = partial_move.into_move(dst_loc)
                pool = self.move_group_pool
                pool.add(move)
                del self.available_dst_locations[move.dst_loc]
                return pool

        if move := self.get_compatible_move(self.move_group_pool, partial_move):
            self.move_group_pool.add(move)
            del self.available_dst_locations[move.dst_loc]
            return self.move_group_pool

        raise Exception("not enough IZ space to schedule all moves")

    def add_pair_to_largest_compatible_move_group(
        self, partial_move_pair: PartialMovePair
    ) -> MoveGroupPool:
        zone_row_offset = self.zone.offset // self.device.column_count
        partial_move = partial_move_pair[0]

        # Heuristic: Prefer moves that are straight up or down.
        if partial_move.src_loc[1] % 2 == 0:
            for row in range(zone_row_offset, zone_row_offset + self.zone.row_count):
                dst_loc1 = (row, partial_move.src_loc[1])
                dst_loc2 = (row, partial_move.src_loc[1] + 1)
                if (
                    dst_loc1 in self.available_dst_locations
                    and dst_loc2 in self.available_dst_locations
                ):
                    move1 = partial_move.into_move(dst_loc1)
                    move2 = partial_move_pair[1].into_move(dst_loc2)
                    pool1 = self.move_group_pool
                    pool2 = self.move_group_pool
                    pool1.add(move1)
                    pool2.add(move2)
                    del self.available_dst_locations[dst_loc1]
                    del self.available_dst_locations[dst_loc2]
                    return pool1

        if move1 := self.get_compatible_move(
            self.move_group_pool, partial_move, is_pair=True
        ):
            # Push the move corresponding to the first qubit of the CZ pair.
            self.move_group_pool.add(move1)

            # Build the move corresponding to the second qubit of the CZ pair.
            dest2 = (move1.dst_loc[0], move1.dst_loc[1] + 1)
            move2 = partial_move_pair[1].into_move(dest2)
            self.move_group_pool.add(move2)
            del self.available_dst_locations[move1.dst_loc]
            del self.available_dst_locations[move2.dst_loc]
            return self.move_group_pool
        raise Exception("not enough IZ space to schedule all moves")

    def get_destination(
        self,
        partial_move: PartialMove,
        scale_factor: MoveGroupScaleFactor,
        group: MoveGroup,
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
        if loc in self.available_dst_locations:
            return loc

    def get_compatible_move(
        self,
        pool: MoveGroupPool,
        partial_move: PartialMove,
        is_pair=False,
    ) -> Optional[Move]:
        # First, try finding a large enough group to place the partial move in.
        if self.zone.type != ZoneType.MEAS:
            GROUP_SIZE_THRESHOLD = self.device.column_count // 4
            best_destination: Optional[Location] = None
            best_destination_group_len = 0
            for scale, groups in pool.move_group_candidates.items():
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
                return partial_move.into_move(best_destination)

        # If we didn't find a group to place the partial_move in,
        # just pick the next available IZ location.
        for destination in self.available_dst_locations:
            if (not is_pair) or destination[1] % 2 == 0:
                return partial_move.into_move(destination)

    def __iter__(self):
        return self

    def __next__(self) -> list[Move]:
        # If there are no moves left to schedule, stop the iteration.
        if self.is_empty():
            raise StopIteration

        # Try_get from the largest candidate.
        return self.largest_move_group_pool().take_largest_candidate()


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
        self.pending_moves: list[list[Move]] = []

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
        measurement_zone = self.device.get_measurement_zones()[0]
        max_iz_pairs = (self.device.column_count // 2) * interaction_zone.row_count
        max_measurements = self.device.column_count * measurement_zone.row_count

        # Track pending/queued single qubit operations by qubit id.
        self.single_qubit_ops = [[] for _ in range(self.num_qubits)]

        # Track pending CZ operations.
        self.curr_cz_ops = []

        # Track pending measurements.
        self.measurements = []

        # Track pending qubits to move to an interaction or measurement zone.
        self.pending_qubits_to_move: list[QubitId | tuple[QubitId, QubitId]] = []

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
                if any(
                    (
                        gate["qubit_args"][0] == qubit_id(q)
                        if isinstance(q, QubitId)
                        else (
                            gate["qubit_args"][0] == qubit_id(q[0])
                            or gate["qubit_args"][0] == qubit_id(q[1])
                        )
                    )
                    for q in self.pending_qubits_to_move
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
                    self.measurements
                    or uses_any_value(vals_used, self.vals_used_in_cz_ops)
                    or len(self.curr_cz_ops) >= max_iz_pairs
                ):
                    self.flush_pending(instr)
                instr.remove()
                self.curr_cz_ops.append(instr)
                self.vals_used_in_cz_ops.update(vals_used)

                # Prefer using matching relative column ordering to home locations to reduce move crossings.
                if (
                    self.device.get_home_loc(gate["qubit_args"][0])[1]
                    > self.device.get_home_loc(gate["qubit_args"][1])[1]
                ):
                    self.pending_qubits_to_move.append((instr.args[1], instr.args[0]))
                else:
                    self.pending_qubits_to_move.append((instr.args[0], instr.args[1]))

            elif gate != {} and len(gate["result_args"]) == 1:
                # This is a measurement; queue it up to be executed in the measurement zone.

                # Pick next available measurement zone location for this qubit. If none, flush the current set and start a fresh set.
                # Create move instructions to move qubit to measurement zone and save them in pending moves for later insertion.
                assert isinstance(instr, Call)
                (vals_used, _) = get_used_values(instr)
                if (
                    not self.measurements
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
                    temp_moves = self.pending_qubits_to_move
                    self.pending_qubits_to_move = []
                    self.flush_pending(instr)
                    self.measurements = temp_meas
                    self.pending_qubits_to_move = temp_moves

                # Remove the measurement from the block and queue it.
                instr.remove()
                self.measurements.append((instr, gate))
                self.vals_used_in_measurements.update(vals_used)
                self.pending_qubits_to_move.append(instr.args[0])
            else:
                # This is not a gate or measurement; flush any pending operations and leave the instruction in place.
                # This uses a while loop to ensure all pending operations are flushed before the instruction.
                while self.any_pending_ops():
                    self.flush_pending(instr)

    def any_pending_single_qubit_ops(self):
        return any(ops for ops in self.single_qubit_ops)

    def any_pending_czs(self):
        return bool(self.curr_cz_ops)

    def any_pending_measurements(self):
        return bool(self.measurements)

    def any_pending_ops(self):
        return (
            self.any_pending_czs()
            or self.any_pending_single_qubit_ops()
            or self.any_pending_measurements()
        )

    def flush_pending(self, insert_before: Instruction):
        interaction_zone = self.device.get_interaction_zones()[0]
        self.builder.insert_before(insert_before)
        # If cz ops pending, insert accumulated moves, single qubits ops matching cz rows, then the cz ops, then move back.
        if self.curr_cz_ops:
            self.schedule_pending_moves(interaction_zone)
            self.insert_moves()
            qubits_by_row = self.target_qubits_by_row(interaction_zone)
            for qubits_in_row in qubits_by_row:
                self.flush_single_qubit_ops(qubits_in_row)
            self.builder.call(self.begin_func, [])
            for cz_op in self.curr_cz_ops:
                self.builder.instr(cz_op)
            self.builder.call(self.end_func, [])
            self.curr_cz_ops = []
            self.insert_moves_back()
            self.vals_used_in_cz_ops = set()
            return
        # If measurements pending, insert accumulated moves, then measurements, then move back.
        elif len(self.measurements) > 0:
            self.schedule_pending_moves(self.device.get_measurement_zones()[0])
            self.insert_moves()
            self.builder.call(self.begin_func, [])
            for meas_op, meas_gate in self.measurements:
                self.builder.instr(meas_op)
            self.builder.call(self.end_func, [])
            self.measurements = []
            self.vals_used_in_measurements = set()
            self.insert_moves_back()
            return
        # Else, create movements for remaining single qubit ops to the first interaction zone,
        # insert those moves, then the ops, then move back.
        else:
            while self.any_pending_single_qubit_ops():
                target_qubits_by_row = [[] for _ in range(interaction_zone.row_count)]
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
                for target_qubits in target_qubits_by_row:
                    for q in target_qubits:
                        qubit = self.single_qubit_ops[q][0][0].args[0]
                        if self.single_qubit_ops[q][0][1]["gate"] == "rz":
                            qubit = self.single_qubit_ops[q][0][0].args[1]
                        self.pending_qubits_to_move.append(qubit)
                self.schedule_pending_moves(interaction_zone)
                self.insert_moves()
                qubits_by_row = self.target_qubits_by_row(interaction_zone)
                for qubits_in_row in qubits_by_row:
                    self.flush_single_qubit_ops(qubits_in_row)
                self.insert_moves_back()
            return

    def target_qubits_by_row(self, zone: Zone) -> list[list[int]]:
        zone_row_offset = zone.offset // self.device.column_count
        qubits_by_row: list[list[int]] = [[] for _ in range(zone.row_count)]
        for group in self.pending_moves:
            for move in group:
                row_idx = move.dst_loc[0] - zone_row_offset
                qubits_by_row[row_idx].append(move.qubit_id)
        # Organize qubits in each row by qubit_id, so that parallel sections
        # of single-qubit ops in the generated QIR are easier to read.
        for row in qubits_by_row:
            row.sort()
        return qubits_by_row

    def schedule_pending_moves(self, zone: Zone):
        move_scheduler = MoveScheduler(self.device, zone, self.pending_qubits_to_move)
        for move_group in move_scheduler:
            self.pending_moves.append(move_group)
        # self.verify_that_all_moves_were_scheduled()
        self.pending_qubits_to_move = []

    def verify_that_all_moves_were_scheduled(self):
        moves_to_schedule = sum(
            len(x) if isinstance(x, tuple) else 1 for x in self.pending_qubits_to_move
        )
        scheduled_moves = sum(len(group) for group in self.pending_moves)
        assert (
            moves_to_schedule == scheduled_moves
        ), f"{moves_to_schedule} != {scheduled_moves}"

    def insert_moves(self):
        """
        For each pending move, insert a call to the move function that moves the
        given qubit to the given (row, col) location.
        """
        move_group_id = 0
        for move_group in self.pending_moves:
            # We can execute `MOVE_GROUPS_PER_PARALLEL_SECTION`, if
            # this is the first one, start a parallel section.
            if move_group_id == 0:
                self.builder.call(self.begin_func, [])

            # Insert all the moves in a group using the same move function.
            for move in move_group:
                self.builder.call(self.move_func, (move.qubit_id_ptr, *move.dst_loc))

            # There `MOVE_GROUPS_PER_PARALLEL_SECTION` move groups,
            # so we increment the id modulo `MOVE_GROUPS_PER_PARALLEL_SECTION`.
            move_group_id = (move_group_id + 1) % MOVE_GROUPS_PER_PARALLEL_SECTION

            # We can execute `MOVE_GROUPS_PER_PARALLEL_SECTION`, if
            # this is the last one, end the parallel section.
            if move_group_id == 0:
                self.builder.call(self.end_func, [])

        # End the parallel section if it hasn't been ended.
        if move_group_id != 0:
            self.builder.call(self.end_func, [])

    def insert_moves_back(self):
        move_group_id = 0
        for move_group in self.pending_moves:
            # We can execute `MOVE_GROUPS_PER_PARALLEL_SECTION`, if
            # this is the first one, start a parallel section.
            if move_group_id == 0:
                self.builder.call(self.begin_func, [])

            # Insert all the moves in a group using the same move function.
            for move in move_group:
                self.builder.call(self.move_func, (move.qubit_id_ptr, *move.src_loc))

            # There `MOVE_GROUPS_PER_PARALLEL_SECTION` move groups,
            # so we increment the id modulo `MOVE_GROUPS_PER_PARALLEL_SECTION`.
            move_group_id = (move_group_id + 1) % MOVE_GROUPS_PER_PARALLEL_SECTION

            # We can execute `MOVE_GROUPS_PER_PARALLEL_SECTION`, if
            # this is the last one, end the parallel section.
            if move_group_id == 0:
                self.builder.call(self.end_func, [])

        # End the parallel section if it hasn't been ended.
        if move_group_id != 0:
            self.builder.call(self.end_func, [])

        # Clear pending moves.
        self.pending_moves = []

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
