# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from textwrap import dedent
import pytest
import qdk as qsharp
import qdk.code
from contextlib import redirect_stdout
import io

from qdk import TargetProfile

# pull in from native module for tests so that we don't have to install qiskit
# using the interop module
from qdk._native import OutputSemantics, ProgramType


def test_target_profile_int_values_match_enum_values() -> None:
    assert 0 == TargetProfile.Base
    assert 1 == TargetProfile.Adaptive_RI
    assert 2 == TargetProfile.Adaptive_RIF
    assert 3 == TargetProfile.Adaptive_RIFLA
    assert 4 == TargetProfile.Unrestricted


def test_target_profile_serialization() -> None:
    input = [
        TargetProfile.Base,
        TargetProfile.Adaptive_RI,
        TargetProfile.Adaptive_RIF,
        TargetProfile.Unrestricted,
    ]
    import pickle

    ser = pickle.dumps(input)
    de = pickle.loads(ser)
    assert de == input


def test_target_profile_str_values_match_enum_values() -> None:
    target_profile = TargetProfile.Base
    str_value = str(target_profile)
    assert str_value == "Base"
    target_profile = TargetProfile.Adaptive_RI
    str_value = str(target_profile)
    assert str_value == "Adaptive_RI"
    target_profile = TargetProfile.Adaptive_RIF
    str_value = str(target_profile)
    assert str_value == "Adaptive_RIF"
    target_profile = TargetProfile.Adaptive_RIFLA
    str_value = str(target_profile)
    assert str_value == "Adaptive_RIFLA"
    target_profile = TargetProfile.Unrestricted
    str_value = str(target_profile)
    assert str_value == "Unrestricted"


def test_target_profile_from_str_match_enum_values() -> None:
    target_profile = TargetProfile.Base
    str_value = str(target_profile)
    assert TargetProfile.from_str(str_value) == target_profile
    target_profile = TargetProfile.Adaptive_RI
    str_value = str(target_profile)
    assert TargetProfile.from_str(str_value) == target_profile
    target_profile = TargetProfile.Adaptive_RIF
    str_value = str(target_profile)
    assert TargetProfile.from_str(str_value) == target_profile
    target_profile = TargetProfile.Adaptive_RIFLA
    str_value = str(target_profile)
    assert TargetProfile.from_str(str_value) == target_profile
    target_profile = TargetProfile.Unrestricted
    str_value = str(target_profile)
    assert TargetProfile.from_str(str_value) == target_profile
    with pytest.raises(ValueError):
        TargetProfile.from_str("Invalid")


def test_output_semantics_int_values_match_enum_values() -> None:
    assert 0 == OutputSemantics.Qiskit
    assert 1 == OutputSemantics.OpenQasm
    assert 2 == OutputSemantics.ResourceEstimation


def test_output_semantics_serialization() -> None:
    input = [
        OutputSemantics.Qiskit,
        OutputSemantics.OpenQasm,
        OutputSemantics.ResourceEstimation,
    ]
    import pickle

    ser = pickle.dumps(input)
    de = pickle.loads(ser)
    assert de == input


def test_program_type_int_values_match_enum_values() -> None:
    assert 0 == ProgramType.File
    assert 1 == ProgramType.Operation
    assert 2 == ProgramType.Fragments


def test_program_type_serialization() -> None:
    input = [
        ProgramType.File,
        ProgramType.Operation,
        ProgramType.Fragments,
    ]
    import pickle

    ser = pickle.dumps(input)
    de = pickle.loads(ser)
    assert de == input
