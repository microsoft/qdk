# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ..._native import GlobalCallable, Closure
from ...qsharp import run
from typing import (
    Tuple,
    Union,
    Callable,
    Any,
    Optional,
    Literal,
    List,
    Generator,
    Union,
)


class MajoranaDevice:
    def __init__(self, rows: int, cols: int):
        self.layout = (rows, cols)

    def run(
        self,
        callable: Union[Callable, GlobalCallable, Closure],
        *args: Any,
        seed: Optional[int] = None,
        type: Optional[Literal["sparse", "clifford"]] = None,
        num_qubits: Optional[int] = None,
        show_trace: bool = True,
        use_virtual: bool = False,
    ):
        from IPython.display import display  # type: ignore[import-not-found]

        result = run(
            callable,
            1,  # shots
            *args,
            save_events=True,
            seed=seed,
            type=type,
            num_qubits=num_qubits,
        )[0]

        for event in result["events"]:
            display(event)

        if show_trace:
            try:
                from qsharp_widgets import Majorana  # type: ignore[import-not-found]
            except ImportError:
                raise ImportError(
                    "The qsharp-widgets package is required for showing Majorana trace visualization. "
                    "Please install it via 'pip install \"qdk[jupyter]\"' or 'pip install qsharp-widgets'."
                )
            if use_virtual:
                trace = schedule_virtual(self.layout, eval(result["trace"]))
            else:
                trace = schedule_tetrons(
                    self.layout,
                    eval(result["trace"]),
                )
            display(Majorana(self.layout, trace, enable_virtual_view=use_virtual))
        return result["result"]


