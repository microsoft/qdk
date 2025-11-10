# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
import re
from typing import Any, Dict, List, Optional, Union
from dataclasses import dataclass, field
from .._native import physical_estimates

import json

try:
    # Both markdown and mdx_math (from python-markdown-math) must be present for our markdown
    # rendering logic to work. If either is missing, we'll fall back to plain text.
    import markdown
    import mdx_math

    has_markdown = True
except ImportError:
    has_markdown = False


class EstimatorError(BaseException):
    """
    An error returned from the resource estimation.
    """

    def __init__(self, code: str, message: str):
        self.message = f"Error estimating resources ({code}):\n{message}"
        self.code = code

    def __str__(self):
        return self.message


@dataclass
class AutoValidatingParams:
    """
    A helper class for target parameters.

    It has a function as_dict that automatically extracts a dictionary from
    the class' fields.  They are added to the result dictionary if their value
    is not None, the key is automatically transformed from Python snake case
    to camel case, and if validate is True and if the field has a validation
    function, the field is validated beforehand.
    """

    def as_dict(self, validate=True):
        result = {}

        for name, field in self.__dataclass_fields__.items():
            field_value = self.__getattribute__(name)
            if field_value is not None:
                # validate field?
                if validate and "validate" in field.metadata:
                    func = field.metadata["validate"]
                    # check for indirect call (like in @staticmethod)
                    if hasattr(func, "__func__"):
                        func = func.__func__
                    func(name, field_value)

                # translate field name to camel case
                s = re.sub(r"(_|-)+", " ", name).title().replace(" ", "")
                attribute = "".join([s[0].lower(), s[1:]])
                result[attribute] = field_value

        if validate:
            self.post_validation(result)

        return result

    def post_validation(self, result):
        """
        A function that is called after all individual fields have been
        validated, but before the result is returned.

        Here result is the current dictionary.
        """
        pass


def validating_field(validation_func, default=None):
    """
    A helper method to declare field for an AutoValidatingParams data class.
    """
    return field(default=default, metadata={"validate": validation_func})


class QubitParams:
    GATE_US_E3 = "qubit_gate_us_e3"
    GATE_US_E4 = "qubit_gate_us_e4"
    GATE_NS_E3 = "qubit_gate_ns_e3"
    GATE_NS_E4 = "qubit_gate_ns_e4"
    MAJ_NS_E4 = "qubit_maj_ns_e4"
    MAJ_NS_E6 = "qubit_maj_ns_e6"


class QECScheme:
    SURFACE_CODE = "surface_code"
    FLOQUET_CODE = "floquet_code"


def _check_error_rate(name, value):
    if value <= 0.0 or value >= 1.0:
        raise ValueError(f"{name} must be between 0 and 1")


def _check_error_rate_or_process_and_readout(name, value):
    if value is None:
        return

    if isinstance(value, float):
        _check_error_rate(name, value)
        return

    if not isinstance(value, MeasurementErrorRate):
        raise ValueError(
            f"{name} must be either a float or "
            "MeasurementErrorRate with two fields: 'process' and 'readout'"
        )


def check_time(name, value):
    pat = r"^(\+?[0-9]*\.?[0-9]+([eE][-+]?[0-9]+)?)\s*(s|ms|μs|µs|us|ns)$"
    if re.match(pat, value) is None:
        raise ValueError(
            f"{name} is not a valid time string; use a " "suffix s, ms, us, or ns"
        )


@dataclass
class MeasurementErrorRate(AutoValidatingParams):
    process: float = field(metadata={"validate": _check_error_rate})
    readout: float = field(metadata={"validate": _check_error_rate})


