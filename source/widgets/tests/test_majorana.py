# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import pytest

from qsharp_widgets import Majorana, _normalize_majorana_trace


def test_constructor_normalizes_and_defensively_copies_input():
    device_size = [1, 1]
    targets = [0]
    trace = [[["Mx", targets, None]]]

    widget = Majorana(device_size, trace)
    device_size[0] = 4
    targets[0] = 3
    trace.append([["T", [1], None]])

    assert widget.device_size == [1, 1]
    assert widget.trace == [[["Mx", [0], None]]]
    assert widget.comp == "Majorana"
    assert widget.show_qubit_labels is True
    assert widget.show_mzm_labels is False
    assert widget.enable_virtual_view is False
    assert widget.initial_view == "Tetrons"


def test_constructor_accepts_all_supported_operations():
    widget = Majorana(
        (1, 1),
        (
            (("Mx", (0,), None),),
            (("My-lw", (0,), None),),
            (("My-up", (2,), None),),
            (("Mz-lw", (1,), None),),
            (("Mz-up", (3,), None),),
            (("T", (3,), None),),
            (("Mxx", (0, 1), None),),
            (("Mzz", (0, 2), None),),
            (("Mzy", (1, 3), None),),
            (("Myy", (0, 2), None),),
            (("Myz", (1, 3), None),),
        ),
        show_qubit_labels=False,
        show_mzm_labels=True,
        initial_view="Qubits",
    )

    assert widget.device_size == [1, 1]
    assert widget.show_qubit_labels is False
    assert widget.show_mzm_labels is True
    assert widget.initial_view == "Qubits"
    assert widget.trace[-1] == [["Myz", [1, 3], None]]


def test_enabled_constructor_defaults_to_virtual_and_normalizes_metadata():
    trace = [
        [
            ("Mx", [0], ("T", (0,), 2)),
            ("Mx", [0], ("CX", (0, 1), 1)),
        ],
        [("Mzz", [0, 2], ("CZ", (2, 0), 0))],
    ]

    widget = Majorana((2, 1), trace, enable_virtual_view=True)

    assert widget.enable_virtual_view is True
    assert widget.initial_view == "Virtual"
    assert widget.trace == [
        [
            ["Mx", [0], ["T", [0], 2]],
            ["Mx", [0], ["CX", [0, 1], 1]],
        ],
        [["Mzz", [0, 2], ["CZ", [2, 0], 0]]],
    ]


@pytest.mark.parametrize("initial_view", ["Tetrons", "Qubits", "Virtual"])
def test_enabled_constructor_accepts_every_explicit_view(initial_view):
    widget = Majorana(
        (1, 1),
        [[("Mx", [0], ("T", [0], 0))]],
        enable_virtual_view=True,
        initial_view=initial_view,
    )

    assert widget.initial_view == initial_view


def test_disabled_constructor_rejects_virtual_initial_view():
    with pytest.raises(
        ValueError,
        match='options.initialView: expected "Tetrons" or "Qubits"',
    ):
        Majorana(
            (1, 1),
            [[("Mx", [0], None)]],
            initial_view="Virtual",
        )


@pytest.mark.parametrize(
    ("device_size", "error_type", "message"),
    [
        (1, TypeError, "deviceSize: expected [cellRows, cellColumns]"),
        ((1,), ValueError, "deviceSize: expected [cellRows, cellColumns]"),
        ((True, 1), TypeError, "deviceSize[0]: expected an integer"),
        ((0, 1), ValueError, "deviceSize[0]: expected an integer"),
        ((1, 5), ValueError, "deviceSize[1]: expected an integer"),
    ],
)
def test_constructor_rejects_invalid_device_size(device_size, error_type, message):
    with pytest.raises(error_type) as error:
        Majorana(device_size, [[("Mx", [0], None)]])
    assert message in str(error.value)


