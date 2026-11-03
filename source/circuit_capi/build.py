#!/usr/bin/env python3
# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Build the standalone circuit library and optionally run native consumers."""

import argparse
import json
import logging
import os
from pathlib import Path
import shlex
import shutil
import subprocess
import sys

LOGGER = logging.getLogger(__name__)
SOURCE = Path(__file__).resolve().parent
ROOT = SOURCE.parents[1]
REFERENCE_HEADER = SOURCE / "includes" / "qdk_circuit.h"


def create_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--test",
        action=argparse.BooleanOptionalAction,
        default=False,
        help="compile/run C11 and Go/cgo consumers with strict pointer checking",
    )
    parser.add_argument(
        "--test-fortran",
        action="store_true",
        help="compile/run the Fortran consumer using FC (flang or gfortran by default)",
    )
    headers = parser.add_mutually_exclusive_group()
    headers.add_argument(
        "--update-header", action="store_true", help="refresh the tracked generated header"
    )
    headers.add_argument(
        "--check-header", action="store_true", help="check the tracked header without changing it"
    )
    return parser


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = create_parser()
    args = parser.parse_args(argv)
    if args.update_header and (args.test or args.test_fortran):
        parser.error("--update-header cannot be combined with consumer tests")
    return args


def check_header(generated: Path, reference: Path) -> None:
    if not reference.is_file():
        raise RuntimeError(
            f"Missing tracked header: {reference}; run with --update-header"
        )
    if generated.read_bytes() != reference.read_bytes():
        raise RuntimeError(
            f"Stale tracked header: {reference}; run with --update-header and review the diff"
        )
    LOGGER.info("Tracked header is current: %s", reference)


def update_header(generated: Path, reference: Path) -> None:
    reference.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(generated, reference)
    LOGGER.info("Updated tracked header: %s", reference)


def run(
    command: list[str],
    *,
    cwd: Path = ROOT,
    env: dict[str, str] | None = None,
    capture: bool = False,
) -> str:
    LOGGER.info("%s", shlex.join(command))
    try:
        result = subprocess.run(
            command,
            cwd=cwd,
            env=env,
            check=True,
            text=True,
            stdout=subprocess.PIPE if capture else None,
        )
    except subprocess.CalledProcessError as error:
        if capture and error.stdout:
            LOGGER.error("%s", error.stdout.rstrip())
        raise
    return result.stdout if capture else ""


def tool(name: str) -> str:
    found = shutil.which(name)
    if found is None:
        raise RuntimeError(f"Required tool is missing: {name}")
    return found


def only(paths: set[Path], description: str) -> Path:
    if len(paths) != 1:
        raise RuntimeError(
            f"Expected one {description}, found: {sorted(map(str, paths))}"
        )
    path = paths.pop()
    if not path.is_file():
        raise RuntimeError(f"Missing {description}: {path}")
    return path


def stage_sdk(staging: Path, header: Path, library: Path) -> tuple[Path, Path]:
    include = staging / "include"
    libraries = staging / "lib"
    include.mkdir(parents=True, exist_ok=True)
    libraries.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(header, include / header.name)
    staged_library = libraries / library.name
    shutil.copy2(library, staged_library)
    shutil.copyfile(ROOT / "LICENSE.txt", staging / "LICENSE.txt")
    for language in ("go", "fortran"):
        bindings = staging / "bindings" / language
        ignore = (
            shutil.ignore_patterns("*.mod", "*.smod", "*.o")
            if language == "fortran" else None
        )
        shutil.copytree(
            SOURCE / "bindings" / language, bindings,
            dirs_exist_ok=True, ignore=ignore,
        )
        shutil.copyfile(ROOT / "LICENSE.txt", bindings / "LICENSE.txt")
        LOGGER.info("%s bindings: %s", language, bindings)
    LOGGER.info("Header: %s", include / header.name)
    LOGGER.info("Library: %s", staged_library)
    return include, staged_library