@dataclass
class EstimatorQubitParams(AutoValidatingParams):
    @staticmethod
    def check_instruction_set(name, value):
        if value not in [
            "gate-based",
            "gate_based",
            "GateBased",
            "gateBased",
            "Majorana",
            "majorana",
        ]:
            raise ValueError(f"{name} must be GateBased or Majorana")

    name: Optional[str] = None
    instruction_set: Optional[str] = validating_field(check_instruction_set)
    one_qubit_measurement_time: Optional[str] = validating_field(check_time)
    two_qubit_joint_measurement_time: Optional[str] = validating_field(check_time)
    one_qubit_gate_time: Optional[str] = validating_field(check_time)
    two_qubit_gate_time: Optional[str] = validating_field(check_time)
    t_gate_time: Optional[str] = validating_field(check_time)
    one_qubit_measurement_error_rate: Union[None, float, MeasurementErrorRate] = (
        validating_field(_check_error_rate_or_process_and_readout)
    )
    two_qubit_joint_measurement_error_rate: Union[None, float, MeasurementErrorRate] = (
        validating_field(_check_error_rate_or_process_and_readout)
    )
    one_qubit_gate_error_rate: Optional[float] = validating_field(_check_error_rate)
    two_qubit_gate_error_rate: Optional[float] = validating_field(_check_error_rate)
    t_gate_error_rate: Optional[float] = validating_field(_check_error_rate)
    idle_error_rate: Optional[float] = validating_field(_check_error_rate)

    _default_models = [
        QubitParams.GATE_US_E3,
        QubitParams.GATE_US_E4,
        QubitParams.GATE_NS_E3,
        QubitParams.GATE_NS_E4,
        QubitParams.MAJ_NS_E4,
        QubitParams.MAJ_NS_E6,
    ]
    _gate_based = ["gate-based", "gate_based", "GateBased", "gateBased"]
    _maj_based = ["Majorana", "majorana"]

    def post_validation(self, result):
        # check whether all fields have been specified in case a custom qubit
        # model is specified
        custom = result != {} and (
            self.name is None or self.name not in self._default_models
        )

        # no further validation needed for non-custom models
        if not custom:
            return

        # instruction set must be set
        if self.instruction_set is None:
            raise LookupError(
                "instruction_set must be set for custom qubit " "parameters"
            )

        # NOTE at this point, we know that instruction set must have valid
        # value
        if self.one_qubit_measurement_time is None:
            raise LookupError("one_qubit_measurement_time must be set")
        if self.one_qubit_measurement_error_rate is None:
            raise LookupError("one_qubit_measurement_error_rate must be set")

        # this only needs to be checked for gate based qubits
        if self.instruction_set in self._gate_based:
            if self.one_qubit_gate_time is None:
                raise LookupError("one_qubit_gate_time must be set")

    def as_dict(self, validate=True) -> Dict[str, Any]:
        qubit_params = super().as_dict(validate)
        if len(qubit_params) != 0:
            if isinstance(self.one_qubit_measurement_error_rate, MeasurementErrorRate):
                qubit_params["oneQubitMeasurementErrorRate"] = (
                    self.one_qubit_measurement_error_rate.as_dict(validate)
                )

            if isinstance(
                self.two_qubit_joint_measurement_error_rate, MeasurementErrorRate
            ):
                qubit_params["twoQubitJointMeasurementErrorRate"] = (
                    self.two_qubit_joint_measurement_error_rate.as_dict(validate)
                )

        return qubit_params


@dataclass
class EstimatorQecScheme(AutoValidatingParams):
    name: Optional[str] = None
    error_correction_threshold: Optional[float] = validating_field(_check_error_rate)
    crossing_prefactor: Optional[float] = None
    distance_coefficient_power: Optional[int] = None
    logical_cycle_time: Optional[str] = None
    physical_qubits_per_logical_qubit: Optional[str] = None
    max_code_distance: Optional[int] = None


@dataclass
class ProtocolSpecificDistillationUnitSpecification(AutoValidatingParams):
    num_unit_qubits: Optional[int] = None
    duration_in_qubit_cycle_time: Optional[int] = None

    def post_validation(self, result):
        if self.num_unit_qubits is None:
            raise LookupError("num_unit_qubits must be set")

        if self.duration_in_qubit_cycle_time is None:
            raise LookupError("duration_in_qubit_cycle_time must be set")


