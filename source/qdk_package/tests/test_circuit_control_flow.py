# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import gc
import warnings

import pytest

warnings.filterwarnings("ignore", message="qdk.circuit is experimental")

from qdk.circuit import (  # noqa: E402
    ClassicalRegister,
    LoopVariable,
    Parameter,
    QuantumCircuit,
    QuantumRegister,
)


def test_if_test_collects_body_into_a_block():
    circuit = QuantumCircuit(2, 1)
    circuit.h(0)
    circuit.measure(0, 0)
    with circuit.if_test(0, 1):
        circuit.x(1)

    names = [circuit[i].name for i in range(len(circuit))]
    assert names == ["h", "measure_z", "if_else"]
    assert [v.name for v in circuit.body_of(2)] == ["x"]


def test_body_builder_retains_its_circuit_after_parent_variable_is_dropped():
    circuit = QuantumCircuit(1, 1)
    builder = circuit.if_test(0, 1)
    del circuit
    gc.collect()

    with builder as retained:
        retained.x(0)

    assert retained[0].name == "if_else"
    assert [instruction.name for instruction in retained.body_of(0)] == ["x"]


def test_case_builder_retains_its_switch_after_parent_variable_is_dropped():
    circuit = QuantumCircuit(1, 1)
    switch = circuit.switch(0)
    switch.__enter__()
    case = switch.case(1)
    del switch
    gc.collect()

    with case as retained:
        retained.x(0)

    assert retained is circuit
    # The retained switch still owns the completed case; it is not implicitly
    # published merely because the caller dropped its reference.
    assert len(circuit) == 0


def test_body_instructions_do_not_appear_at_top_level():
    circuit = QuantumCircuit(2, 1)
    with circuit.if_test(0, 1):
        circuit.x(0)
        circuit.y(0)

    assert len(circuit) == 1, "the body must not leak into the outer instruction list"
    assert [v.name for v in circuit.body_of(0)] == ["x", "y"]


def test_if_else_produces_one_construct_with_both_branches():
    circuit = QuantumCircuit(2, 1)
    builder = circuit.if_test(0, 1)
    with builder:
        circuit.x(0)
    with builder.otherwise():
        circuit.z(0)

    assert len(circuit) == 1, "an if-else pair is one instruction, not two"
    bodies = [v.name for v in circuit.body_of(0)]
    assert bodies == ["x", "z"]


def test_chained_conditionals_nest_without_disturbing_earlier_instructions():
    circuit = QuantumCircuit(2, 2)
    circuit.measure(0, 0)
    circuit.measure(0, 1)

    outer = circuit.if_test(0, 1)
    with outer:
        circuit.x(1)
    with outer.otherwise():
        inner = circuit.if_test(1, 1)
        with inner:
            circuit.y(1)
        with inner.otherwise():
            circuit.h(1)

    # An else branch replaces the if-only construct emitted when the then branch
    # closed. That replacement has to happen inside the enclosing block when the
    # chain is nested, or it removes an unrelated instruction from the top level.
    assert [circuit[i].name for i in range(len(circuit))] == [
        "measure_z",
        "measure_z",
        "if_else",
    ]
    assert [v.name for v in circuit.body_of(2)] == ["x", "if_else"]


def test_a_chain_keeps_one_construct_per_link():
    circuit = QuantumCircuit(1, 2)
    outer = circuit.if_test(0, 1)
    with outer:
        circuit.x(0)
    with outer.otherwise():
        middle = circuit.if_test(1, 1)
        with middle:
            circuit.y(0)
        with middle.otherwise():
            inner = circuit.if_test(1, 0)
            with inner:
                circuit.z(0)
            with inner.otherwise():
                circuit.h(0)

    assert len(circuit) == 1
    outer_body = [v.name for v in circuit.body_of(0)]
    assert outer_body == ["x", "if_else"], "each link is one construct, not two"


def test_while_loop_collects_its_body():
    circuit = QuantumCircuit(1, 1)
    with circuit.while_loop(0, 1):
        circuit.h(0)
        circuit.measure(0, 0)

    assert circuit[0].name == "while"
    assert [v.name for v in circuit.body_of(0)] == ["h", "measure_z"]


def test_for_loop_over_compile_time_values():
    circuit = QuantumCircuit(1)
    with circuit.for_loop([0, 1, 2]):
        circuit.x(0)

    assert circuit[0].name == "for"
    assert [v.name for v in circuit.body_of(0)] == ["x"]


def test_for_loop_hands_its_index_to_the_body():
    circuit = QuantumCircuit(1)
    with circuit.for_loop([0.0, 1.0]) as index:
        assert isinstance(index, Parameter)
        circuit.rx(index, 0)

    assert [v.params for v in circuit.body_of(0)] == [[index]]