@pytest.mark.parametrize(
    ("trace", "error_type", "message"),
    [
        ("Mx", TypeError, "trace: expected a non-empty array"),
        ([], ValueError, "trace: expected a non-empty array"),
        ([[]], ValueError, r"trace\[0\]: expected a non-empty array"),
        (
            [[("mx", [0], None)]],
            ValueError,
            r"trace\[0\]\[0\]\.operation: expected a supported operation name",
        ),
        (
            [[("Mx", [], None)]],
            ValueError,
            r"trace\[0\]\[0\]\.targets: expected exactly one target",
        ),
        (
            [[("Mx", [4], None)]],
            ValueError,
            r"trace\[0\]\[0\]\.targets\[0\]: expected an integer",
        ),
        (
            [[("Mxx", [0, 2], None)]],
            ValueError,
            "expected horizontally adjacent targets",
        ),
        (
            [[("Mxx", [1, 0], None)]],
            ValueError,
            "expected targets ordered left then right",
        ),
        (
            [[("Mzz", [0, 1], None)]],
            ValueError,
            "expected vertically adjacent targets",
        ),
        (
            [[("Mzz", [2, 0], None)]],
            ValueError,
            "expected targets ordered upper then lower",
        ),
        (
            [[("Mzz", [0, 0], None)]],
            ValueError,
            "expected two distinct targets",
        ),
        (
            [[("My-up", [0], None)]],
            ValueError,
            "My-up requires an island above the target",
        ),
        (
            [[("Mz-lw", [2], None)]],
            ValueError,
            "Mz-lw requires an island below the target",
        ),
    ],
)
def test_constructor_rejects_invalid_trace(trace, error_type, message):
    with pytest.raises(error_type, match=message):
        Majorana((1, 1), trace)


def test_constructor_requires_three_members_and_ignores_disabled_metadata():
    with pytest.raises(
        ValueError,
        match=r"trace\[0\]\[0\]: expected \[physicalOperation",
    ):
        Majorana((1, 1), [[("Mx", [0])]])

    widget = Majorana((1, 1), [[("Mx", [0], {"malformed": True})]])
    assert widget.trace == [[["Mx", [0], None]]]


def test_internal_virtual_normalization_is_prepared_for_feature_enablement():
    trace = [
        [
            ("Mx", [0], ("T", (0,), 0)),
            ("Mx", [0], ("H", (0,), 0)),
            ("Mx", [0], ("S", (0,), 0)),
            ("Mx", [0], ("Mx", (0,), 0)),
            ("Mx", [0], ("My", (0,), 0)),
            ("Mx", [0], ("Mz", (0,), 0)),
            ("Mx", [0], ("CX", (0, 1), 0)),
        ],
        [("Mx", [0], ("CZ", (2, 0), 0))],
    ]
    normalized = _normalize_majorana_trace(
        trace, [2, 1], enable_virtual_view=True
    )

    assert normalized == [
        [
            ["Mx", [0], ["T", [0], 0]],
            ["Mx", [0], ["H", [0], 0]],
            ["Mx", [0], ["S", [0], 0]],
            ["Mx", [0], ["Mx", [0], 0]],
            ["Mx", [0], ["My", [0], 0]],
            ["Mx", [0], ["Mz", [0], 0]],
            ["Mx", [0], ["CX", [0, 1], 0]],
        ],
        [["Mx", [0], ["CZ", [2, 0], 0]]],
    ]
    assert trace[0][0][2] == ("T", (0,), 0)

    with pytest.raises(
        ValueError,
        match=r"trace\[0\]\[0\]\.virtualOperation\.targets",
    ):
        _normalize_majorana_trace(
            [[("Mx", [0], ("CX", (0, 2), 0))]],
            [2, 1],
            enable_virtual_view=True,
        )


@pytest.mark.parametrize(
    ("metadata", "message"),
    [
        (
            None,
            r"virtualOperation: expected \[operationName, targets, precedence\]",
        ),
        (
            ("T", (0,)),
            r"virtualOperation: expected \[operationName, targets, precedence\]",
        ),
        (("X", (0,), 0), "expected a supported virtual operation name"),
        (("T", (0, 1), 0), "expected exactly one target"),
        (("CZ", (0,), 0), "expected exactly two targets"),
        (("T", (4,), 0), r"virtualOperation\.targets\[0\]"),
        (("CX", (0, 0), 0), "expected two distinct virtual targets"),
        (("CX", (0, 2), 0), "expected horizontally adjacent virtual targets"),
        (("CZ", (0, 1), 0), "expected vertically adjacent virtual targets"),
        (("T", (0,), -1), r"virtualOperation\.precedence"),
        (("T", (0,), 0.5), r"virtualOperation\.precedence"),
        (("T", (0,), True), r"virtualOperation\.precedence"),
    ],
)
def test_internal_virtual_normalization_rejects_invalid_metadata(
    metadata, message
):
    with pytest.raises((TypeError, ValueError), match=message):
        _normalize_majorana_trace(
            [[("Mx", [0], metadata)]],
            [2, 1],
            enable_virtual_view=True,
        )


