# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ..._native import GlobalCallable, Closure
from ...qsharp import run
from ..._types import QirInputData
from typing import (
    Tuple,
    Union,
    Optional,
    List,
    Union,
)
from pyqir import (
    Module,
    Context,
    Call,
    Function,
    QirModuleVisitor,
    ptr_id,
    required_num_qubits,
)


class MajoranaDevice:
    def __init__(self, rows: int, cols: int):
        self.layout = (rows, cols)

    def show_trace(self, qir: str | QirInputData, use_virtual: bool = False):
        """
        Visualize the execution trace of a QIR program on the MajoranaDevice device using the Majorana widget.
        This includes approximate layout and scheduling of the program to show the parallelism of operations.
        By default, this only supports native Majorana operations. The `use_virtual` parameter enables
        support for decomposing Clifford + T operations into native Majorana operations using the virtual layout.

        :param qir: The QIR program to visualize, either as a string or as QirInputData.
        :param use_virtual: Whether to use the virtual device Majorana layout, enabling support for
            decomposing Clifford + T operations into native Majorana measurement sequences.
        """
        try:
            from qsharp_widgets import Majorana  # type: ignore[import-not-found]
        except ImportError:
            raise ImportError(
                "The qsharp-widgets package is required for showing Majorana trace visualization. "
                "Please install it via 'pip install \"qdk[jupyter]\"' or 'pip install qsharp-widgets'."
            )
        from IPython.display import display  # type: ignore[import-not-found]

        module = Module.from_ir(Context(), str(qir))
        tracer = Trace()
        tracer.run(module)
        trace = tracer.trace
        num_qubits = tracer.num_qubits
        if use_virtual:
            assert (
                self.layout[0] % 2 == 0
            ), "The number of rows must be even for virtual qubit scheduling."
            assert (
                self.layout[0] // 2 * self.layout[1] >= num_qubits
            ), "The virtual layout does not have enough qubits for the QIR program."
            trace = schedule_virtual(self.layout, trace)
        else:
            assert (
                self.layout[0] * self.layout[1] >= num_qubits
            ), "The layout does not have enough qubits for the QIR program."
            trace = schedule_tetrons(
                self.layout,
                trace,
            )
        display(Majorana(self.layout, trace))


def schedule_tetrons(
    layout: Tuple[int, int],
    trace: Union[
        List[Tuple[str, List[int]]],
        List[Tuple[str, List[int], Optional[Tuple[str, List[int], int]]]],
    ],
) -> List[List[Tuple[str, List[int]]]]:
    _, num_cols = layout
    scheduled_trace = []
    for event in trace:
        if len(event) == 3:
            event = event[:2]

        event = convert(event)
        match event[0]:
            case "X" | "Y" | "Z":
                continue
        if event[0].endswith("-lw") and event[1][0] in tetron_bottom_row_ids(layout):
            event = (event[0].replace("-lw", "-up"), event[1])

        # Find the last step in the scheduled trace that includes this event's qubits,
        # and append this event into the step afterward. Otherwise, add it as a new step at the end.
        step_idx = len(scheduled_trace) - 1
        while step_idx >= 0 and not any(
            qubit in step_qubits
            for qubit in event[1]
            for _, step_qubits in scheduled_trace[step_idx]
        ):
            step_idx -= 1

        if step_idx < len(scheduled_trace) - 1:
            # Check that the event for this qubit found in that index is not the same as the current event.
            if (
                (
                    other_event := next(
                        (
                            (op, step_qubits)
                            for op, step_qubits in scheduled_trace[step_idx]
                            if any(qubit in event[1] for qubit in step_qubits)
                        ),
                        None,
                    )
                )
                and len(other_event[1]) == 1
                and (other_event[0].split("-")[0], other_event[1][0])
                == (event[0].split("-")[0], event[1][0])
            ):
                # The same qubit already has the same operation scheduled, so don't schedule it twice in a row.
                continue

            step_idx += 1
            if event[0].endswith("-lw"):
                # If the event is a lower wire operation, ensure it's bottom neighbor
                # doesn't have an upper wire operation scheduled in the same step.
                neighbor = event[1][0] + num_cols
                while any(
                    neighbor in step_qubits and op.endswith("-up")
                    for op, step_qubits in scheduled_trace[step_idx]
                ):
                    step_idx += 1
                    if step_idx >= len(scheduled_trace):
                        # Couldn't find a place for this lower wire operation, but check
                        # to see if there is a better spot for an upper wire operation on the same qubit.
                        if event[1][0] not in tetron_top_row_ids(layout):
                            neighbor = event[1][0] - num_cols
                            upper_step = step_idx
                            while any(
                                neighbor in step_qubits and op.endswith("-lw")
                                for op, step_qubits in scheduled_trace[upper_step]
                            ):
                                upper_step += 1
                                if upper_step >= len(scheduled_trace):
                                    break
                            if upper_step < step_idx:
                                # Use the located step for the upper wire operation instead of
                                # putting the lower wire operation at the end.
                                step_idx = upper_step
                                event = (
                                    event[0].replace("-lw", "-up"),
                                    event[1],
                                )
                                break
                        # Couldn't find a better spot for the upper wire operation, so
                        # put the lower wire operation at the end.
                        scheduled_trace.append([])
                        break
            elif event[0].endswith("-up"):
                # If the event is an upper wire operation, ensure its top neighbor
                # doesn't have a lower wire operation scheduled in the same step.
                neighbor = event[1][0] - num_cols
                while any(
                    neighbor in step_qubits and op.endswith("-lw")
                    for op, step_qubits in scheduled_trace[step_idx]
                ):
                    step_idx += 1
                    if step_idx >= len(scheduled_trace):
                        scheduled_trace.append([])
                        break
            scheduled_trace[step_idx].append((event[0], event[1]))
        else:
            # If there are other events scheduled, check the last step to see if the same qubit already has an operation scheduled.
            if (
                len(scheduled_trace) > 0
                and (
                    other_event := next(
                        (
                            (op, step_qubits)
                            for op, step_qubits in scheduled_trace[-1]
                            if any(qubit in event[1] for qubit in step_qubits)
                        ),
                        None,
                    )
                )
                and len(other_event[1]) == 1
                and (other_event[0].split("-")[0], other_event[1][0])
                == (event[0].split("-")[0], event[1][0])
            ):
                # Skip scheduling this event if the same qubit already has the same operation scheduled.
                continue

            scheduled_trace.append([(event[0], event[1])])

    return scheduled_trace