@dataclass
class DistillationUnitSpecification(AutoValidatingParams):
    name: Optional[str] = None
    display_name: Optional[str] = None
    num_input_ts: Optional[int] = None
    num_output_ts: Optional[int] = None
    failure_probability_formula: Optional[str] = None
    output_error_rate_formula: Optional[str] = None
    physical_qubit_specification: Optional[
        ProtocolSpecificDistillationUnitSpecification
    ] = None
    logical_qubit_specification: Optional[
        ProtocolSpecificDistillationUnitSpecification
    ] = None
    logical_qubit_specification_first_round_override: Optional[
        ProtocolSpecificDistillationUnitSpecification
    ] = None

    def has_custom_specification(self):
        return (
            self.display_name is not None
            or self.num_input_ts is not None
            or self.num_output_ts is not None
            or self.failure_probability_formula is not None
            or self.output_error_rate_formula is not None
            or self.physical_qubit_specification is not None
            or self.logical_qubit_specification is not None
            or self.logical_qubit_specification_first_round_override is not None
        )

    def has_predefined_name(self):
        return self.name is not None

    def post_validation(self, result):
        if not self.has_custom_specification() and not self.has_predefined_name():
            raise LookupError(
                "name must be set or custom specification must be provided"
            )

        if self.has_custom_specification() and self.has_predefined_name():
            raise LookupError(
                "If predefined name is provided, "
                "custom specification is not allowed. "
                "Either remove name or remove all other "
                "specification of the distillation unit"
            )

        if self.has_predefined_name():
            return  # all other validation is on the server side

        if self.num_input_ts is None:
            raise LookupError("num_input_ts must be set")

        if self.num_output_ts is None:
            raise LookupError("num_output_ts must be set")

        if self.failure_probability_formula is None:
            raise LookupError("failure_probability_formula must be set")

        if self.output_error_rate_formula is None:
            raise LookupError("output_error_rate_formula must be set")

        if self.physical_qubit_specification is not None:
            self.physical_qubit_specification.post_validation(result)

        if self.logical_qubit_specification is not None:
            self.logical_qubit_specification.post_validation(result)

        if self.logical_qubit_specification_first_round_override is not None:
            self.logical_qubit_specification_first_round_override.post_validation(
                result
            )

    def as_dict(self, validate=True) -> Dict[str, Any]:
        specification_dict = super().as_dict(validate)
        if len(specification_dict) != 0:
            if self.physical_qubit_specification is not None:
                physical_qubit_specification_dict = (
                    self.physical_qubit_specification.as_dict(validate)
                )
                if len(physical_qubit_specification_dict) != 0:
                    specification_dict["physicalQubitSpecification"] = (
                        physical_qubit_specification_dict
                    )

            if self.logical_qubit_specification is not None:
                logical_qubit_specification_dict = (
                    self.logical_qubit_specification.as_dict(validate)
                )
                if len(logical_qubit_specification_dict) != 0:
                    specification_dict["logicalQubitSpecification"] = (
                        logical_qubit_specification_dict
                    )

            if self.logical_qubit_specification_first_round_override is not None:
                logical_qubit_specification_first_round_override_dict = (
                    self.logical_qubit_specification_first_round_override.as_dict(
                        validate
                    )
                )
                if len(logical_qubit_specification_first_round_override_dict) != 0:
                    specification_dict[
                        "logicalQubitSpecificationFirstRoundOverride"
                    ] = logical_qubit_specification_first_round_override_dict

        return specification_dict


@dataclass
class ErrorBudgetPartition(AutoValidatingParams):
    logical: float = 0.001 / 3
    t_states: float = 0.001 / 3
    rotations: float = 0.001 / 3


@dataclass
class EstimatorConstraints(AutoValidatingParams):
    @staticmethod
    def at_least_one(name, value):
        if value < 1:
            raise ValueError(f"{name} must be at least 1")

    logical_depth_factor: Optional[float] = validating_field(at_least_one)
    max_t_factories: Optional[int] = validating_field(at_least_one)
    max_duration: Optional[int] = validating_field(check_time)
    max_physical_qubits: Optional[int] = validating_field(at_least_one)

    def post_validation(self, result):
        if self.max_duration is not None and self.max_physical_qubits is not None:
            raise LookupError(
                "Both duration and number of physical qubits constraints are provided, but only one is allowed at a time."
            )