def test_for_loop_names_its_index_when_asked():
    circuit = QuantumCircuit(1)
    with circuit.for_loop([0, 1], "step") as index:
        circuit.x(0)

    assert index.name == "step"


def test_generated_index_names_do_not_collide():
    circuit = QuantumCircuit(1)
    with circuit.for_loop([0]) as outer:
        with circuit.for_loop([0]) as inner:
            circuit.x(0)

    assert outer.name != inner.name


def test_generated_index_avoids_a_declared_name():
    circuit = QuantumCircuit(1)
    circuit.parameter("_loop_i_0")
    with circuit.for_loop([0]) as index:
        circuit.x(0)

    assert index.name != "_loop_i_0"


def test_runtime_for_loop_hands_back_a_variable():
    circuit = QuantumCircuit(1)
    with circuit.for_loop([0, 1, 2], runtime=True) as step:
        assert isinstance(step, LoopVariable)
        with circuit.if_test(step, 1):
            circuit.x(0)

    assert [v.name for v in circuit.body_of(0)] == ["if_else"]


def test_a_runtime_index_cannot_stand_in_for_an_angle():
    circuit = QuantumCircuit(1)
    with circuit.for_loop([0, 1], runtime=True) as step:
        with pytest.raises(TypeError):
            circuit.rx(step, 0)


def test_a_compile_time_index_cannot_be_tested():
    circuit = QuantumCircuit(1, 1)
    with circuit.for_loop([0, 1]) as index:
        with pytest.raises(TypeError):
            circuit.if_test(index, 1)


def test_for_loop_builder_exposes_its_index_outside_the_body():
    circuit = QuantumCircuit(1)
    loop = circuit.for_loop([0, 1], "step")
    assert loop.index.name == "step"
    with loop as index:
        circuit.x(0)
    assert index.name == "step"


def test_a_failed_for_body_emits_nothing():
    circuit = QuantumCircuit(1)
    with pytest.raises(IndexError):
        with circuit.for_loop([0, 1]):
            circuit.x(5)

    assert len(circuit) == 0


def test_box_groups_instructions():
    circuit = QuantumCircuit(2)
    with circuit.box_("prep"):
        circuit.h(0)
        circuit.cx(0, 1)

    assert circuit[0].name == "box"
    assert [v.name for v in circuit.body_of(0)] == ["h", "cx"]


def test_nested_control_flow_nests_blocks():
    circuit = QuantumCircuit(1, 1)
    with circuit.if_test(0, 1):
        circuit.x(0)
        with circuit.while_loop(0, 1):
            circuit.h(0)

    assert len(circuit) == 1
    outer = [v.name for v in circuit.body_of(0)]
    assert outer == ["x", "while"], "the inner loop belongs to the outer body"


def test_a_raising_body_does_not_emit_a_half_formed_construct():
    circuit = QuantumCircuit(1, 1)
    with pytest.raises(RuntimeError, match="boom"):
        with circuit.if_test(0, 1):
            circuit.x(0)
            raise RuntimeError("boom")

    assert len(circuit) == 0, "a failed build must not leave a construct behind"


def test_builder_is_not_reentrant():
    circuit = QuantumCircuit(1, 1)
    builder = circuit.if_test(0, 1)
    with builder:
        circuit.x(0)
    with pytest.raises(ValueError, match="not reentrant"):
        with builder:
            pass


def test_appends_resume_at_top_level_after_the_block_closes():
    circuit = QuantumCircuit(2, 1)
    with circuit.if_test(0, 1):
        circuit.x(0)
    circuit.h(1)

    assert [circuit[i].name for i in range(len(circuit))] == ["if_else", "h"]


def test_otherwise_rejected_on_a_loop():
    circuit = QuantumCircuit(1, 1)
    builder = circuit.while_loop(0, 1)
    with pytest.raises(ValueError, match="alternative branch"):
        builder.otherwise()


def test_parameters_inside_a_block_are_reported_and_bindable():
    circuit = QuantumCircuit(1, 1)
    theta = circuit.parameter("theta")
    with circuit.if_test(0, 1):
        circuit.rx(theta, 0)

    assert circuit.parameters == [theta]
    bound = circuit.assign_parameters([(theta, 0.5)])
    assert bound.parameters == []
    assert bound.body_of(0)[0].params == [pytest.approx(0.5)]


def test_switch_collects_each_case_into_its_own_block():
    circuit = QuantumCircuit(1, 1)
    circuit.measure(0, 0)
    with circuit.switch(0) as selected:
        with selected.case(0):
            circuit.x(0)
        with selected.case(1):
            circuit.z(0)

    view = circuit[1]
    assert view.name == "switch"
    assert view.cases == [[0], [1]]
    assert view.default_block is None
    bodies = [[v.name for v in circuit.block(block)] for block in view.blocks]
    assert bodies == [["x"], ["z"]], "each case keeps its own body"


