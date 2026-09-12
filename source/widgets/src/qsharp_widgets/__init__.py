# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import importlib.metadata
import json
import pathlib
import time
from typing import Literal

import anywidget
import traitlets

try:
    __version__ = importlib.metadata.version("qsharp_widgets")
except importlib.metadata.PackageNotFoundError:
    __version__ = "unknown"


class SpaceChart(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("SpaceChart").tag(sync=True)
    estimates = traitlets.Dict().tag(sync=True)
    index = traitlets.Integer().tag(sync=True)

    def __init__(self, estimates, index=None):
        """
        This function generates a chart for the qubit utilization of the estimates.

        Parameters:
        - estimates: data for the chart.
        - index (optional): the index of the estimate to be displayed. In case of a single point estimate, the parameter is ignored. In case of the frontier estimate, indexes correspond to points on frontier from the shortest runtime to the longest one. If not provided, the shortest runtime estimate is displayed.
        """
        super().__init__(estimates=estimates, index=0 if index is None else index)


class EstimatesOverview(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("EstimatesOverview").tag(sync=True)
    estimates = traitlets.Dict().tag(sync=True)
    colors = traitlets.List().tag(sync=True)
    runNames = traitlets.List().tag(sync=True)

    def __init__(self, estimates, colors=None, runNames=None):
        """
        This function generates a summary results table with a qubit-time diagram.

        Parameters:
        - estimates: data for the table and the diagram.
        - colors (optional): the list of colors which could be provided in the hex form or by name. If the length of the list does not match the number of the estimates, the colors parameter will be ignored and replaced with defaults.
        - runNames (optional): the list of the run names. If the length of the list does not match the number of the estimates, the runNames parameter will be ignored and replaced with defaults.

        Returns:
        None
        """
        super().__init__(
            estimates=estimates,
            colors=[] if colors is None else colors,
            runNames=[] if runNames is None else runNames,
        )


class EstimatesPanel(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("EstimatesPanel").tag(sync=True)
    estimates = traitlets.Dict().tag(sync=True)
    colors = traitlets.List().tag(sync=True)
    runNames = traitlets.List().tag(sync=True)

    def __init__(self, estimates, colors=None, runNames=None):
        """
        This function generates the whole estimates panel with the summary results table, the space-time chart, the space chart and the details report.

        Parameters:
        - estimates: data for all the tables and diagrams.
        - colors (optional): the list of colors which could be provided in the hex form or by name. If the length of the list does not match the number of the estimates, the colors parameter will be ignored and replaced with defaults.
        - runNames (optional): the list of the run names. If the length of the list does not match the number of the estimates, the runNames parameter will be ignored and replaced with defaults.

        Returns:
        None
        """
        super().__init__(
            estimates=estimates,
            colors=[] if colors is None else colors,
            runNames=[] if runNames is None else runNames,
        )


class EstimateDetails(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("EstimateDetails").tag(sync=True)
    estimates = traitlets.Dict().tag(sync=True)
    index = traitlets.Integer().tag(sync=True)

    def __init__(self, estimates, index=None):
        """
        This function generates a report for the qubit utilization of the estimates.

        Parameters:
        - estimates: data for the report.
        - index (optional): the index of the estimate to be displayed. In case of a single point estimate, the parameter is ignored. In case of the frontier estimate, indexes correspond to points on frontier from the shortest runtime to the longest one. If not provided, the shortest runtime estimate is displayed.
        """
        super().__init__(estimates=estimates, index=0 if index is None else index)


class Histogram(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Histogram").tag(sync=True)
    buckets = traitlets.Dict().tag(sync=True)
    shot_count = traitlets.Integer().tag(sync=True)
    shot_header = traitlets.Bool(True).tag(sync=True)
    labels = traitlets.Unicode("raw").tag(sync=True)
    items = traitlets.Unicode("all").tag(sync=True)
    sort = traitlets.Unicode("a-to-z").tag(sync=True)

    def _update_ui(self):
        self.buckets = self._new_buckets.copy()
        self.shot_count = self._new_count
        self._last_message = time.time()

    def _add_result(self, result):
        result_str = str(result["result"])
        old_value = self._new_buckets.get(result_str, 0)
        self._new_buckets.update({result_str: old_value + 1})
        self._new_count += 1

        # Only update the UI max 10 times per second
        if time.time() - self._last_message >= 0.1:
            self._update_ui()

    def __init__(
        self,
        results=None,
        *,
        shot_header=True,
        bar_values=None,
        labels: Literal["raw", "kets", "none"] = "raw",
        items: Literal["all", "top-10", "top-25"] = "all",
        sort: Literal["a-to-z", "high-to-low", "low-to-high"] = "a-to-z",
    ):
        # Set up initial values before calling super().__init__()
        self._new_buckets = {}
        self._new_count = 0
        self._last_message = time.time()

        # Calculate initial traitlet values
        initial_shot_header = shot_header
        initial_buckets = {}
        initial_shot_count = 0

        # If provided a list of results, count the buckets and update.
        # Need to distinguish between the case where we're provided a list of results
        # or a list of ShotResults
        if results is not None:
            for result in results:
                if isinstance(result, dict) and "result" in result:
                    self._add_result(result)
                else:
                    # Convert the raw result to a ShotResult for the call
                    self._add_result({"result": result, "events": []})

            initial_buckets = self._new_buckets.copy()
            initial_shot_count = self._new_count
        elif bar_values is not None:
            initial_buckets = bar_values
            initial_shot_count = 0
            initial_shot_header = False

        # Pass all initial values to super().__init__()
        super().__init__(
            shot_header=initial_shot_header,
            buckets=initial_buckets,
            shot_count=initial_shot_count,
            labels=labels,
            items=items,
            sort=sort,
        )

    def run(self, entry_expr, shots):
        from qdk import qsharp

        self._new_buckets = {}
        self._new_count = 0

        # Note: For now, we don't care about saving the results, just counting
        # up the results for each bucket. If/when we add output details and
        # navigation, then we'll need to save the results. However, we pass
        # 'save_results=True' to avoid printing to the console.
        qsharp.run(entry_expr, shots, on_result=self._add_result, save_events=True)

        # Update the UI one last time to make sure we show the final results
        self._update_ui()


class Circuit(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Circuit").tag(sync=True)
    circuit_json = traitlets.Unicode().tag(sync=True)

    def __init__(self, circuit):
        super().__init__(circuit_json=circuit.json())
        self.layout.overflow = "visible scroll"


class BlochSphere(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "bloch.js"
    _css = pathlib.Path(__file__).parent / "static" / "bloch.css"

    comp = traitlets.Unicode("BlochSphere").tag(sync=True)
    _initial_gates = traitlets.Unicode("").tag(sync=True)

    def __init__(self, initial_gates=""):
        """
        This function displays an interactive Bloch sphere for exploring
        single-qubit states and gates.

        Parameters:
        - initial_gates (optional): a whitespace-separated sequence of gate
          tokens to replay when the widget is first shown. Fixed gates are
          X, Y, Z, H, S, T, and SX; adjoints use a trailing apostrophe
          (S', T', SX'); rotations are Rx(angle), Ry(angle), and Rz(angle)
          with the angle in radians. For example: "X H Z" or "H Rx(1.5708) S'".
        """
        super().__init__(_initial_gates=initial_gates)


class Atoms(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Atoms").tag(sync=True)
    machine_layout = traitlets.Dict().tag(sync=True)
    trace_data = traitlets.Dict().tag(sync=True)

    def __init__(self, machine_layout, trace_data):
        super().__init__(machine_layout=machine_layout, trace_data=trace_data)


_SINGLE_MAJORANA_OPERATIONS = {
    "Mx",
    "My-up",
    "My-lw",
    "Mz-up",
    "Mz-lw",
    "T",
}
_JOINT_MAJORANA_OPERATIONS = {"Mzz", "Mzy", "Myy", "Myz", "Mxx"}
_SINGLE_VIRTUAL_OPERATIONS = {"T", "H", "S", "Mx", "My", "Mz"}
_JOINT_VIRTUAL_OPERATIONS = {"CX", "CZ"}
_MAJORANA_VIEWS = ("Tetrons", "Qubits", "Virtual")


def _describe_majorana_value(value):
    if isinstance(value, str):
        return json.dumps(value)
    if value is None or isinstance(value, (bool, int, float)):
        return str(value)
    if isinstance(value, (list, tuple)):
        return f"Array(length={len(value)})"
    return type(value).__name__


def _majorana_error(error_type, path, message, value):
    raise error_type(f"{path}: {message}; received {_describe_majorana_value(value)}")


def _normalize_majorana_device_size(value):
    if not isinstance(value, (list, tuple)):
        _majorana_error(
            TypeError,
            "deviceSize",
            "expected [cellRows, cellColumns]",
            value,
        )
    if len(value) != 2:
        _majorana_error(
            ValueError,
            "deviceSize",
            "expected [cellRows, cellColumns]",
            value,
        )

    result = []
    for index, dimension in enumerate(value):
        if isinstance(dimension, bool) or not isinstance(dimension, int):
            _majorana_error(
                TypeError,
                f"deviceSize[{index}]",
                "expected an integer from 1 through 4",
                dimension,
            )
        if dimension < 1 or dimension > 4:
            _majorana_error(
                ValueError,
                f"deviceSize[{index}]",
                "expected an integer from 1 through 4",
                dimension,
            )
        result.append(dimension)
    return result


def _majorana_tetron_position(qubit_id, cell_rows):
    cell_block = qubit_id // 4
    cell_column = cell_block // cell_rows
    cell_row = cell_block % cell_rows
    local_number = qubit_id % 4
    return cell_row * 2 + local_number // 2, cell_column * 2 + local_number % 2


def _normalize_majorana_targets(targets, path, count, qubit_count):
    if not isinstance(targets, (list, tuple)):
        _majorana_error(
            TypeError,
            f"{path}.targets",
            f"expected exactly {'one target' if count == 1 else 'two targets'}",
            targets,
        )
    if len(targets) != count:
        _majorana_error(
            ValueError,
            f"{path}.targets",
            f"expected exactly {'one target' if count == 1 else 'two targets'}",
            targets,
        )

    result = []
    for index, target in enumerate(targets):
        target_path = f"{path}.targets[{index}]"
        if isinstance(target, bool) or not isinstance(target, int):
            _majorana_error(
                TypeError,
                target_path,
                f"expected an integer from 0 through {qubit_count - 1}",
                target,
            )
        if target < 0 or target >= qubit_count:
            _majorana_error(
                ValueError,
                target_path,
                f"expected an integer from 0 through {qubit_count - 1}",
                target,
            )
        result.append(target)
    return result


def _normalize_majorana_virtual_operation(value, path, device_size):
    if not isinstance(value, (list, tuple)):
        _majorana_error(
            TypeError,
            path,
            "expected [operationName, targets, precedence]",
            value,
        )
    if len(value) != 3:
        _majorana_error(
            ValueError,
            path,
            "expected [operationName, targets, precedence]",
            value,
        )

    name, targets, precedence = value
    if not isinstance(name, str):
        _majorana_error(
            TypeError,
            f"{path}.operation",
            "expected a supported virtual operation name",
            name,
        )
    if (
        name not in _SINGLE_VIRTUAL_OPERATIONS
        and name not in _JOINT_VIRTUAL_OPERATIONS
    ):
        _majorana_error(
            ValueError,
            f"{path}.operation",
            "expected a supported virtual operation name",
            name,
        )

    target_count = 1 if name in _SINGLE_VIRTUAL_OPERATIONS else 2
    if not isinstance(targets, (list, tuple)):
        _majorana_error(
            TypeError,
            f"{path}.targets",
            f"expected exactly {'one target' if target_count == 1 else 'two targets'}",
            targets,
        )
    if len(targets) != target_count:
        _majorana_error(
            ValueError,
            f"{path}.targets",
            f"expected exactly {'one target' if target_count == 1 else 'two targets'}",
            targets,
        )
    if isinstance(precedence, bool) or not isinstance(precedence, int):
        _majorana_error(
            TypeError,
            f"{path}.precedence",
            "expected a non-negative integer",
            precedence,
        )
    if precedence < 0:
        _majorana_error(
            ValueError,
            f"{path}.precedence",
            "expected a non-negative integer",
            precedence,
        )

    virtual_qubit_count = device_size[0] * device_size[1] * 2
    normalized_targets = []
    for index, target in enumerate(targets):
        target_path = f"{path}.targets[{index}]"
        if isinstance(target, bool) or not isinstance(target, int):
            _majorana_error(
                TypeError,
                target_path,
                f"expected an integer from 0 through {virtual_qubit_count - 1}",
                target,
            )
        if target < 0 or target >= virtual_qubit_count:
            _majorana_error(
                ValueError,
                target_path,
                f"expected an integer from 0 through {virtual_qubit_count - 1}",
                target,
            )
        normalized_targets.append(target)

    if target_count == 1:
        return [name, normalized_targets, precedence]

    first_target, second_target = normalized_targets
    if first_target == second_target:
        _majorana_error(
            ValueError,
            f"{path}.targets",
            "expected two distinct virtual targets",
            normalized_targets,
        )
    orientation = _majorana_virtual_adjacency(
        first_target, second_target, device_size
    )
    expected_orientation = "horizontal" if name == "CX" else "vertical"
    if orientation != expected_orientation:
        _majorana_error(
            ValueError,
            f"{path}.targets",
            f"expected {expected_orientation}ly adjacent virtual targets",
            normalized_targets,
        )
    return [name, normalized_targets, precedence]


def _majorana_virtual_adjacency(first_target, second_target, device_size):
    cell_rows, _ = device_size

    def position(target):
        cell_id, side = divmod(target, 2)
        return cell_id % cell_rows, cell_id // cell_rows, side

    first_row, first_column, first_side = position(first_target)
    second_row, second_column, second_side = position(second_target)
    if first_row == second_row and (
        (first_column == second_column and first_side != second_side)
        or (
            abs(first_column - second_column) == 1
            and (
                (
                    first_column < second_column
                    and first_side == 1
                    and second_side == 0
                )
                or (
                    second_column < first_column
                    and second_side == 1
                    and first_side == 0
                )
            )
        )
    ):
        return "horizontal"
    if (
        first_column == second_column
        and first_side == second_side
        and abs(first_row - second_row) == 1
    ):
        return "vertical"
    return None


def _normalize_majorana_operation(
    value, path, device_size, enable_virtual_view=False
):
    if not isinstance(value, (list, tuple)):
        _majorana_error(
            TypeError,
            path,
            "expected [physicalOperation, physicalTargets, virtualOperation]",
            value,
        )
    if len(value) != 3:
        _majorana_error(
            ValueError,
            path,
            "expected [physicalOperation, physicalTargets, virtualOperation]",
            value,
        )

    name, targets, virtual_operation = value
    if not isinstance(name, str):
        _majorana_error(
            TypeError,
            f"{path}.operation",
            "expected a supported operation name",
            name,
        )
    if (
        name not in _SINGLE_MAJORANA_OPERATIONS
        and name not in _JOINT_MAJORANA_OPERATIONS
    ):
        _majorana_error(
            ValueError,
            f"{path}.operation",
            "expected a supported operation name",
            name,
        )

    cell_rows, cell_columns = device_size
    qubit_count = cell_rows * cell_columns * 4
    if name in _SINGLE_MAJORANA_OPERATIONS:
        normalized_targets = _normalize_majorana_targets(targets, path, 1, qubit_count)
        target = normalized_targets[0]
        row, _ = _majorana_tetron_position(target, cell_rows)
        if name in {"My-up", "Mz-up"} and row == 0:
            _majorana_error(
                ValueError,
                f"{path}.targets[0]",
                f"{name} requires an island above the target",
                target,
            )
        if name in {"My-lw", "Mz-lw"} and row == cell_rows * 2 - 1:
            _majorana_error(
                ValueError,
                f"{path}.targets[0]",
                f"{name} requires an island below the target",
                target,
            )
        return [
            name,
            normalized_targets,
            _normalize_majorana_virtual_operation(
                virtual_operation, f"{path}.virtualOperation", device_size
            )
            if enable_virtual_view
            else None,
        ]

    normalized_targets = _normalize_majorana_targets(targets, path, 2, qubit_count)
    first_target, second_target = normalized_targets
    if first_target == second_target:
        _majorana_error(
            ValueError,
            f"{path}.targets",
            "expected two distinct targets",
            normalized_targets,
        )

    first_row, first_column = _majorana_tetron_position(first_target, cell_rows)
    second_row, second_column = _majorana_tetron_position(second_target, cell_rows)
    if name == "Mxx":
        if first_row != second_row or abs(first_column - second_column) != 1:
            _majorana_error(
                ValueError,
                f"{path}.targets",
                "expected horizontally adjacent targets",
                normalized_targets,
            )
        if first_column >= second_column:
            _majorana_error(
                ValueError,
                f"{path}.targets",
                "expected targets ordered left then right",
                normalized_targets,
            )
    else:
        if first_column != second_column or abs(first_row - second_row) != 1:
            _majorana_error(
                ValueError,
                f"{path}.targets",
                "expected vertically adjacent targets",
                normalized_targets,
            )
        if first_row >= second_row:
            _majorana_error(
                ValueError,
                f"{path}.targets",
                "expected targets ordered upper then lower",
                normalized_targets,
            )
    return [
        name,
        normalized_targets,
        _normalize_majorana_virtual_operation(
            virtual_operation, f"{path}.virtualOperation", device_size
        )
        if enable_virtual_view
        else None,
    ]


def _normalize_majorana_trace(value, device_size, enable_virtual_view=False):
    if not isinstance(value, (list, tuple)):
        _majorana_error(TypeError, "trace", "expected a non-empty array", value)
    if not value:
        _majorana_error(ValueError, "trace", "expected a non-empty array", value)

    normalized_trace = []
    for step_index, step in enumerate(value):
        path = f"trace[{step_index}]"
        if not isinstance(step, (list, tuple)):
            _majorana_error(TypeError, path, "expected a non-empty array", step)
        if not step:
            _majorana_error(ValueError, path, "expected a non-empty array", step)
        normalized_trace.append(
            [
                _normalize_majorana_operation(
                    operation,
                    f"{path}[{operation_index}]",
                    device_size,
                    enable_virtual_view,
                )
                for operation_index, operation in enumerate(step)
            ]
        )
    return normalized_trace


def _majorana_feature_mode(obj):
    if getattr(obj, "_majorana_initialized", False):
        return obj.enable_virtual_view
    return getattr(obj, "_majorana_initial_enable_virtual_view", False)


def _normalize_majorana_view(value, enable_virtual_view):
    expected = (
        '"Tetrons", "Qubits", or "Virtual"'
        if enable_virtual_view
        else '"Tetrons" or "Qubits"'
    )
    if not isinstance(value, str):
        _majorana_error(
            TypeError,
            "options.initialView",
            f"expected {expected}",
            value,
        )
    if value not in _MAJORANA_VIEWS or (
        value == "Virtual" and not enable_virtual_view
    ):
        _majorana_error(
            ValueError,
            "options.initialView",
            f"expected {expected}",
            value,
        )
    return value


class _MajoranaDeviceSizeTrait(traitlets.TraitType):
    info_text = "a two-item Majorana device size"

    def validate(self, obj, value):
        normalized = _normalize_majorana_device_size(value)
        if "trace" in obj._trait_values:
            _normalize_majorana_trace(
                obj._trait_values["trace"],
                normalized,
                _majorana_feature_mode(obj),
            )
        return normalized


class _MajoranaTraceTrait(traitlets.TraitType):
    info_text = "a valid Majorana operation trace"

    def validate(self, obj, value):
        return _normalize_majorana_trace(
            value,
            obj.device_size,
            _majorana_feature_mode(obj),
        )


class _MajoranaBoolTrait(traitlets.Bool):
    def __init__(self, default_value, option_name):
        self.option_name = option_name
        super().__init__(default_value)

    def validate(self, obj, value):
        if not isinstance(value, bool):
            _majorana_error(
                TypeError,
                f"options.{self.option_name}",
                "expected a boolean",
                value,
            )
        return value


class _MajoranaFeatureModeTrait(_MajoranaBoolTrait):
    def validate(self, obj, value):
        value = super().validate(obj, value)
        if getattr(obj, "_majorana_initialized", False):
            current = obj._trait_values.get("enable_virtual_view")
            if current is not None and value != current:
                _majorana_error(
                    ValueError,
                    "options.enableVirtualView",
                    "cannot be changed after construction",
                    value,
                )
        return value


class _MajoranaViewTrait(traitlets.Enum):
    def __init__(self):
        super().__init__(_MAJORANA_VIEWS, default_value="Tetrons")

    def validate(self, obj, value):
        return _normalize_majorana_view(value, _majorana_feature_mode(obj))


class Majorana(anywidget.AnyWidget):
    """Display a Majorana device and its operation trace.

    ``initial_view`` is read when the browser component mounts. Changing it
    later does not override the view selected by the user.
    """

    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Majorana").tag(sync=True)
    device_size = _MajoranaDeviceSizeTrait().tag(sync=True)
    trace = _MajoranaTraceTrait().tag(sync=True)
    enable_virtual_view = _MajoranaFeatureModeTrait(
        False, "enableVirtualView"
    ).tag(sync=True)
    show_qubit_labels = _MajoranaBoolTrait(True, "showQubitLabels").tag(sync=True)
    show_mzm_labels = _MajoranaBoolTrait(False, "showMzmLabels").tag(sync=True)
    initial_view = _MajoranaViewTrait().tag(sync=True)

    def __init__(
        self,
        device_size,
        trace,
        *,
        enable_virtual_view=False,
        show_qubit_labels=True,
        show_mzm_labels=False,
        initial_view=None,
    ):
        normalized_device_size = _normalize_majorana_device_size(device_size)
        if not isinstance(enable_virtual_view, bool):
            _majorana_error(
                TypeError,
                "options.enableVirtualView",
                "expected a boolean",
                enable_virtual_view,
            )
        normalized_trace = _normalize_majorana_trace(
            trace,
            normalized_device_size,
            enable_virtual_view,
        )
        if not isinstance(show_qubit_labels, bool):
            _majorana_error(
                TypeError,
                "options.showQubitLabels",
                "expected a boolean",
                show_qubit_labels,
            )
        if not isinstance(show_mzm_labels, bool):
            _majorana_error(
                TypeError,
                "options.showMzmLabels",
                "expected a boolean",
                show_mzm_labels,
            )
        resolved_initial_view = (
            "Virtual" if enable_virtual_view else "Tetrons"
        ) if initial_view is None else initial_view
        resolved_initial_view = _normalize_majorana_view(
            resolved_initial_view,
            enable_virtual_view,
        )
        self._majorana_initialized = False
        self._majorana_initial_enable_virtual_view = enable_virtual_view
        super().__init__(
            device_size=normalized_device_size,
            trace=normalized_trace,
            enable_virtual_view=enable_virtual_view,
            show_qubit_labels=show_qubit_labels,
            show_mzm_labels=show_mzm_labels,
            initial_view=resolved_initial_view,
        )
        self._majorana_initialized = True
        del self._majorana_initial_enable_virtual_view


class Entanglement(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("Entanglement").tag(sync=True)
    s1_entropies = traitlets.List().tag(sync=True)
    mutual_information = traitlets.List().tag(sync=True)
    labels = traitlets.List().tag(sync=True)
    selected_indices = traitlets.List(allow_none=True, default_value=None).tag(
        sync=True
    )
    groups = traitlets.Dict(allow_none=True, default_value=None).tag(sync=True)
    options = traitlets.Dict().tag(sync=True)

    def __init__(
        self,
        wavefunction=None,
        *,
        s1_entropies=None,
        mutual_information=None,
        labels=None,
        selected_indices=None,
        groups=None,
        **options,
    ):
        """
        Displays an entanglement chord diagram.

        Can be constructed either from a ``Wavefunction`` object or from raw
        entropy / mutual-information arrays.

        Parameters
        ----------
        wavefunction : optional
            A ``qdk_chemistry.Wavefunction`` instance (from the
            ``qdk-chemistry`` package) with single-orbital entropies and
            mutual information.  When provided, *s1_entropies* and
            *mutual_information* are extracted automatically.
        s1_entropies : list[float], optional
            Single-orbital entropies (length *N*).  Required when
            *wavefunction* is not given.
        mutual_information : list[list[float]], optional
            N×N mutual-information matrix.  Required when *wavefunction*
            is not given.
        labels : list[str], optional
            Orbital labels.  Defaults to ``["0", "1", …]``.
        selected_indices : list[int], optional
            Orbital indices to highlight (single group, legacy API).
        groups : dict[str, list[int]], optional
            Named groups of orbital indices.  Each group is rendered with
            a distinct outline colour and, when grouped, its members are
            placed adjacent on the ring.  Takes precedence over
            *selected_indices* for grouping when both are provided.
        **options
            Visual knobs forwarded to the JS component.  All are
            optional; snake_case names are converted to camelCase
            automatically.

            ``gap_deg`` : float
                Gap in degrees between adjacent arcs.  Default ``3``.
            ``radius`` : float
                Outer radius of the ring in SVG user units.  Default ``1``.
            ``arc_width`` : float
                Radial thickness of each arc as a fraction of the
                radius.  Default ``0.08``.
            ``line_scale`` : float or None
                Chord width multiplier.  ``None`` (default) auto-scales
                to the data range.
            ``mi_threshold`` : float
                Minimum mutual-information value to draw a chord.
                Default ``0`` (draw all).
            ``s1_vmax`` : float or None
                Clamp for the single-orbital-entropy colour scale.
                Default ``ln(4)``.
            ``mi_vmax`` : float or None
                Clamp for the mutual-information colour scale.
                Default ``ln(16)``.
            ``title`` : str or None
                Title shown above the diagram.
                Default ``"Entanglement"``.
            ``width`` : int
                SVG viewport width in pixels.  Default ``600``.
            ``height`` : int
                SVG viewport height in pixels.  Default ``660``.
            ``selection_color`` : str
                CSS colour for the highlight outline around selected
                arcs.  Default auto-detected from background luminance.
            ``selection_linewidth`` : float
                Stroke width of the selection outline.  Default ``1.2``.
            ``group_colors`` : list[str]
                Override outline colours for each group (cycles if
                fewer colours than groups).
            ``group_selected`` : bool
                When ``True``, reorder arcs so that members of each
                group sit adjacent on the ring.  Default ``False``.
        """
        if wavefunction is not None:
            try:
                from qdk_chemistry.data import Wavefunction as _Wavefunction
            except ImportError:
                raise ImportError(
                    "The 'qdk-chemistry' package is required when passing a "
                    "wavefunction object.  Install it with:  pip install qdk-chemistry"
                ) from None
            if not isinstance(wavefunction, _Wavefunction):
                raise TypeError(
                    f"Expected a qdk_chemistry.data.Wavefunction instance, "
                    f"got {type(wavefunction).__qualname__}"
                )

            raw_s1 = wavefunction.get_single_orbital_entropies()
            raw_mi = wavefunction.get_mutual_information()
            # Accept numpy arrays or plain lists; normalise to plain lists.
            s1_entropies = (
                raw_s1.tolist() if hasattr(raw_s1, "tolist") else list(raw_s1)
            )
            mutual_information = (
                raw_mi.tolist()
                if hasattr(raw_mi, "tolist")
                else [list(row) for row in raw_mi]
            )
            n = len(s1_entropies)
            if labels is None:
                try:
                    orbitals = wavefunction.get_orbitals()
                    if orbitals.has_active_space():
                        active_indices = orbitals.get_active_space_indices()[0]
                        labels = [str(idx) for idx in active_indices]
                    else:
                        labels = [str(i) for i in range(n)]
                except (AttributeError, TypeError, IndexError):
                    labels = [str(i) for i in range(n)]
        elif s1_entropies is None or mutual_information is None:
            raise ValueError(
                "Either 'wavefunction' or both 's1_entropies' and "
                "'mutual_information' must be provided."
            )

        if labels is None:
            labels = [str(i) for i in range(len(s1_entropies))]

        super().__init__(
            s1_entropies=s1_entropies,
            mutual_information=mutual_information,
            labels=labels,
            selected_indices=selected_indices,
            groups=groups,
            options=options,
        )


class MoleculeViewer(anywidget.AnyWidget):
    _esm = pathlib.Path(__file__).parent / "static" / "index.js"
    _css = pathlib.Path(__file__).parent / "static" / "index.css"

    comp = traitlets.Unicode("MoleculeViewer").tag(sync=True)
    molecule_data = traitlets.Unicode().tag(sync=True)
    cube_data = traitlets.Dict().tag(sync=True)
    isoval = traitlets.Float(0.02).tag(sync=True)

    def __init__(self, molecule_data, cube_data={}, isoval=0.02):
        """
        This function generates a 3D molecule viewer for the provided molecular data in XYZ format.

        Parameters:
        - molecule_data: string containing the molecular data in XYZ format.
        - cube_data (optional): a dictionary where keys are cube names and values are dictionaries with the following structure:
          - "data": string containing the cube data in .cube file format.
          - "info": (optional) a dictionary containing any metadata you want to display with the cube data.
        """
        super().__init__(
            molecule_data=molecule_data, cube_data=cube_data, isoval=isoval
        )