def schedule_tetrons(
    layout: Tuple[int, int],
    trace: Union[
        List[Tuple[str, List[int]]],
        List[Tuple[str, List[int], Optional[Tuple[str, List[int], int]]]],
    ],
) -> List[List[Tuple[str, List[int]]]]:
    scheduled_trace = []
    for event in trace:
        virtual_event = None
        if len(event) == 3:
            virtual_event = event[2]
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
            for _, step_qubits, _ in scheduled_trace[step_idx]
        ):
            step_idx -= 1

        if step_idx < len(scheduled_trace) - 1:
            step_idx += 1
            if event[0].endswith("-lw"):
                # If the event is a lower wire operation, ensure it's bottom neighbor
                # doesn't have an upper wire operation scheduled in the same step.
                neighbor = event[1][0] + 2
                while any(
                    neighbor in step_qubits and op.endswith("-up")
                    for op, step_qubits, _ in scheduled_trace[step_idx]
                ):
                    step_idx += 1
                    if step_idx >= len(scheduled_trace):
                        # Couldn't find a place for this lower wire operation, but check
                        # to see if there is a better spot for an upper wire operation on the same qubit.
                        if event[1][0] not in tetron_top_row_ids(layout):
                            neighbor = event[1][0] - 2
                            upper_step = step_idx
                            while any(
                                neighbor in step_qubits and op.endswith("-lw")
                                for op, step_qubits, _ in scheduled_trace[upper_step]
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
                neighbor = event[1][0] - 2
                while any(
                    neighbor in step_qubits and op.endswith("-lw")
                    for op, step_qubits, _ in scheduled_trace[step_idx]
                ):
                    step_idx += 1
                    if step_idx >= len(scheduled_trace):
                        scheduled_trace.append([])
                        break
            scheduled_trace[step_idx].append(((event[0], event[1], virtual_event)))
        else:
            scheduled_trace.append([(event[0], event[1], virtual_event)])

    return scheduled_trace


def convert(event: Tuple[str, List[int]]) -> Tuple[str, List[int]]:
    name, targets = event
    match name:
        case "M" | "MResetZ" | "Reset":
            name = "Mz-lw"
        case "My":
            name = "My-lw"
        case "X" | "Y" | "Z":
            name = ("C" * (len(targets) - 1)) + name
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


def tetron_bottom_row_ids(layout: Tuple[int, int]) -> Generator[int]:
    rows, cols = layout
    for col in range(cols):
        idx = (col + 1) * (rows * 4) - 1
        yield idx - 1
        yield idx


def tetron_top_row_ids(layout: Tuple[int, int]) -> Generator[int]:
    rows, cols = layout
    for col in range(cols):
        idx = col * (rows * 4)
        yield idx
        yield idx + 1


def virtual_vertical_neighbor_ids(id: int) -> Generator[int]:
    yield id - 2
    yield id + 2


def virtual_horizontal_neighbor_ids(layout: Tuple[int, int], id: int) -> Generator[int]:
    rows, _ = layout
    if id % 2 == 0:
        yield id - 1 - (2 * rows)
        yield id + 1
    else:
        yield id - 1
        yield id - 1 + (2 * rows)


def physical_and_ancilla_from_virtual(virtual: int) -> Tuple[int, int]:
    if virtual % 2 == 0:
        return (2 * virtual, 2 * virtual + 2)
    else:
        return (2 * virtual + 1, 2 * virtual - 1)


def schedule_virtual(
    layout: Tuple[int, int], trace: List[Tuple[str, List[int]]]
) -> List[List[Tuple[str, List[int]]]]:
    tetron_trace = []
    for event in trace:
        name, targets = event
        match name:
            case "X" | "Y" | "Z":
                name = ("C" * (len(targets) - 1)) + name
        tetron_trace += decompose(layout, (name, targets))
    return schedule_tetrons(layout, tetron_trace)


def decompose(
    layout: Tuple[int, int], event: Tuple[str, List[int]]
) -> List[Tuple[str, List[int], Tuple[str, List[int], int]]]:
    name, targets = event
    tetron_targets = [physical_and_ancilla_from_virtual(t) for t in targets]
    match name:
        case "X" | "Y" | "Z" | "T" | "M" | "Mx" | "My" | "MResetZ" | "Reset":
            virtual_name = name
            match name:
                case "M" | "MResetZ" | "Reset":
                    virtual_name = "Mz"
            return [(name, [t[0] for t in tetron_targets], (virtual_name, targets, 0))]
        case "H":
            tetron_target, tetron_ancilla = tetron_targets[0][0], tetron_targets[0][1]
            return [
                ("Mx", [tetron_ancilla], ("H", targets, 2)),
                ("Mzy", [tetron_ancilla, tetron_target], ("H", targets, 1)),
                ("My", [tetron_ancilla], ("H", targets, 2)),
            ]
        case "S":
            tetron_target, tetron_ancilla = tetron_targets[0][0], tetron_targets[0][1]
            return [
                ("Mx", [tetron_ancilla], ("S", targets, 2)),
                ("Mzz", [tetron_ancilla, tetron_target], ("S", targets, 1)),
                ("My", [tetron_ancilla], ("S", targets, 2)),
            ]
        case "CX":
            control, target = targets[0], targets[1]
            assert target in virtual_horizontal_neighbor_ids(
                layout, control
            ), "CX must be between horizontal neighbors"
            tetron_control, tetron_ancilla, tetron_target = (
                tetron_targets[0][0],
                tetron_targets[0][1],
                tetron_targets[1][0],
            )
            return [
                ("Mx", [tetron_ancilla], ("CX", targets, 2)),
                ("Mzz", [tetron_ancilla, tetron_control], ("CX", targets, 1)),
                ("Mxx", [tetron_ancilla, tetron_target], ("CX", targets, 1)),
                ("M", [tetron_ancilla], ("CX", targets, 2)),
            ]
        case "CZ":
            if targets[0] % 2 == 0:
                control, target = min(targets[0], targets[1]), max(
                    targets[0], targets[1]
                )
            else:
                control, target = max(targets[0], targets[1]), min(
                    targets[0], targets[1]
                )
            assert target in virtual_vertical_neighbor_ids(
                control
            ), "CZ must be between vertical neighbors"
            tetron_control, tetron_ancilla = physical_and_ancilla_from_virtual(control)
            tetron_target, _ = physical_and_ancilla_from_virtual(target)
            return [
                ("M", [tetron_ancilla], ("CZ", targets, 2)),
                ("Mzy", [tetron_control, tetron_ancilla], ("CZ", targets, 1)),
                ("Mzz", [tetron_ancilla, tetron_target], ("CZ", targets, 1)),
                ("My", [tetron_ancilla], ("CZ", targets, 2)),
            ]
        case _:
            raise ValueError(f"Unsupported gate: {name}")