def convert(event: Tuple[str, List[int]]) -> Tuple[str, List[int]]:
    name, targets = event
    match name:
        case "M" | "Mresetz" | "Reset":
            name = "Mz-lw"
        case "My":
            name = "My-lw"
        case "T" | "Tadj":
            name = "T-lw"
        case "Myz":
            if targets[0] > targets[1]:
                name = "Mzy"
                targets = [targets[1], targets[0]]
        case "Mzy":
            if targets[0] > targets[1]:
                name = "Myz"
                targets = [targets[1], targets[0]]
        case "Mxx":
            if targets[0] > targets[1]:
                targets = [targets[1], targets[0]]
        case "Mzz" | "Myy":
            if targets[0] > targets[1]:
                targets = [targets[1], targets[0]]
    return (name, targets)


def tetron_bottom_row_ids(layout: Tuple[int, int]) -> List[int]:
    num_rows, num_cols = layout
    return [i for i in range((num_rows - 1) * num_cols, num_rows * num_cols)]


def tetron_top_row_ids(layout: Tuple[int, int]) -> List[int]:
    _, num_cols = layout
    return [i for i in range(0, num_cols)]


def virtual_vertical_neighbor_ids(layout: Tuple[int, int], id: int) -> List[int]:
    num_rows, num_cols = layout
    return [
        neighbor
        for neighbor in [id - num_cols, id + num_cols]
        if neighbor >= 0 and neighbor < (num_rows // 2) * num_cols
    ]


def virtual_horizontal_neighbor_ids(layout: Tuple[int, int], id: int) -> List[int]:
    _, num_cols = layout
    neighbors = []
    if id % num_cols != 0:
        neighbors.append(id - 1)
    if (id + 1) % num_cols != 0:
        neighbors.append(id + 1)
    return neighbors


def physical_and_ancilla_from_virtual(
    layout: Tuple[int, int], virtual: int
) -> Tuple[int, int]:
    _, num_cols = layout
    virtual_row = virtual // num_cols
    col = virtual - (virtual_row * num_cols)
    physical_row = virtual_row * 2
    t1 = physical_row * num_cols + col
    t2 = (physical_row + 1) * num_cols + col
    if virtual % 2 == 0:
        return (t1, t2)
    else:
        return (t2, t1)


def schedule_virtual(
    layout: Tuple[int, int], trace: List[Tuple[str, List[int]]]
) -> List[List[Tuple[str, List[int]]]]:
    tetron_trace = []
    for name, targets in trace:
        tetron_trace += decompose(layout, (name, targets))
    return schedule_tetrons(layout, tetron_trace)


def decompose(
    layout: Tuple[int, int], event: Tuple[str, List[int]]
) -> List[Tuple[str, List[int], Tuple[str, List[int]]]]:
    name, targets = event
    tetron_targets = [physical_and_ancilla_from_virtual(layout, t) for t in targets]
    adjacency = None
    if len(targets) == 2:
        if targets[0] in virtual_horizontal_neighbor_ids(layout, targets[1]):
            adjacency = "horizontal"
        elif targets[0] in virtual_vertical_neighbor_ids(layout, targets[1]):
            adjacency = "vertical"
        if adjacency is None:
            raise ValueError(
                f"Targets for {name} must be either horizontal or vertical neighbors, got {targets}"
            )
    match name:
        case "X" | "Y" | "Z" | "T" | "M" | "Mx" | "My" | "Mresetz" | "Reset":
            virtual_name = name
            match name:
                case "M" | "Mresetz" | "Reset":
                    virtual_name = "Mz"
            return [(name, [t[0] for t in tetron_targets], (virtual_name, targets))]
        case "H":
            tetron_target, tetron_ancilla = tetron_targets[0][0], tetron_targets[0][1]
            return apply_H(tetron_target, tetron_ancilla, targets)
        case "S" | "Sadj":
            tetron_target, tetron_ancilla = tetron_targets[0][0], tetron_targets[0][1]
            return apply_S(tetron_target, tetron_ancilla, targets)
        case "SX":
            tetron_target, tetron_ancilla = tetron_targets[0][0], tetron_targets[0][1]
            return apply_SX(tetron_target, tetron_ancilla, targets)
        case "CX":
            control, target = targets[0], targets[1]
            if adjacency == "horizontal":
                return apply_CX_horizontal(
                    tetron_targets[0][0],
                    tetron_targets[1][0],
                    tetron_targets[0][1],
                    targets,
                )
            else:
                assert adjacency == "vertical"
                if (targets[0] % 2 == 0 and targets[0] < targets[1]) or (
                    targets[0] % 2 == 1 and targets[0] > targets[1]
                ):
                    return apply_CX_vertical(
                        tetron_targets[0][0],
                        tetron_targets[1][0],
                        tetron_targets[0][1],
                        tetron_targets[1][1],
                        targets,
                    )
                else:
                    return apply_CX_vertical(
                        tetron_targets[1][0],
                        tetron_targets[0][0],
                        tetron_targets[1][1],
                        tetron_targets[0][1],
                        targets,
                    )
        case "CZ":
            if adjacency == "vertical":
                if (targets[0] % 2 == 0 and targets[0] < targets[1]) or (
                    targets[0] % 2 == 1 and targets[0] > targets[1]
                ):
                    return apply_CZ_vertical(
                        tetron_targets[0][0],
                        tetron_targets[1][0],
                        tetron_targets[0][1],
                        targets,
                    )
                else:
                    return apply_CZ_vertical(
                        tetron_targets[0][0],
                        tetron_targets[1][0],
                        tetron_targets[1][1],
                        targets,
                    )
            else:
                assert adjacency == "horizontal"
                return apply_CZ_horizontal(
                    tetron_targets[0][0],
                    tetron_targets[1][0],
                    tetron_targets[0][1],
                    tetron_targets[1][1],
                    targets,
                )
        case "CY":
            if adjacency == "vertical":
                if (targets[0] % 2 == 0 and targets[0] < targets[1]) or (
                    targets[0] % 2 == 1 and targets[0] > targets[1]
                ):
                    return apply_CY_vertical(
                        tetron_targets[0][0],
                        tetron_targets[1][0],
                        tetron_targets[0][1],
                        targets,
                    )
                else:
                    return apply_CY_vertical(
                        tetron_targets[0][0],
                        tetron_targets[1][0],
                        tetron_targets[1][1],
                        targets,
                    )
            else:
                assert adjacency == "horizontal"
                return apply_CY_horizontal(
                    tetron_targets[0][0],
                    tetron_targets[1][0],
                    tetron_targets[0][1],
                    tetron_targets[1][1],
                    targets,
                )
        case "Swap":
            if adjacency == "vertical":
                if (targets[0] % 2 == 0 and targets[0] < targets[1]) or (
                    targets[0] % 2 == 1 and targets[0] > targets[1]
                ):
                    return apply_SWAP_vertical(
                        tetron_targets[0][0],
                        tetron_targets[1][0],
                        tetron_targets[0][1],
                        targets,
                    )
                else:
                    return apply_SWAP_vertical(
                        tetron_targets[1][0],
                        tetron_targets[0][0],
                        tetron_targets[1][1],
                        targets,
                    )
            else:
                assert adjacency == "horizontal"
                return apply_SWAP_horizontal(
                    tetron_targets[0][0],
                    tetron_targets[1][0],
                    tetron_targets[0][1],
                    tetron_targets[1][1],
                    targets,
                )
        case _:
            raise ValueError(f"Unsupported gate: {name}")


def apply_H(target: int, ancilla: int, virtual_targets: List[int]):
    return [
        ("Mx", [ancilla], ("H", virtual_targets)),
        ("Mzy", [ancilla, target], ("H", virtual_targets)),
        ("My", [ancilla], ("H", virtual_targets)),
    ]


def apply_S(target: int, ancilla: int, virtual_targets: List[int]):
    return [
        ("Mx", [ancilla], ("S", virtual_targets)),
        ("Mzz", [ancilla, target], ("S", virtual_targets)),
        ("My", [ancilla], ("S", virtual_targets)),
    ]


def apply_SX(target: int, ancilla: int, virtual_targets: List[int]):
    return [
        ("Mx", [ancilla], ("SX", virtual_targets)),
        ("Mzz", [ancilla, target], ("SX", virtual_targets)),
        ("Mzy", [ancilla, target], ("SX", virtual_targets)),
        ("Mx", [ancilla], ("SX", virtual_targets)),
    ]


def apply_CX_horizontal(
    control: int, target: int, ancilla: int, virtual_targets: List[int]
):
    return [
        ("Mx", [ancilla], ("CX", virtual_targets)),
        ("Mzz", [ancilla, control], ("CX", virtual_targets)),
        ("Mxx", [ancilla, target], ("CX", virtual_targets)),
        ("M", [ancilla], ("CX", virtual_targets)),
    ]


def apply_CX_vertical(
    control: int,
    target: int,
    ancilla: int,
    target_ancilla: int,
    virtual_targets: List[int],
):
    return (
        apply_H(target, target_ancilla, virtual_targets[1:])
        + apply_CZ_vertical(control, target, ancilla, virtual_targets)
        + apply_H(target, target_ancilla, virtual_targets[1:])
    )


def apply_CZ_vertical(
    control: int, target: int, ancilla: int, virtual_targets: List[int]
):
    return [
        ("M", [ancilla], ("CZ", virtual_targets)),
        ("Mzy", [control, ancilla], ("CZ", virtual_targets)),
        ("Mzz", [ancilla, target], ("CZ", virtual_targets)),
        ("My", [ancilla], ("CZ", virtual_targets)),
    ]


def apply_CZ_horizontal(
    control: int,
    target: int,
    ancilla: int,
    target_ancilla: int,
    virtual_targets: List[int],
):
    return (
        apply_H(target, target_ancilla, virtual_targets[1:])
        + apply_CX_horizontal(control, target, ancilla, virtual_targets)
        + apply_H(target, target_ancilla, virtual_targets[1:])
    )


def apply_CY_vertical(
    control: int, target: int, ancilla: int, virtual_targets: List[int]
):
    return [
        ("M", [ancilla], ("CY", virtual_targets)),
        ("Mzy", [control, ancilla], ("CY", virtual_targets)),
        ("Mzy", [ancilla, target], ("CY", virtual_targets)),
        ("My", [ancilla], ("CY", virtual_targets)),
    ]


def apply_CY_horizontal(
    control: int,
    target: int,
    ancilla: int,
    target_ancilla: int,
    virtual_targets: List[int],
):
    return (
        apply_S(target, target_ancilla, virtual_targets[1:])
        + apply_CX_horizontal(control, target, ancilla, virtual_targets)
        + apply_S(target, target_ancilla, virtual_targets[1:])
    )


def apply_SWAP_vertical(t1: int, t2: int, ancilla: int, virtual_targets: List[int]):
    return (
        apply_CY_vertical(t1, t2, ancilla, virtual_targets)
        + apply_CY_vertical(t2, t1, ancilla, virtual_targets)
        + apply_CY_vertical(t1, t2, ancilla, virtual_targets)
    )


def apply_SWAP_horizontal(
    t1: int, t2: int, ancilla1: int, ancilla2: int, virtual_targets: List[int]
):
    return (
        apply_CX_horizontal(t1, t2, ancilla1, virtual_targets)
        + apply_CX_horizontal(t2, t1, ancilla2, virtual_targets)
        + apply_CX_horizontal(t1, t2, ancilla1, virtual_targets)
    )


class Trace(QirModuleVisitor):

    def __init__(self):
        self.trace = []
        self.num_qubits = 0
        super().__init__()

    def _on_function(self, function: Function) -> None:
        num_qubits = required_num_qubits(function)
        if num_qubits:
            self.num_qubits = num_qubits
        super()._on_function(function)

    def _on_call_instr(self, call: Call) -> None:
        if call.callee.name == "__quantum__rt__read_result":
            raise NotImplementedError(
                "Majorana tracing of programs that branch on measurement results is not supported"
            )
        if not call.callee.name.startswith("__quantum__qis__"):
            return
        name = call.callee.name[len("__quantum__qis__") :]
        gate, suffix = name.rsplit("__", 1)
        if gate.startswith("c"):
            gate = gate.upper()
        else:
            gate = gate.capitalize()
        targets = [ptr_id(arg) for arg in call.args if arg]
        if gate.startswith("M"):
            targets = targets[:-1]
        if suffix == "adj":
            gate += "adj"
        self.trace.append((gate, targets))