def build(cargo: str) -> tuple[Path, Path, Path]:
    metadata = json.loads(
        run(
            [cargo, "metadata", "--no-deps", "--format-version=1"],
            capture=True,
        )
    )
    package_ids = {
        package["id"]
        for package in metadata["packages"]
        if package["name"] == "qdk_circuit_capi"
    }
    if len(package_ids) != 1:
        raise RuntimeError("Cargo metadata did not identify qdk_circuit_capi")
    target = Path(metadata["target_directory"]).resolve()
    artifacts = run(
        [cargo, "build", "--locked", "-p", "qdk_circuit_capi", "--message-format=json"],
        capture=True,
    )
    libraries: set[Path] = set()
    headers: set[Path] = set()
    for line in artifacts.splitlines():
        artifact = json.loads(line)
        if artifact.get("package_id") not in package_ids:
            continue
        if artifact["reason"] == "build-script-executed":
            headers.add(Path(artifact["out_dir"]) / "qdk_circuit.h")
        if artifact["reason"] == "compiler-artifact":
            libraries.update(
                Path(filename)
                for filename in artifact["filenames"]
                if Path(filename).suffix in {".dylib", ".so", ".dll"}
            )
    library = only(libraries, "native shared library").resolve()
    header = only(headers, "generated C header").resolve()
    staging = target / "circuit-capi" / "debug"
    include, staged_library = stage_sdk(staging, header, library)
    return staging, include, staged_library


def c_compiler() -> list[str]:
    compiler = shlex.split(os.environ.get("CC", "cc"))
    if not compiler:
        raise RuntimeError("CC must name a C compiler")
    compiler[0] = tool(compiler[0])
    if Path(compiler[0]).stem.lower() == "cl":
        raise RuntimeError(
            "These C11/cgo clients require a GCC/Clang-compatible C compiler"
        )
    return compiler


def fortran_compiler(value: str | None) -> list[str]:
    if value is not None:
        compiler = shlex.split(value)
        if not compiler:
            raise RuntimeError("FC must name a Fortran compiler")
        compiler[0] = tool(compiler[0])
        return compiler
    for candidate in ("flang", "flang-new", "gfortran"):
        found = shutil.which(candidate)
        if found is not None:
            return [found]
    raise RuntimeError("Required Fortran compiler is missing; set FC to flang or gfortran")


def loader_environment(library: Path) -> dict[str, str]:
    env = os.environ.copy()
    loader = (
        "DYLD_LIBRARY_PATH"
        if sys.platform == "darwin"
        else "PATH" if os.name == "nt" else "LD_LIBRARY_PATH"
    )
    env[loader] = str(library.parent) + os.pathsep + env.get(loader, "")
    return env


def library_link(library: Path) -> list[str]:
    link = [str(library)]
    if os.name != "nt":
        link.append(f"-Wl,-rpath,{library.parent}")
    return link


def fortran_consumer(staging: Path, include: Path, library: Path) -> None:
    compiler = fortran_compiler(os.environ.get("FC"))
    cc = c_compiler()
    env = loader_environment(library)
    directory = staging / "fortran"
    directory.mkdir(parents=True, exist_ok=True)
    source = SOURCE / "tests" / "fortran"
    adapter = directory / "shim.o"
    executable = directory / ("circuit_test.exe" if os.name == "nt" else "circuit_test")
    run(
        cc + [
            "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", str(include),
            "-c", str(source / "shim.c"), "-o", str(adapter),
        ],
        cwd=directory,
        env=env,
    )
    run(
        compiler + [
            str(source / "qdk_circuit_bindings.f90"),
            str(source / "circuit_test.f90"),
            str(adapter), *library_link(library), "-o", str(executable),
        ],
        cwd=directory,
        env=env,
    )
    fixtures = SOURCE / "tests" / "fixtures"
    run(
        [str(executable), str(fixtures / "adaptive.ll"), str(fixtures / "adaptive_ri.ll")],
        cwd=directory,
        env=env,
    )
    bindings = staging / "bindings" / "fortran"
    directory = staging / "fortran-binding-tests"
    directory.mkdir(parents=True, exist_ok=True)
    adapter = directory / "bridge.o"
    executable = directory / ("test_binding.exe" if os.name == "nt" else "test_binding")
    run(
        cc + [
            "-std=c11", "-Wall", "-Wextra", "-Werror", "-I", str(include),
            "-c", str(bindings / "bridge.c"), "-o", str(adapter),
        ],
        cwd=directory,
        env=env,
    )
    run(
        compiler + [
            str(bindings / "qdk_circuit.f90"),
            str(bindings / "test_binding.f90"),
            str(adapter), *library_link(library), "-o", str(executable),
        ],
        cwd=directory,
        env=env,
    )
    run([str(executable)], cwd=directory, env=env)


