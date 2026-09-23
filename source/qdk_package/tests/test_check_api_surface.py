"""The API checker must distinguish unavailable exports from a clean scan."""

import json
from pathlib import Path
import sys
import types

import pytest

sys.path.append(str(Path(__file__).resolve().parents[1]))
import check_api_surface as checker


@pytest.mark.parametrize(
    "error", [ModuleNotFoundError("missing backend"), AttributeError("missing export")]
)
def test_unavailable_export_reports_incomplete_scan(monkeypatch, capsys, error) -> None:
    module = types.ModuleType("qdk.example")
    module.__all__ = ["Public"]

    def unavailable(name):
        raise error

    module.__getattr__ = unavailable
    monkeypatch.setattr(
        checker, "_iter_qdk_modules", lambda: [(module.__name__, module)]
    )
    monkeypatch.setattr(sys, "argv", ["check_api_surface.py"])
    assert checker.main() == 2
    output = capsys.readouterr()
    assert "API scan incomplete" in output.err
    assert "qdk.example.Public" in output.err
    assert "No private API leakage" not in output.err


def test_programming_errors_in_lazy_exports_propagate() -> None:
    module = types.ModuleType("qdk.example")

    def broken(name):
        raise ValueError("invalid initialization")

    module.__getattr__ = broken
    with pytest.raises(ValueError, match="invalid initialization"):
        checker._lazy_getattr(module, module.__name__, "Public")


def test_missing_declared_export_retains_the_cause() -> None:
    with pytest.raises(checker.ScanIncomplete, match="qdk.example.Missing") as caught:
        checker._lazy_getattr(types.ModuleType("qdk.example"), "qdk.example", "Missing")
    assert isinstance(caught.value.__cause__, AttributeError)


def test_class_member_failures_are_not_silently_skipped(monkeypatch) -> None:
    class BrokenMeta(type):
        def __getattribute__(cls, name):
            if name == "method":
                raise RuntimeError("broken descriptor")
            return super().__getattribute__(name)

    class Public(metaclass=BrokenMeta):
        def method(self):
            pass

    module = types.ModuleType("qdk.example")
    module.__all__ = ["Public"]
    module.Public = Public
    monkeypatch.setattr(
        checker, "_iter_qdk_modules", lambda: [(module.__name__, module)]
    )
    with pytest.raises(RuntimeError, match="broken descriptor"):
        checker.scan()


def test_incomplete_scan_has_machine_readable_output(monkeypatch, capsys) -> None:
    def incomplete():
        raise checker.ScanIncomplete("missing module")

    monkeypatch.setattr(checker, "scan", incomplete)
    monkeypatch.setattr(sys, "argv", ["check_api_surface.py", "--json"])
    assert checker.main() == 2
    assert json.loads(capsys.readouterr().out) == {
        "error": "incomplete scan",
        "detail": "missing module",
    }


def test_complete_empty_scan_succeeds(monkeypatch, capsys) -> None:
    module = types.ModuleType("qdk.example")
    module.__all__ = ["constant"]
    module.constant = None
    monkeypatch.setattr(
        checker, "_iter_qdk_modules", lambda: [(module.__name__, module)]
    )
    monkeypatch.setattr(sys, "argv", ["check_api_surface.py"])
    assert checker.main() == 0
    assert "No private API leakage" in capsys.readouterr().err


def test_module_discovery_does_not_skip_import_failures(monkeypatch) -> None:
    root = types.ModuleType("qdk")
    root.__path__ = []
    monkeypatch.setattr(checker, "_import_root_package", lambda: root)
    monkeypatch.setattr(
        checker.pkgutil,
        "walk_packages",
        lambda *args, **kwargs: [(None, "qdk.example", False)],
    )

    def unavailable(name):
        raise ImportError("missing dependency")

    monkeypatch.setattr(checker.importlib, "import_module", unavailable)
    with pytest.raises(checker.ScanIncomplete, match="qdk.example"):
        checker.scan()