class EstimatorInputParamsItem:
    """
    Input params for microsoft.estimator target

    :ivar error_budget Total error budget for execution of the algorithm
    """

    def __init__(self):
        super().__init__()

        self.qubit_params: EstimatorQubitParams = EstimatorQubitParams()
        self.qec_scheme: EstimatorQecScheme = EstimatorQecScheme()
        self.distillation_unit_specifications = (
            []
        )  # type: List[DistillationUnitSpecification]
        self.constraints: EstimatorConstraints = EstimatorConstraints()
        self.error_budget: Optional[Union[float, ErrorBudgetPartition]] = None
        self.estimate_type: Optional[str] = None

    def as_dict(self, validate=True, additional_params=None) -> Dict[str, Any]:
        result = {}

        qubit_params = self.qubit_params.as_dict(validate)
        if len(qubit_params) != 0:
            result["qubitParams"] = qubit_params
        elif hasattr(additional_params, "qubit_params"):
            qubit_params = additional_params.qubit_params.as_dict(validate)
            if len(qubit_params) != 0:
                result["qubitParams"] = qubit_params

        qec_scheme = self.qec_scheme.as_dict(validate)
        if len(qec_scheme) != 0:
            result["qecScheme"] = qec_scheme
        elif hasattr(additional_params, "qec_scheme"):
            qec_scheme = additional_params.qec_scheme.as_dict(validate)
            if len(qec_scheme) != 0:
                result["qecScheme"] = qec_scheme

        for specification in self.distillation_unit_specifications:
            specification_dict = specification.as_dict(validate)
            if len(specification_dict) != 0:
                if result.get("distillationUnitSpecifications") is None:
                    result["distillationUnitSpecifications"] = []

                result["distillationUnitSpecifications"].append(specification_dict)
        if result.get("distillationUnitSpecifications") is not None and hasattr(
            additional_params, "distillation_unit_specifications"
        ):
            for specification in additional_params.distillation_unit_specifications:
                specification_dict = specification.as_dict(validate)
                if len(specification_dict) != 0:
                    if result.get("distillationUnitSpecifications") is None:
                        result["distillationUnitSpecifications"] = []

                    result["distillationUnitSpecifications"].append(specification_dict)

        constraints = self.constraints.as_dict(validate)
        if len(constraints) != 0:
            result["constraints"] = constraints
        elif hasattr(additional_params, "constraints"):
            constraints = additional_params.constraints.as_dict(validate)
            if len(constraints) != 0:
                result["constraints"] = constraints

        if self.error_budget is not None:
            if isinstance(self.error_budget, float) or isinstance(
                self.error_budget, int
            ):
                if validate and (self.error_budget <= 0 or self.error_budget >= 1):
                    message = "error_budget must be value between 0 and 1"
                    raise ValueError(message)
                result["errorBudget"] = self.error_budget
            elif isinstance(self.error_budget, ErrorBudgetPartition):
                result["errorBudget"] = self.error_budget.as_dict(validate)
        elif hasattr(additional_params, "error_budget"):
            if isinstance(additional_params.error_budget, float) or isinstance(
                additional_params.error_budget, int
            ):
                if validate and (
                    additional_params.error_budget <= 0
                    or additional_params.error_budget >= 1
                ):
                    message = "error_budget must be value between 0 and 1"
                    raise ValueError(message)
                result["errorBudget"] = additional_params.error_budget
            elif isinstance(additional_params.error_budget, ErrorBudgetPartition):
                result["errorBudget"] = additional_params.error_budget.as_dict(validate)

        if self.estimate_type is not None:
            if self.estimate_type not in ["frontier", "singlePoint"]:
                raise ValueError(
                    "estimate_type must be either 'frontier' or 'singlePoint'"
                )
            result["estimateType"] = self.estimate_type

        return result


