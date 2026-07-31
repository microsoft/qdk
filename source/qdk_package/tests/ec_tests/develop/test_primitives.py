"""``qdk.ec.develop`` primitives: load, save, from_yaml, to_yaml."""

from __future__ import annotations

from pathlib import Path

import pytest
import qodec

from ec_tests.testing.qodecs import c4
from qdk.ec import develop


def test_to_yaml_round_trips_through_from_yaml() -> None:
    codec = c4()

    restored = develop.from_yaml(develop.to_yaml(codec))

    assert restored.name == codec.name
    assert [layer.isa.name for layer in restored.layers] == [
        layer.isa.name for layer in codec.layers
    ]
    assert sorted(restored.layers[0].gadgets) == sorted(codec.layers[0].gadgets)


def test_to_yaml_is_stable() -> None:
    codec = c4()

    once = develop.to_yaml(codec)

    assert develop.to_yaml(develop.from_yaml(once)) == once


def test_save_then_load_round_trips(tmp_path: Path) -> None:
    codec = c4()

    develop.save(codec, tmp_path / "bundle")
    restored = develop.load(tmp_path / "bundle")

    assert restored.name == codec.name
    assert sorted(restored.codes) == sorted(codec.codes)


def test_save_accepts_a_pathlib_path_and_creates_the_directory(
    tmp_path: Path,
) -> None:
    destination = tmp_path / "nested" / "bundle"

    develop.save(c4(), destination, single_file=True)

    assert destination.is_dir()
    assert any(destination.iterdir())


def test_load_accepts_a_str_path(tmp_path: Path) -> None:
    develop.save(c4(), tmp_path / "bundle")

    assert isinstance(develop.load(str(tmp_path / "bundle")), qodec.Qodec)


def test_from_yaml_rejects_garbage() -> None:
    with pytest.raises(Exception):
        develop.from_yaml("not: a qodec\n")