@pytest.mark.parametrize(
    ("option", "value", "error_type", "message"),
    [
        (
            "show_qubit_labels",
            1,
            TypeError,
            "options.showQubitLabels: expected a boolean",
        ),
        (
            "show_mzm_labels",
            "yes",
            TypeError,
            "options.showMzmLabels: expected a boolean",
        ),
        (
            "enable_virtual_view",
            1,
            TypeError,
            "options.enableVirtualView: expected a boolean",
        ),
        (
            "initial_view",
            1,
            TypeError,
            "options.initialView: expected",
        ),
        (
            "initial_view",
            "tetrons",
            ValueError,
            "options.initialView: expected",
        ),
    ],
)
def test_constructor_rejects_invalid_options(option, value, error_type, message):
    options = {option: value}
    with pytest.raises(error_type, match=message):
        Majorana((1, 1), [[("Mx", [0], None)]], **options)


def test_trait_updates_are_validated_normalized_and_synchronized():
    widget = Majorana((1, 1), [[("Mx", [0], None)]])
    replacement_trace = ((("Mzz", (0, 2), None),),)

    widget.trace = replacement_trace

    assert widget.trace == [[["Mzz", [0, 2], None]]]
    assert widget.traits()["device_size"].metadata["sync"] is True
    assert widget.traits()["trace"].metadata["sync"] is True
    assert widget.traits()["enable_virtual_view"].metadata["sync"] is True
    assert widget.traits()["show_qubit_labels"].metadata["sync"] is True
    with pytest.raises(ValueError, match="expected horizontally adjacent targets"):
        widget.trace = [[("Mxx", [0, 2], None)]]
    with pytest.raises(ValueError, match="options.initialView: expected"):
        widget.initial_view = "tetrons"
    with pytest.raises(TypeError, match="options.showQubitLabels"):
        widget.show_qubit_labels = 1


def test_trace_updates_use_the_constructed_feature_mode():
    disabled = Majorana((1, 1), [[("Mx", [0], None)]])
    disabled.trace = [[("Mx", [0], {"malformed": True})]]
    assert disabled.trace == [[["Mx", [0], None]]]

    enabled = Majorana(
        (2, 1),
        [[("Mx", [0], ("T", [0], 0))]],
        enable_virtual_view=True,
    )
    enabled.trace = [[("Mx", [0], ("CX", (0, 1), 0))]]
    assert enabled.trace == [[["Mx", [0], ["CX", [0, 1], 0]]]]
    with pytest.raises(
        TypeError,
        match=r"trace\[0\]\[0\]\.virtualOperation",
    ):
        enabled.trace = [[("Mx", [0], None)]]


def test_enable_virtual_view_is_immutable_after_construction():
    disabled = Majorana((1, 1), [[("Mx", [0], None)]])
    with pytest.raises(
        ValueError,
        match="options.enableVirtualView: cannot be changed after construction",
    ):
        disabled.enable_virtual_view = True
    assert disabled.enable_virtual_view is False

    enabled = Majorana(
        (1, 1),
        [[("Mx", [0], ("T", [0], 0))]],
        enable_virtual_view=True,
    )
    with pytest.raises(
        ValueError,
        match="options.enableVirtualView: cannot be changed after construction",
    ):
        enabled.enable_virtual_view = False
    assert enabled.enable_virtual_view is True


def test_device_size_update_cannot_invalidate_the_current_trace():
    widget = Majorana((1, 2), [[("Mx", [7], None)]])

    with pytest.raises(ValueError, match=r"trace\[0\]\[0\]\.targets\[0\]"):
        widget.device_size = [1, 1]

    assert widget.device_size == [1, 2]
