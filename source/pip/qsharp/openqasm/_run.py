# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from time import monotonic
from typing import Any, Callable, Dict, List, Optional, Tuple, Union
from .._fs import read_file, list_directory, resolve
from .._http import fetch_github
from .._native import QasmError, Output, run_qasm_program  # type: ignore
from .._qsharp import (
    BitFlipNoise,
    DepolarizingNoise,
    PauliNoise,
    PhaseFlipNoise,
    ShotResult,
    StateDump,
    get_interpreter,
    ipython_helper,
    python_args_to_interpreter_args,
)
from .. import telemetry_events
from ._ipython import display_or_print


def run(
    source: Union[str, Callable],
    shots: int = 1024,
    *args: Any,
    on_result: Optional[Callable[[ShotResult], None]] = None,
    save_events: bool = False,
    noise: Optional[
        Union[
            Tuple[float, float, float],
            PauliNoise,
            BitFlipNoise,
            PhaseFlipNoise,
            DepolarizingNoise,
        ]
    ] = None,
    qubit_loss: Optional[float] = None,
    as_bitstring: bool = False,
    **kwargs: Any,
) -> List[Any]:
    """
    Runs the given OpenQASM program for the given number of shots.
    Either a full program or a callable with arguments must be provided.
    Each shot uses an independent instance of the simulator.

    Args:
        source (str): An OpenQASM program. Alternatively, a callable can be provided,
            which must be an already imported global callable.
        shots: The number of shots to run, Defaults to 1024.
        *args: The arguments to pass to the callable, if one is provided.
        on_result: A callback function that will be called with each result. Only used when a callable is provided.
        save_events: If true, the output of each shot will be saved. If false, they will be printed. Only used when a callable is provided.
        noise: The noise to use in simulation.
        qubit_loss: The probability of qubit loss in simulation.
        as_bitstring: If true, the result registers will be converted to bitstrings.
        **kwargs: Additional keyword arguments to pass to the compilation when source program is provided.
          - name (str): The name of the circuit. This is used as the entry point for the program.
          - target_profile (TargetProfile): The target profile to use for code generation.
          - search_path (Optional[str]): The optional search path for resolving file references.
          - output_semantics (OutputSemantics, optional): The output semantics for the compilation.
          - seed (int): The seed to use for the random number generator.

    Returns:
        values: A list of results or runtime errors. If `save_events` is true,
            a List of ShotResults is returned.

    Raises:
        QasmError: If there is an error generating, parsing, or analyzing the OpenQASM source.
        QSharpError: If there is an error interpreting the input.
        ValueError: If the number of shots is less than 1.
        ValueError: If the `on_result` and `save_events` parameters are used when running OpenQASM programs.
    """

    ipython_helper()

    if shots < 1:
        raise ValueError("The number of shots must be greater than 0.")

    telemetry_events.on_run_qasm(
        shots, noise=noise is not None, qubit_loss=qubit_loss is not None
    )
    start_time = monotonic()

    results: List[ShotResult] = []

    def on_save_events(output: Output) -> None:
        # Append the output to the last shot's output list
        results[-1]["events"].append(output)
        if output.is_matrix():
            results[-1]["matrices"].append(output)
        elif output.is_state_dump():
            dump_data = output.state_dump()
            if dump_data is not None:
                results[-1]["dumps"].append(StateDump(dump_data))
        elif output.is_message():
            results[-1]["messages"].append(str(output))

    callable = None
    if isinstance(source, Callable) and hasattr(source, "__global_callable"):
        args = python_args_to_interpreter_args(args)
        callable = source.__global_callable
        source = None

    if callable:
        for _ in range(shots):
            results.append(
                {
                    "result": None,
                    "events": [],
                    "matrices": [],
                    "dumps": [],
                    "messages": [],
                }
            )
            run_results = get_interpreter().run(
                source,
                on_save_events if save_events else display_or_print,
                noise,
                qubit_loss=qubit_loss,
                callable=callable,
                args=args,
            )
            results[-1]["result"] = run_results

            if on_result:
                on_result(results[-1])

        if not save_events:
            # If we are not saving events, we can just return the results
            # as a list of results.
            results = [result["result"] for result in results]
    else:
        # running the QASM program in isolation means we can't use the
        # interpreter to run the program, so we can't cache the compilation
        # results. This means we need to compile the program for each
        # shot, or we push the shots into the QASM program and compile it once.
        #
        # This breaks the output streaming and event saving.
        if on_result or save_events:
            raise QasmError(
                "The `on_result` and `save_events` parameters are not supported when running QASM programs."
            )

        # remove any entries from kwargs with a None key or None value
        kwargs = {k: v for k, v in kwargs.items() if k is not None and v is not None}

        if "search_path" not in kwargs:
            kwargs["search_path"] = "."

        kwargs["shots"] = shots

        results = run_qasm_program(
            source,
            display_or_print,
            noise,
            qubit_loss,
            read_file,
            list_directory,
            resolve,
            fetch_github,
            **kwargs,
        )

    durationMs = (monotonic() - start_time) * 1000
    telemetry_events.on_run_qasm_end(durationMs, shots)

    if as_bitstring:
        from ._utils import as_bitstring as convert_to_bitstring

        results = convert_to_bitstring(results)

    return results