def consumers(staging: Path, include: Path, library: Path) -> None:
    compiler = c_compiler()
    go = tool("go")
    env = loader_environment(library)
    binaries = staging / "tests"
    binaries.mkdir(parents=True, exist_ok=True)
    sources = sorted((SOURCE / "tests" / "c").glob("*.c"))
    if not sources:
        raise RuntimeError("No C consumer tests found")
    link = library_link(library)
    for source in sources:
        executable = binaries / (source.stem + (".exe" if os.name == "nt" else ""))
        run(
            compiler
            + [
                "-std=c11",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-I",
                str(include),
                str(source),
                *link,
                "-o",
                str(executable),
            ],
            env=env,
        )
        run([str(executable)], env=env)
    cache = staging / "go-cache"
    temporary = staging / "go-tmp"
    cache.mkdir(parents=True, exist_ok=True)
    temporary.mkdir(parents=True, exist_ok=True)
    experiments = [
        item
        for item in env.get("GOEXPERIMENT", "").split(",")
        if item and item not in {"cgocheck2", "nocgocheck2"}
    ]
    env.update(
        {
            "CGO_ENABLED": "1",
            "GOEXPERIMENT": ",".join([*experiments, "cgocheck2"]),
            "CC": shlex.join(compiler),
            "CGO_CFLAGS": shlex.join(
                ["-std=c11", "-Wall", "-Wextra", "-Werror", "-I", str(include)]
            ),
            "CGO_LDFLAGS": shlex.join([str(library)]),
            "GOCACHE": str(cache),
            "GOTMPDIR": str(temporary),
        }
    )
    LOGGER.info("Go consumers: CGO_ENABLED=1 GOEXPERIMENT=%s", env["GOEXPERIMENT"])
    run([go, "test", "-count=1", "-v", "./..."], cwd=SOURCE / "tests" / "go", env=env)
    env.update(
        {
            "CGO_LDFLAGS": shlex.join(["-L", str(library.parent)]),
            "GOWORK": "off",
            "GO111MODULE": "on",
        }
    )
    run(
        [go, "test", "-count=1", "-v", "./..."],
        cwd=staging / "bindings" / "go",
        env=env,
    )


def main() -> int:
    logging.basicConfig(level=logging.INFO, format="%(message)s")
    args = parse_args()
    try:
        staging, include, library = build(tool("cargo"))
        if args.update_header:
            update_header(include / "qdk_circuit.h", REFERENCE_HEADER)
        if args.check_header or args.test or args.test_fortran:
            check_header(include / "qdk_circuit.h", REFERENCE_HEADER)
        if args.test:
            consumers(staging, include, library)
        if args.test_fortran:
            fortran_consumer(staging, include, library)
    except (
        OSError,
        RuntimeError,
        subprocess.CalledProcessError,
        ValueError,
        KeyError,
    ) as error:
        LOGGER.error("Circuit C API build failed: %s", error)
        return 1
    except KeyboardInterrupt:
        LOGGER.error("Interrupted")
        return 130
    return 0


if __name__ == "__main__":
    sys.exit(main())
