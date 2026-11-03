# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Build the standalone native clients used by the circuit notebooks."""

import json
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
SOURCES = HERE / "native"


def compiler(variable: str, candidates: tuple[str, ...]) -> list[str]:
    configured = os.environ.get(variable)
    if configured is not None:
        command = shlex.split(configured)
        if not command or shutil.which(command[0]) is None:
            raise RuntimeError(
                f"{variable} must name an installed compiler: {configured!r}"
            )
        return command
    for candidate in candidates:
        found = shutil.which(candidate)
        if found:
            return [found]
    raise RuntimeError(f"Install one of {candidates} or set {variable}.")


def native_environment() -> tuple[Path, Path, dict[str, str]]:
    if os.name == "nt":
        raise RuntimeError(
            "Run these GCC/Clang-style native samples in WSL2 on Windows."
        )
    if shutil.which("cargo") is None:
        raise RuntimeError(
            "Install Rust/Cargo and build the standalone circuit library first."
        )
    metadata = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version=1"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    target = Path(json.loads(metadata.stdout)["target_directory"])
    suffix = ".dylib" if os.sys.platform == "darwin" else ".so"
    library = Path(
        os.environ.get(
            "QDK_CIRCUIT_CAPI_LIBRARY",
            target / "circuit-capi/debug/lib" / f"libqdk_circuit_capi{suffix}",
        )
    ).resolve()
    include = Path(
        os.environ.get(
            "QDK_CIRCUIT_CAPI_INCLUDE", target / "circuit-capi/debug/include"
        )
    ).resolve()
    if not library.is_file() or not (include / "qdk_circuit.h").is_file():
        raise RuntimeError(
            "Build this branch's native library/header from the repository root with "
            "`python source/circuit_capi/build.py`, or set "
            "QDK_CIRCUIT_CAPI_LIBRARY and QDK_CIRCUIT_CAPI_INCLUDE."
        )
    env = os.environ.copy()
    key = "DYLD_LIBRARY_PATH" if os.sys.platform == "darwin" else "LD_LIBRARY_PATH"
    env[key] = str(library.parent) + os.pathsep + env.get(key, "")
    return include, library, env


def build_and_run(language: str, directory: Path) -> None:
    """Compile in a temporary directory, run there, and surface all failures."""
    include, library, env = native_environment()
    cc = compiler("CC", ("cc", "clang", "gcc"))
    executable = directory / f"circuit_{language}"
    link = [str(library), f"-Wl,-rpath,{library.parent}"]
    cflags = [
        "-std=c11",
        "-Wall",
        "-Wextra",
        "-Werror",
        "-I",
        str(include),
        "-I",
        str(SOURCES),
    ]
    if language == "c":
        commands = [
            cc + cflags + [str(SOURCES / "circuits.c")] + link + ["-o", str(executable)]
        ]
    elif language == "go":
        go = shutil.which("go")
        if go is None:
            raise RuntimeError(
                "Install Go with cgo support before running this notebook."
            )
        bindings = include.parent / "bindings" / "go"
        if not (bindings / "go.mod").is_file():
            raise RuntimeError(
                "Build the native SDK with `python source/circuit_capi/build.py`; "
                "the Go module must be staged alongside its include and lib directories."
            )
        shutil.copy2(SOURCES / "circuits.go", directory / "circuits.go")
        env["CGO_ENABLED"] = "1"
        env["GO111MODULE"] = "on"
        env["GOWORK"] = "off"
        env["CC"] = shlex.join(cc)
        env["CGO_CFLAGS"] = shlex.join(["-I", str(include)])
        env["CGO_LDFLAGS"] = shlex.join(["-L", str(library.parent)])
        module = "github.com/microsoft/qdk/source/circuit_capi/bindings/go"
        commands = []
        if not (directory / "go.mod").exists():
            commands.append([go, "mod", "init", "qdk-circuit-sample"])
        commands.extend(
            [
                [
                    go,
                    "mod",
                    "edit",
                    f"-require={module}@v0.0.0",
                    f"-replace={module}={bindings}",
                ],
                [go, "build", "-o", str(executable), "."],
            ]
        )
    elif language == "fortran":
        bindings = include.parent / "bindings" / "fortran"
        if (
            not (bindings / "qdk_circuit.f90").is_file()
            or not (bindings / "bridge.c").is_file()
        ):
            raise RuntimeError(
                "Build the native SDK with `python source/circuit_capi/build.py`; "
                "the Fortran sources must be staged alongside its include and lib directories."
            )
        bridge = directory / "qdk_fortran_bridge.o"
        commands = [cc + cflags + ["-c", str(bindings / "bridge.c"), "-o", str(bridge)]]
        fc = compiler("FC", ("flang", "flang-new", "gfortran"))
        commands.append(
            fc
            + [
                str(bindings / "qdk_circuit.f90"),
                str(SOURCES / "circuits.f90"),
                str(bridge),
            ]
            + link
            + ["-o", str(executable)]
        )
    else:
        raise ValueError(f"Unknown native sample language: {language}")
    for command in commands + [[str(executable)]]:
        print(shlex.join(command))
        result = subprocess.run(
            command, cwd=directory, env=env, check=False, capture_output=True, text=True
        )
        print(result.stdout, end="")
        print(result.stderr, end="", file=sys.stderr)
        result.check_returncode()
