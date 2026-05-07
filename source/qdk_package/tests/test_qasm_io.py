# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from math import pi
import pytest
from qdk import (
    QSharpError,
    init,
    TargetProfile,
    Result,
)
from qdk.openqasm import (
    import_openqasm,
    ProgramType,
)
import qdk.code as code


def test_import_unsupported_angle_input_type() -> None:
    source = """
        input angle input_var;
        """
    init(target_profile=TargetProfile.Base)
    with pytest.raises(QSharpError):
        import_openqasm(source, program_type=ProgramType.Operation, name="program")


def test_import_supported_angle_output_type() -> None:
    source = """
        input float input_var;
        output angle output_var;
        output_var = input_var;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    input = 1.0
    output = code.program(input)
    assert abs(output - input) < 1e-10

    input = 0.0
    output = code.program(input)
    assert abs(output - input) < 1e-10

    # 2pi wraps the angle type and should come back as ~0.0
    input = 2 * pi
    output = code.program(input)
    assert abs(output) < 1e-10


def test_import_supported_bit_input_and_output_types() -> None:
    source = """
        input bit input_var;
        output bit output_var;
        output_var = input_var;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    input = Result.One
    assert code.program(input) == input
    input = Result.Zero
    assert code.program(input) == input

    input = True
    with pytest.raises(TypeError):
        assert code.program(input) == input
    input = False
    with pytest.raises(TypeError):
        assert code.program(input) == input
    input = 1
    with pytest.raises(TypeError):
        assert code.program(input) == input
    input = 0
    with pytest.raises(TypeError):
        assert code.program(input) == input


def test_import_supported_bool_input_and_output_types() -> None:
    source = """
        input bool input_var;
        output bool output_var;
        output_var = input_var;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    input = True
    assert code.program(input) == input
    input = False
    assert code.program(input) == input

    input = Result.One
    with pytest.raises(TypeError):
        assert code.program(input) == input
    input = Result.Zero
    with pytest.raises(TypeError):
        assert code.program(input) == input
    input = 1
    with pytest.raises(TypeError):
        assert code.program(input) == input
    input = 0
    with pytest.raises(TypeError):
        assert code.program(input) == input


def test_import_supported_complex_input_and_output_types() -> None:
    source = """
        input complex input_var;
        output complex output_var;
        output_var = input_var;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    input = 1.0
    output = code.program(input)
    assert output == input
    input = 1.0j
    assert code.program(input) == input
    input = 1.0 + 2.0j
    assert code.program(input) == input


def test_import_supported_float_input_and_output_types() -> None:
    source = """
        input float input_var;
        output float output_var;
        output_var = input_var;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    input = 1.0
    assert code.program(input) == input
    input = 0.0
    assert code.program(input) == input

    input = 1
    code.program(input) == float(input)
    input = 0
    code.program(input) == float(input)


def test_import_supported_int_input_and_output_types() -> None:
    source = """
        input int input_var;
        output int output_var;
        output_var = input_var;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    input = 1
    assert code.program(input) == input
    input = 0
    assert code.program(input) == input
    input = -1
    assert code.program(input) == input


def test_import_supported_bigint_input_and_output_types() -> None:
    source = """
        input int[128] input_var;
        output int[128] output_var;
        output_var = input_var;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    input = 1
    assert code.program(input) == input
    input = 0
    assert code.program(input) == input
    input = -1
    assert code.program(input) == input
    input = (1 << 64) - 1
    assert code.program(input) == input
    input = -(1 << 63)
    assert code.program(input) == input
    input = (1 << 128) - 1
    assert code.program(input) == input
    input = -(1 << 127)
    assert code.program(input) == input


def test_import_supported_uint_input_and_output_types() -> None:
    source = """
        input uint input_var;
        output uint output_var;
        output_var = input_var;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    input = 1
    assert code.program(input) == input
    input = 0
    assert code.program(input) == input
    input = -1
    # we don't have uint, so it goes in as-is
    assert code.program(input) == input


def test_import_supported_biguint_input_and_output_types() -> None:
    source = """
        input uint[128] input_var;
        output uint[128] output_var;
        output_var = input_var;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    input = 1
    assert code.program(input) == input
    input = 0
    assert code.program(input) == input
    input = -1
    assert code.program(input) == input
    input = (1 << 64) - 1
    assert code.program(input) == input
    # we don't have uint, so it goes in as-is
    input = -(1 << 63)
    assert code.program(input) == input
    input = (1 << 128) - 1
    assert code.program(input) == input
    # we don't have uint, so it goes in as-is
    input = -(1 << 128)
    assert code.program(input) == input


def test_import_supported_bitarray_input_and_output_types() -> None:
    source = """
        input bit[5] i_0;
        output bit[5] o_0;
        o_0 = i_0;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    with pytest.raises(TypeError):
        code.program([1, 0, 1, 0, 0])
    with pytest.raises(TypeError):
        code.program([True, False, True, False, False])
    with pytest.raises(QSharpError):
        # invalid size
        code.program([Result.One, Result.Zero, Result.Zero])

    input = [Result.One, Result.Zero, Result.One, Result.Zero, Result.Zero]
    assert code.program(input) == input


def test_import_supported_bool_array_input_and_output_types() -> None:
    source = """
        input array[bool, 5] i_0;
        output array[bool, 5] o_0;
        o_0 = i_0;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    with pytest.raises(TypeError):
        code.program([1, 0, 1, 0, 0])
    with pytest.raises(TypeError):
        code.program([Result.One, Result.Zero, Result.One, Result.Zero, Result.Zero])
    with pytest.raises(QSharpError):
        # invalid size
        code.program([True, False, False])

    input = [True, False, True, False, False]
    assert code.program(input) == input


def test_import_supported_bool_array_array_input_and_output_types() -> None:
    source = """
        input array[bool, 5, 2] i_0;
        output array[bool, 5, 2] o_0;
        o_0 = i_0;
        """
    init(target_profile=TargetProfile.Base)
    import_openqasm(source, program_type=ProgramType.Operation, name="program")

    with pytest.raises(TypeError):
        code.program([[1, 1], [0, 0], [1, 0], [0, 0]])
    with pytest.raises(TypeError):
        # right shape, wrong type
        code.program(
            [
                [Result.One, Result.Zero],
                [Result.One, Result.Zero],
                [Result.Zero, Result.Zero],
                [Result.Zero, Result.Zero],
                [Result.Zero, Result.Zero],
            ]
        )
    with pytest.raises(QSharpError):
        # invalid size
        code.program([[True, True], [False, False], [False, False]])

    input = [
        [True, False],
        [False, False],
        [True, False],
        [False, False],
        [False, False],
    ]
    assert code.program(input) == input
