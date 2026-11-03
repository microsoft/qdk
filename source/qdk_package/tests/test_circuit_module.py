# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import subprocess
import sys
import warnings

import pytest


def _run_in_fresh_interpreter(source: str) -> subprocess.CompletedProcess:
    """Runs source in a new interpreter.

    The import warning fires once per interpreter, and re-importing the module
    in-process rebuilds ``ExperimentalWarning`` as a distinct class, so an
    in-process check would compare against a class the warning was not raised
    with. A fresh interpreter is what a user actually experiences.
    """
    return subprocess.run(
        [sys.executable, "-W", "always", "-c", source],
        capture_output=True,
        text=True,
        check=False,
    )


def test_import_emits_experimental_warning():
    result = _run_in_fresh_interpreter("import qdk.circuit")
    assert result.returncode == 0, result.stderr
    assert "ExperimentalWarning" in result.stderr
    assert "experimental" in result.stderr.lower()


def test_import_warning_can_be_silenced_by_message_before_import():
    # The documented escape hatch, which has to work before the import runs.
    result = _run_in_fresh_interpreter(
        "import warnings\n"
        "warnings.filterwarnings('ignore', message='qdk.circuit is experimental')\n"
        "import qdk.circuit\n"
    )
    assert result.returncode == 0, result.stderr
    assert "ExperimentalWarning" not in result.stderr


def test_import_warns_once_not_per_attribute_access():
    result = _run_in_fresh_interpreter(
        "import qdk.circuit\n"
        "import qdk.circuit\n"
        "qdk.circuit.ExperimentalWarning\n"
    )
    assert result.returncode == 0, result.stderr
    assert result.stderr.count("ExperimentalWarning:") == 1


def test_experimental_warning_is_a_filterable_user_warning_subclass():
    from qdk.circuit import ExperimentalWarning

    assert issubclass(ExperimentalWarning, UserWarning)
    assert ExperimentalWarning is not UserWarning

    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always")
        warnings.filterwarnings("ignore", category=ExperimentalWarning)
        warnings.warn("post-import experimental notice", ExperimentalWarning)
        warnings.warn("unrelated", UserWarning)

    assert [w.category for w in caught] == [UserWarning]


def test_module_is_reachable_from_the_single_import_root():
    import qdk.circuit

    assert qdk.circuit.__name__ == "qdk.circuit"


if __name__ == "__main__":
    pytest.main([__file__])