class EstimatorParams(EstimatorInputParamsItem):
    MAX_NUM_ITEMS: int = 1000

    def __init__(self, num_items: Optional[int] = None):
        EstimatorInputParamsItem.__init__(self)

        if num_items is not None:
            self.has_items = True
            if num_items <= 0 or num_items > self.MAX_NUM_ITEMS:
                raise ValueError(
                    "num_items must be a positive value less or equal to "
                    f"{self.MAX_NUM_ITEMS}"
                )
            self._items = [EstimatorInputParamsItem() for _ in range(num_items)]
        else:
            self.has_items = False

    @property
    def items(self) -> List:
        if self.has_items:
            return self._items
        else:
            raise Exception(
                "Cannot access items in a non-batching job, call "
                "make_params with num_items parameter"
            )

    def as_dict(self, validate=True) -> Dict[str, Any]:
        """
        Constructs a dictionary from the input params.

        For batching jobs, top-level entries are merged into item entries.
        Item entries have priority in case they are specified.
        """

        # initialize result and set type hint
        result: Dict[str, Any] = EstimatorInputParamsItem.as_dict(self, validate)

        if self.has_items:
            result["items"] = [item.as_dict(validate, self) for item in self._items]
            # In case of batching, no need to stop if failing an item
            result["resumeAfterFailedItem"] = True

        return result


class HTMLWrapper:
    """
    Simple HTML wrapper to expose _repr_html_ for Jupyter clients.
    """

    def __init__(self, content: str):
        self.content = content

    def _repr_html_(self):
        return self.content


