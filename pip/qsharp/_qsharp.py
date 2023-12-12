# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from typing import Any, Dict, Optional, Union, List
from ._native import Interpreter, Output, TargetProfile, StateDump
from .estimator._estimator import EstimatorResult, EstimatorParams
import json

_interpreter = None


class Config:
    _config: Dict[str, str]
    """
    Configuration hints for the language service.
    """

    def __init__(self, target_profile: TargetProfile):
        if target_profile == TargetProfile.Unrestricted:
            self._config = {"targetProfile": "unrestricted"}
        elif target_profile == TargetProfile.Base:
            self._config = {"targetProfile": "base"}

    def __repr__(self) -> str:
        return "Q# initialized with configuration: " + str(self._config)

    # See https://ipython.readthedocs.io/en/stable/config/integrating.html#rich-display
    # See https://ipython.org/ipython-doc/3/notebook/nbformat.html#display-data
    # This returns a custom MIME-type representation of the Q# configuration.
    # This data will be available in the cell output, but will not be displayed
    # to the user, as frontends would not know how to render the custom MIME type.
    # Editor services that interact with the notebook frontend
    # (i.e. the language service) can read and interpret the data.
    def _repr_mimebundle_(
        self, include: Any | None = None, exclude: Any | None = None
    ) -> Dict[str, Dict[str, str]]:
        return {"application/x.qsharp-config": self._config}


def init(target_profile: TargetProfile = TargetProfile.Unrestricted) -> Config:
    """
    Initializes the Q# interpreter.

    :param target_profile: Setting the target profile allows the Q#
        interpreter to generate programs that are compatible
        with a specific target. See :py:class: `qsharp.TargetProfile`.
    """
    global _interpreter
    _interpreter = Interpreter(target_profile)
    # Return the configuration information to provide a hint to the
    # language service through the cell output.
    return Config(target_profile)


def get_interpreter() -> Interpreter:
    """
    Returns the Q# interpreter.

    :returns: The Q# interpreter.
    """
    global _interpreter
    if _interpreter is None:
        init()
    return _interpreter


def eval(source: str) -> Any:
    """
    Evaluates Q# source code.

    Output is printed to console.

    :param source: The Q# source code to evaluate.
    :returns value: The value returned by the last statement in the source code.
    :raises QSharpError: If there is an error evaluating the source code.
    """

    def callback(output: Output) -> None:
        print(output)

    return get_interpreter().interpret(source, callback)


def eval_file(path: str) -> Any:
    """
    Reads Q# source code from a file and evaluates it.

    :param path: The path to the Q# source file.
    :returns: The value returned by the last statement in the file.
    :raises: QSharpError
    """
    f = open(path, mode="r", encoding="utf-8")
    return eval(f.read())


def run(entry_expr: str, shots: int) -> Any:
    """
    Runs the given Q# expression for the given number of shots.
    Each shot uses an independent instance of the simulator.

    :param entry_expr: The entry expression.
    :param shots: The number of shots to run.

    :returns values: A list of results or runtime errors.

    :raises QSharpError: If there is an error interpreting the input.
    """

    def callback(output: Output) -> None:
        print(output)

    return get_interpreter().run(entry_expr, shots, callback)


# Class that wraps generated QIR, which can be used by
# azure-quantum as input data.
#
# This class must implement the QirRepresentable protocol
# that is defined by the azure-quantum package.
# See: https://github.com/microsoft/qdk-python/blob/fcd63c04aa871e49206703bbaa792329ffed13c4/azure-quantum/azure/quantum/target/target.py#L21
class QirInputData:
    # The name of this variable is defined
    # by the protocol and must remain unchanged.
    _name: str

    def __init__(self, name: str, ll_str: str):
        self._name = name
        self._ll_str = ll_str

    # The name of this method is defined
    # by the protocol and must remain unchanged.
    def _repr_qir_(self, **kwargs) -> bytes:
        return self._ll_str.encode("utf-8")

    def __str__(self) -> str:
        return self._ll_str


def compile(entry_expr: str) -> QirInputData:
    """
    Compiles the Q# source code into a program that can be submitted to a target.

    :param entry_expr: The Q# expression that will be used as the entrypoint
        for the program.

    :returns QirInputData: The compiled program.

    To get the QIR string from the compiled program, use `str()`.

    Example:

    .. code-block:: python
        program = qsharp.compile("...")
        with open('myfile.ll', 'w') as file:
            file.write(str(program))
    """
    ll_str = get_interpreter().qir(entry_expr)
    return QirInputData("main", ll_str)


def estimate(
    entry_expr, params: Optional[Union[Dict[str, Any], List, EstimatorParams]] = None
) -> EstimatorResult:
    """
    Estimates resources for Q# source code.

    :param entry_expr: The entry expression.
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
        json.loads(get_interpreter().estimate(entry_expr, json.dumps(params)))
    )


def dump_machine() -> StateDump:
    """
    Returns the sparse state vector of the simulator as a StateDump object.

    :returns: The state of the simulator.
    """
    return get_interpreter().dump_machine()