def test_a_switch_case_matches_several_values():
    circuit = QuantumCircuit(1, 1)
    with circuit.switch(0) as selected:
        with selected.case(1, 2, 3):
            circuit.x(0)

    assert circuit[0].cases == [[1, 2, 3]]


def test_a_switch_default_is_reported_separately_from_its_cases():
    circuit = QuantumCircuit(1, 1)
    with circuit.switch(0) as selected:
        with selected.case(0):
            circuit.x(0)
        with selected.default():
            circuit.z(0)

    view = circuit[0]
    assert view.cases == [[0]], "the default is not a case with values"
    assert view.default_block == view.blocks[-1]
    assert [v.name for v in circuit.block(view.default_block)] == ["z"]


def test_a_switch_is_one_instruction_however_many_cases_it_has():
    circuit = QuantumCircuit(1, 1)
    with circuit.switch(0) as selected:
        with selected.case(0):
            circuit.x(0)
        with selected.case(1):
            circuit.z(0)
        with selected.default():
            circuit.h(0)

    assert len(circuit) == 1


def test_instructions_between_cases_are_not_swallowed_by_the_switch():
    # The if-else pair is rebuilt when its second branch closes. A switch is
    # not, so an append between two cases has to survive.
    circuit = QuantumCircuit(1, 1)
    with circuit.switch(0) as selected:
        with selected.case(0):
            circuit.x(0)
        circuit.h(0)
        with selected.case(1):
            circuit.z(0)

    assert [circuit[i].name for i in range(len(circuit))] == ["h", "switch"]


def test_a_switch_needs_at_least_one_case():
    circuit = QuantumCircuit(1, 1)
    with pytest.raises(ValueError, match="at least one case"):
        with circuit.switch(0):
            pass


def test_a_switch_case_needs_at_least_one_value():
    circuit = QuantumCircuit(1, 1)
    with circuit.switch(0) as selected:
        with pytest.raises(ValueError, match="at least one value"):
            selected.case()
        with selected.case(0):
            circuit.x(0)


def test_a_switch_has_one_default():
    circuit = QuantumCircuit(1, 1)
    with circuit.switch(0) as selected:
        with selected.default():
            circuit.x(0)
        with pytest.raises(ValueError, match="one default"):
            with selected.default():
                circuit.z(0)


def test_a_switch_does_not_select_on_a_register():
    circuit = QuantumCircuit.from_registers(
        QuantumRegister("q", 1),
        ClassicalRegister("c", 2),
    )
    with pytest.raises(ValueError, match="not on register 'c'"):
        circuit.switch(ClassicalRegister("c", 2))


def test_a_raising_case_body_does_not_record_the_case():
    circuit = QuantumCircuit(1, 1)
    with circuit.switch(0) as selected:
        with pytest.raises(RuntimeError, match="boom"):
            with selected.case(0):
                circuit.x(0)
                raise RuntimeError("boom")
        with selected.case(1):
            circuit.z(0)

    assert circuit[0].cases == [[1]], "the failed case must not be recorded"


def test_break_and_continue_sit_inside_a_loop_body():
    circuit = QuantumCircuit(1, 1)
    with circuit.while_loop(0, 1):
        circuit.h(0)
        circuit.continue_loop()
        circuit.break_loop()

    assert [v.name for v in circuit.body_of(0)] == ["h", "continue", "break"]


def test_break_and_continue_reach_out_through_a_nested_conditional():
    circuit = QuantumCircuit(1, 1)
    with circuit.for_loop([0, 1]):
        with circuit.if_test(0, 1):
            circuit.break_loop()

    inner = circuit.block(circuit.body_of(0)[0].blocks[0])
    assert [v.name for v in inner] == ["break"]


@pytest.mark.parametrize("jump", ["break_loop", "continue_loop"])
def test_a_jump_outside_a_loop_is_refused(jump):
    circuit = QuantumCircuit(1, 1)
    with pytest.raises(ValueError, match="only meaningful inside a loop body"):
        getattr(circuit, jump)()

    assert len(circuit) == 0


@pytest.mark.parametrize("jump", ["break_loop", "continue_loop"])
def test_a_jump_inside_a_box_alone_is_refused(jump):
    # A box groups instructions without repeating them, so there is nothing
    # for a jump to leave.
    circuit = QuantumCircuit(1, 1)
    with circuit.box_("grouped"):
        with pytest.raises(ValueError, match="only meaningful inside a loop body"):
            getattr(circuit, jump)()


if __name__ == "__main__":
    pytest.main([__file__])