class EstimatorResult(dict):
    """
    Microsoft Resource Estimator result.

    The class represents simple resource estimation results as well as batching
    resource estimation results.  The latter can be indexed by an integer index to
    access an individual result from the batching result.
    """

    MAX_DEFAULT_ITEMS_IN_TABLE = 5

    def __init__(self, data: Union[Dict, List]):
        self._error = None

        if isinstance(data, list) and len(data) == 1:
            data = data[0]
            if not EstimatorResult._is_succeeded(data):
                raise EstimatorError(data["code"], data["message"])

        if isinstance(data, dict):
            self._data = data
            super().__init__(data)

            self._is_simple = True
            if EstimatorResult._is_succeeded(self):
                self._repr = self._item_result_table()
                self.summary = HTMLWrapper(self._item_result_summary_table())
                self.diagram = EstimatorResultDiagram(self.data().copy())
            else:
                self._error = EstimatorError(data["code"], data["message"])

        elif isinstance(data, list):
            super().__init__(
                {idx: EstimatorResult(item_data) for idx, item_data in enumerate(data)}
            )

            self._data = data
            self._is_simple = False
            num_items = len(data)
            self._repr = ""
            if num_items > self.MAX_DEFAULT_ITEMS_IN_TABLE:
                self._repr += (
                    "<p><b>Info:</b> <i>The overview table is "
                    "cut off after "
                    f"{self.MAX_DEFAULT_ITEMS_IN_TABLE} items. If "
                    "you want to see all items, suffix the result "
                    "variable with <code>[:]</code></i></p>"
                )
                num_items = self.MAX_DEFAULT_ITEMS_IN_TABLE
            self._repr += self._batch_result_table(range(num_items))

            # Add plot function for batching jobs
            self.plot = self._plot
            self.summary_data_frame = self._summary_data_frame

    def _is_succeeded(self):
        return "status" in self and self["status"] == "success"

    def data(self, idx: Optional[int] = None) -> Any:
        """
        Returns raw data of the result object.

        In case of a batching job, you can pass an index to access a specific
        item.
        """
        if idx is None:
            return self._data
        elif not self._is_simple:
            return self._data[idx]
        else:
            msg = "Cannot pass parameter 'idx' to 'data' for non-batching job"
            raise ValueError(msg)

    @property
    def error(self) -> Optional[EstimatorError]:
        """
        Returns the error object if the result is an error.
        """
        return self._error

    @property
    def logical_counts(self):
        """
        Returns the logical counts of the result.
        """
        if self._is_simple:
            return LogicalCounts(self.data()["logicalCounts"])
        else:
            return LogicalCounts(self.data(0)["logicalCounts"])

    def _repr_html_(self):
        """
        HTML table representation of the result.
        """
        if self._error:
            raise self._error
        return self._repr

    def __getitem__(self, key):
        """
        If the result represents a batching job and key is a slice, a
        side-by-side table comparison is shown for the indexes represented by
        the slice.

        Otherwise, the key is used to access the raw data directly.
        """
        if isinstance(key, slice):
            if self._is_simple:
                msg = "Cannot pass slice to '__getitem__' for non-batching job"
                raise ValueError(msg)
            return HTMLWrapper(self._batch_result_table(range(len(self))[key]))
        else:
            if super().__contains__(key):
                return super().__getitem__(key)
            elif super().__contains__("frontierEntries"):
                return super().__getitem__("frontierEntries")[0].__getitem__(key)
            else:
                raise KeyError(key)

    def _plot(self, **kwargs):
        """
        Plots all result items in a space time plot, where the x-axis shows
        total runtime, and the y-axis shows total number of physical qubits.
        Both axes are in log-scale.
        Attributes:
            labels (list): List of labels for the legend.
        """
        try:
            import matplotlib.pyplot as plt
        except ImportError:
            raise ImportError(
                "Missing optional 'matplotlib' dependency. To install run: "
                "pip install matplotlib"
            )

        labels = kwargs.pop("labels", [])

        [xs, ys] = zip(
            *[
                (
                    self.data(i)["physicalCounts"]["runtime"],
                    self.data(i)["physicalCounts"]["physicalQubits"],
                )
                for i in range(len(self))
            ]
        )

        _ = plt.figure(figsize=(15, 8))

        plt.ylabel("Physical qubits")
        plt.xlabel("Runtime")
        plt.loglog()
        for i, (x, y) in enumerate(zip(xs, ys)):
            if isinstance(labels, list) and i < len(labels):
                label = labels[i]
            else:
                label = str(i)
            plt.scatter(x=[x], y=[y], label=label, marker="os+x"[i % 4])

        nsec = 1
        usec = 1e3 * nsec
        msec = 1e3 * usec
        sec = 1e3 * msec
        min = 60 * sec
        hour = 60 * min
        day = 24 * hour
        week = 7 * day
        month = 31 * day
        year = 365 * month
        decade = 10 * year
        century = 10 * decade

        time_units = [
            nsec,
            usec,
            msec,
            sec,
            min,
            hour,
            day,
            week,
            month,
            year,
            decade,
            century,
        ]
        time_labels = [
            "1 ns",
            "1 µs",
            "1 ms",
            "1 s",
            "1 min",
            "1 hour",
            "1 day",
            "1 week",
            "1 month",
            "1 year",
            "1 decade",
            "1 century",
        ]

        cutoff = (
            next(
                (i for i, x in enumerate(time_units) if x > max(xs)),
                len(time_units) - 1,
            )
            + 1
        )

        plt.xticks(time_units[0:cutoff], time_labels[0:cutoff], rotation=90)
        plt.legend(loc="upper left")
        plt.show()

    @property
    def json(self):
        """
        Returns a JSON representation of the resource estimation result data.
        """
        if not hasattr(self, "_json"):
            import json

            self._json = json.dumps(self._data)

        return self._json

    def _summary_data_frame(self, **kwargs):
        try:
            import pandas as pd
        except ImportError:
            raise ImportError(
                "Missing optional 'pandas' dependency. To install run: "
                "pip install pandas"
            )

        # get labels or use default value, then extend with missing elements,
        # and truncate extra elements
        labels = kwargs.pop("labels", [])
        labels.extend(range(len(labels), len(self)))
        labels = labels[: len(self)]

        def get_row(result):
            if EstimatorResult._is_succeeded(result):
                formatted = result["physicalCountsFormatted"]

                return (
                    formatted["algorithmicLogicalQubits"],
                    formatted["logicalDepth"],
                    formatted["numTstates"],
                    result["logicalQubit"]["codeDistance"],
                    formatted["numTfactories"],
                    formatted["physicalQubitsForTfactoriesPercentage"],
                    formatted["physicalQubits"],
                    formatted["rqops"],
                    formatted["runtime"],
                )
            else:
                return ["No solution found"] * 9

        data = [get_row(self.data(index)) for index in range(len(self))]
        columns = [
            "Logical qubits",
            "Logical depth",
            "T states",
            "Code distance",
            "T factories",
            "T factory fraction",
            "Physical qubits",
            "rQOPS",
            "Physical runtime",
        ]
        return pd.DataFrame(data, columns=columns, index=labels)

    def _item_result_table(self):
        html = ""

        if has_markdown:
            md = markdown.Markdown(extensions=["mdx_math"])
        for group in self["reportData"]["groups"]:
            html += f"""
                <details {"open" if group['alwaysVisible'] else ""}>
                    <summary style="display:list-item">
                        <strong>{group['title']}</strong>
                    </summary>
                    <table>"""
            for entry in group["entries"]:
                val = self
                for key in entry["path"].split("/"):
                    if key not in val and "frontierEntries" in val:
                        val = val["frontierEntries"][0]
                    val = val[key]
                if has_markdown:
                    explanation = md.convert(entry["explanation"])
                else:
                    explanation = entry["explanation"]
                html += f"""
                    <tr>
                        <td style="font-weight: bold; vertical-align: top; white-space: nowrap">{entry['label']}</td>
                        <td style="vertical-align: top; white-space: nowrap">{val}</td>
                        <td style="text-align: left">
                            <strong>{entry["description"]}</strong>
                            <hr style="margin-top: 2px; margin-bottom: 0px; border-top: solid 1px black" />
                            {explanation}
                        </td>
                    </tr>
                """
            html += "</table></details>"

        html += f'<details><summary style="display:list-item"><strong>Assumptions</strong></summary><ul>'
        if has_markdown:
            for assumption in self["reportData"]["assumptions"]:
                html += f"<li>{md.convert(assumption)}</li>"
        html += "</ul></details>"

        return html

    def _item_result_summary_table(self):
        html = """
            <style>
                .aqre-tooltip {
                    position: relative;
                    border-bottom: 1px dotted black;
                }

                .aqre-tooltip .aqre-tooltiptext {
                    font-weight: normal;
                    visibility: hidden;
                    width: 600px;
                    background-color: #e0e0e0;
                    color: black;
                    text-align: center;
                    border-radius: 6px;
                    padding: 5px 5px;
                    position: absolute;
                    z-index: 1;
                    top: 150%;
                    left: 50%;
                    margin-left: -200px;
                    border: solid 1px black;
                }

                .aqre-tooltip .aqre-tooltiptext::after {
                    content: "";
                    position: absolute;
                    bottom: 100%;
                    left: 50%;
                    margin-left: -5px;
                    border-width: 5px;
                    border-style: solid;
                    border-color: transparent transparent black transparent;
                }

                .aqre-tooltip:hover .aqre-tooltiptext {
                    visibility: visible;
                }
            </style>"""

        if has_markdown:
            md = markdown.Markdown(extensions=["mdx_math"])
        for group in self["reportData"]["groups"]:
            html += f"""
                <details {"open" if group['alwaysVisible'] else ""}>
                    <summary style="display:list-item">
                        <strong>{group['title']}</strong>
                    </summary>
                    <table>"""
            for entry in group["entries"]:
                val = self
                for key in entry["path"].split("/"):
                    val = val[key]
                if has_markdown:
                    explanation = md.convert(entry["explanation"])
                else:
                    explanation = entry["explanation"]
                html += f"""
                    <tr class="aqre-tooltip">
                        <td style="font-weight: bold"><span class="aqre-tooltiptext">{explanation}</span>{entry['label']}</td>
                        <td>{val}</td>
                        <td style="text-align: left">{entry["description"]}</td>
                    </tr>
                """
            html += "</table></details>"

        html += f"<details><summary style='display:list-item'><strong>Assumptions</strong></summary><ul>"
        if has_markdown:
            for assumption in self["reportData"]["assumptions"]:
                html += f"<li>{md.convert(assumption)}</li>"
        html += "</ul></details>"

        return html

    def _batch_result_table(self, indices):
        succeeded_item_indices = [
            i for i in indices if EstimatorResult._is_succeeded(self[i])
        ]
        if len(succeeded_item_indices) == 0:
            print("None of the jobs succeeded")
            return ""

        first_succeeded_item_index = succeeded_item_indices[0]

        html = ""

        if has_markdown:
            md = markdown.Markdown(extensions=["mdx_math"])

        item_headers = "".join(f"<th>{i}</th>" for i in indices)

        for group_index, group in enumerate(
            self[first_succeeded_item_index]["reportData"]["groups"]
        ):
            html += f"""
                <details {"open" if group['alwaysVisible'] else ""}>
                    <summary style="display:list-item">
                        <strong>{group['title']}</strong>
                    </summary>
                    <table>
                        <thead><tr><th>Item</th>{item_headers}</tr></thead>"""

            visited_entries = set()

            for entry in [
                entry
                for index in succeeded_item_indices
                for entry in self[index]["reportData"]["groups"][group_index]["entries"]
            ]:
                label = entry["label"]
                if label in visited_entries:
                    continue
                visited_entries.add(label)

                html += f"""
                    <tr>
                        <td style="font-weight: bold; vertical-align: top; white-space: nowrap">{label}</td>
                """

                for index in indices:
                    val = self[index]
                    if index in succeeded_item_indices:
                        for key in entry["path"].split("/"):
                            if key in val:
                                val = val[key]
                            else:
                                val = "N/A"
                                break
                    else:
                        val = "N/A"
                    html += f"""
                            <td style="vertical-align: top; white-space: nowrap">{val}</td>
                    """

                html += """
                    </tr>
                """
            html += "</table></details>"

        html += f'<details><summary style="display:list-item"><strong>Assumptions</strong></summary><ul>'
        if has_markdown:
            for assumption in self[0]["reportData"]["assumptions"]:
                html += f"<li>{md.convert(assumption)}</li>"
        html += "</ul></details>"

        return html

    @staticmethod
    def _is_succeeded(obj):
        return "status" in obj and obj["status"] == "success"


