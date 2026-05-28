# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.


import pytest

from qdk import TargetProfile
from utils import (
    assert_strings_equal_ignore_line_endings,
    compile_qsharp,
    get_input_files,
    get_output_ll_file,
    get_output_out_file,
    QIR_RUNNER_AVAILABLE,
    read_file,
    save_qir_to_temp_file_and_execute,
    SKIP_REASON,
)

TARGET_PROFILE = TargetProfile.Adaptive_RIFLA


# This function is used to generate the expected output files for the tests
# Rename the function to start with test_ to generate the expected output files
def generate_test_outputs():
    from utils import generate_test_outputs

    generate_test_outputs(TARGET_PROFILE)


@pytest.mark.parametrize("file_path", get_input_files(TARGET_PROFILE))
@pytest.mark.skipif(not QIR_RUNNER_AVAILABLE, reason=SKIP_REASON)
def test_adaptive_rifla_qir(file_path: str) -> None:
    source = read_file(file_path, TARGET_PROFILE)
    ll_file_path = get_output_ll_file(file_path, TARGET_PROFILE)
    expected_qir = read_file(ll_file_path, TARGET_PROFILE)
    actual_qir = compile_qsharp(source, TARGET_PROFILE)
    assert actual_qir == expected_qir


@pytest.mark.parametrize("file_path", get_input_files(TARGET_PROFILE))
@pytest.mark.skipif(not QIR_RUNNER_AVAILABLE, reason=SKIP_REASON)
def test_adaptive_rifla_output(file_path: str) -> None:
    source = read_file(file_path, TARGET_PROFILE)
    qir = compile_qsharp(source, TARGET_PROFILE)
    output_file_path = get_output_out_file(file_path, TARGET_PROFILE)
    expected_output = read_file(output_file_path, TARGET_PROFILE)
    actual_output = save_qir_to_temp_file_and_execute(qir)
    assert_strings_equal_ignore_line_endings(actual_output, expected_output)