class EstimatorResultDiagram:
    def __init__(self, data):
        data.pop("reportData")
        self.data_json = json.dumps(data).replace(" ", "")
        self.vis_lib = "https://cdn-aquavisualization-prod.azureedge.net/resource-estimation/index.js"
        self.space = HTMLWrapper(self._space_diagram())
        self.time = HTMLWrapper(self._time_diagram())

    def _space_diagram(self):
        html = f"""
            <script src={self.vis_lib}></script>
            <re-space-diagram data={self.data_json}></re-space-diagram>"""
        return html

    def _time_diagram(self):
        html = f"""
            <script src={self.vis_lib}></script>
            <re-time-diagram data={self.data_json}></re-time-diagram>"""
        return html


class LogicalCounts(dict):
    """
    Microsoft Resource Estimator Logical Counts.

    The class represents logical counts that can be used as input to physical estimation of resources
    in the Microsoft Resource Estimator.
    """

    def __init__(self, data: Dict):
        self._data = {}
        self._data["numQubits"] = data.get("numQubits", 0)
        self._data["tCount"] = data.get("tCount", 0)
        self._data["rotationCount"] = data.get("rotationCount", 0)
        self._data["rotationDepth"] = data.get("rotationDepth", 0)
        self._data["cczCount"] = data.get("cczCount", 0)
        self._data["ccixCount"] = data.get("ccixCount", 0)
        self._data["measurementCount"] = data.get("measurementCount", 0)
        if "numComputeQubits" in data:
            self._data["numComputeQubits"] = data["numComputeQubits"]
        if "readFromMemoryCount" in data:
            self._data["readFromMemoryCount"] = data["readFromMemoryCount"]
        if "writeToMemoryCount" in data:
            self._data["writeToMemoryCount"] = data["writeToMemoryCount"]
        super().__init__(self._data)

    @property
    def json(self):
        """
        Returns a JSON representation of the logical counts.
        """
        if not hasattr(self, "_json"):
            import json

            self._json = json.dumps(self._data)

        return self._json

    def estimate(
        self, params: Union[dict, List, EstimatorParams] = None
    ) -> EstimatorResult:
        """
        Estimates resources for the current logical counts, using the
        Parallel Synthesis Sequential Pauli Computation (PSSPC) layout method.

        :param logical_counts: The logical counts.
        :param params: The parameters to configure physical estimation.

        :returns resources: The estimated resources.
        """
        if params is None:
            params = [{}]
        elif isinstance(params, EstimatorParams):
            if params.has_items:
                params = params.as_dict()["items"]
            else:
                params = [params.as_dict()]
        elif isinstance(params, dict):
            params = [params]
        return EstimatorResult(
            json.loads(physical_estimates(self.json, json.dumps(params)))
        )
