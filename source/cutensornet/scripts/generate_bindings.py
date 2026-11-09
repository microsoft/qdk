#!/usr/bin/env python3

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

"""Generates the crate's Rust declarations of NVIDIA's C APIs, which are
checked in: the bindings of each library in src/libraries/ whose folder has
a bindings.toml, written to that folder's generated/ folder; now the CUDA
Runtime's (cuda_runtime/generated/cudart_12.rs) and cuTensorNet's
(cutensornet/generated/v2_13.rs).

Context: the crate opens the CUDA Runtime and cuTensorNet at run time, so
neither its build nor its users need NVIDIA's SDK. Calling a library still
needs Rust declarations of its functions and types, and they must match
NVIDIA's headers exactly: a wrong declaration is undefined behaviour at run
time, not a compile error.

What: for each library, bindgen's output for its header in a pinned NVIDIA
archive: the library's whole API, every function, type and constant whose
name has the library's prefix, and the types they reach. The crate's
function tables pick the functions it uses, and each row is checked, when
the crate compiles, against its function's declaration; so the tables are
the only list of those functions, and binding another function of a bound
release needs no new bindings.

Why a script: the declarations are generated, never written by hand, and
every input that can change them is pinned (the archives, checked by hash;
bindgen, the libclang it parses with and rustfmt, checked by version; the
target and the Rust edition, given to them), so the checked-in files can be
reproduced, and a reviewer checks the pins, not the declarations. The build
never runs it.

The pins are in two places:
- scripts/generate_bindings.toml pins what every library shares: bindgen,
  the libclang it parses with, rustfmt, the target, the Rust edition, and
  NVIDIA's archives;
- each library's folder, src/libraries/<library>/, has a bindings.toml, which
  says what its bindings are made of: its header, the archives clang
  searches for it, the prefix of its API's names, and the file written (see
  `Library`). The script binds every library that has one.
The generated file's header names the header it comes from and that
header's archive. The generated/ folders hold only the script's output;
the crate's .gitattributes marks them as generated, so that GitHub hides
them in a pull request's diff.

Running it: on Ubuntu 22.04, because the pinned libclang is that of its
clang package (the script checks the exact version string, since another
libclang can declare the same header differently); it has been run on
x86-64. Another host could run it in an ubuntu:22.04 container (not tried).
No GPU and no CUDA installation are needed, as the headers come from
NVIDIA's archives.

    sudo apt-get install build-essential clang libclang-dev pkg-config python3.11
    # Rust 1.95 with rustfmt, through rustup; then:
    cargo install bindgen-cli --version 0.72.1 --locked

Download the archives of generate_bindings.toml (each `file`, from its `url`)
into one folder; the script downloads nothing, and reports each missing
archive with its URL. Then, from the repository's root:

    python3.11 source/cutensornet/scripts/generate_bindings.py --archives DIR --check
    python3.11 source/cutensornet/scripts/generate_bindings.py --archives DIR

The first changes nothing: it prints the difference with each checked-in
file, and fails when there is one. The second writes the files; review them
with `git diff`.

When to run it, once per release of a library:
- to bind another library: follow the steps in src/libraries.rs;
- to audit a new release: pin its archives in generate_bindings.toml,
  regenerate, review the bindings' diff, check the layout assertions against
  the new headers (as src/libraries/cutensornet/layout.rs), and change the
  accepted version in the library's module, which the crate asserts, when
  it compiles, is the version its header declares; rename the bindings'
  file (`output`) and module if its name gives the version (v2_13);
- with --check, to confirm that the checked-in files are reproduced.
Binding another function of a bound release needs no run: add its row to
its library's function table, and its function-pointer type (the library's
module says where).

How:
1. find the libraries, and read their pins;
2. check the tools' versions and the archives' hashes;
3. extract the archives' include/ folders;
4. for each library, run bindgen on its header for the names with its
   prefix, format the output, and check that it declares a function;
5. add the header, then write the output, or with --check compare it with
   the checked-in file.
"""

from __future__ import annotations

import argparse
import difflib
import hashlib
import platform
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from typing import NoReturn

if sys.version_info < (3, 11):
    sys.exit("generate_bindings: needs Python 3.11 or later (for tomllib)")

import tomllib

CRATE_DIR = Path(__file__).resolve().parent.parent
CONFIG_PATH = CRATE_DIR / "scripts" / "generate_bindings.toml"
LIBRARIES_DIR = CRATE_DIR / "src" / "libraries"

OUTPUT_HEADER = """\
// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @generated by scripts/generate_bindings.py; do not edit.
// Source: {header} of {archive}.
"""


def main(argv: list[str] | None = None) -> int:
    arguments = parse_arguments(argv)
    config = load_config()
    check_toolchain(config.toolchain)
    archives = locate_archives(config, arguments.archives)

    differing = []
    with tempfile.TemporaryDirectory(prefix="generate-bindings-") as temp:
        work = Path(temp)
        include_dirs = extract_includes(archives, work)
        for library in config.libraries:
            bindings = generate(library, config, include_dirs, work)
            if not arguments.check:
                library.output_path.parent.mkdir(exist_ok=True)
                library.output_path.write_text(bindings, encoding="utf-8")
                report(library, "written", bindings)
            elif matches_checked_in(library.output_path, bindings):
                report(library, "identical to the checked-in file", bindings)
            else:
                report(library, "differs from the checked-in file", bindings)
                differing.append(library.name)

    if differing:
        fail(f"differs from the checked-in file: {', '.join(differing)}")
    return 0


def parse_arguments(argv: list[str] | None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Generates the crate's bindings of NVIDIA's libraries from the "
            "pinned inputs of scripts/generate_bindings.toml and of each "
            "library's bindings.toml."
        ),
        epilog=(
            "Needs Python 3.11 or later, and the bindgen and libclang pinned "
            "in generate_bindings.toml; this script's docstring says how to "
            "install them. Downloads nothing: a missing archive is reported "
            "with its URL."
        ),
    )
    parser.add_argument(
        "--archives",
        type=Path,
        required=True,
        metavar="DIR",
        help="the folder holding NVIDIA's archives, named as in "
        "generate_bindings.toml",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="compare with the checked-in files, printing any difference, "
        "instead of writing them",
    )
    return parser.parse_args(argv)


# The pins.


@dataclass(frozen=True)
class Config:
    toolchain: Toolchain
    archives: dict[str, Archive]
    libraries: list[Library]


@dataclass(frozen=True)
class Toolchain:
    """The tools, as `bindgen --version --verbose` names bindgen and the
    libclang it parses with and `rustfmt --version` names rustfmt, and the
    inputs bindgen and rustfmt are given."""

    bindgen: str
    libclang: str
    rustfmt: str
    rust_edition: str
    target: str


@dataclass(frozen=True)
class Archive:
    """An NVIDIA archive, downloaded by hand from `url` as `file`."""

    file: str
    url: str
    sha256: str


@dataclass(frozen=True)
class Library:
    """A library to bind. `name` is its module in src/libraries/; its pins
    are in that module's folder, in bindings.toml:

    - header: the header bindgen reads, found in the first include folder
      that has it;
    - includes: the archives whose include/ folders clang searches, in order;
    - prefix: the prefix of the names of the library's API, in any case
      (`cutensornet` matches cutensornetCreate and CUTENSORNET_VERSION):
      bindgen declares every function, type and constant of the header whose
      name has it, and the types they reach;
    - output: the file the bindings are written to, in the generated/ folder
      of the library's folder; its name is the bindings' module;
    - blocklist_types: the types another library's bindings declare, which
      raw_lines import.
    """

    name: str
    header: str
    includes: list[str]
    prefix: str
    output: str
    blocklist_types: list[str] = field(default_factory=list)
    raw_lines: list[str] = field(default_factory=list)

    @property
    def output_path(self) -> Path:
        return LIBRARIES_DIR / self.name / "generated" / self.output


def load_config() -> Config:
    """Reads scripts/generate_bindings.toml, and the bindings.toml of each
    library that has one. A missing or unknown key fails, and so does an
    archive a library names that is not pinned."""
    pins = read_toml(CONFIG_PATH)
    try:
        toolchain = Toolchain(**pins["toolchain"])
        archives = {
            name: Archive(**archive) for name, archive in pins["archives"].items()
        }
        if pins.keys() != {"toolchain", "archives"}:
            raise KeyError(sorted(pins.keys() - {"toolchain", "archives"}))
    except (KeyError, TypeError) as error:
        fail(f"{CONFIG_PATH}: missing or unknown key: {error}")
    names = sorted(path.parent.name for path in LIBRARIES_DIR.glob("*/bindings.toml"))
    if not names:
        fail(f"{LIBRARIES_DIR}: no library folder has a bindings.toml")
    libraries = [load_library(name) for name in names]
    unpinned = sorted(
        {name for library in libraries for name in library.includes} - archives.keys()
    )
    if unpinned:
        fail(f"{CONFIG_PATH}: archives not in [archives]: {unpinned}")
    return Config(toolchain, archives, libraries)


def load_library(name: str) -> Library:
    """The library `name`, from its pins."""
    path = LIBRARIES_DIR / name / "bindings.toml"
    pins = read_toml(path)
    try:
        return Library(name=name, **pins)
    except TypeError as error:
        fail(f"{path}: missing or unknown key: {error}")


def read_toml(path: Path) -> dict:
    try:
        return tomllib.loads(path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        fail(f"{path}: {error}")


# The inputs: the tools and the archives.


def check_toolchain(toolchain: Toolchain) -> None:
    """Checks that bindgen, the libclang it loads and rustfmt are the pinned
    versions; prints the environment."""
    missing = [tool for tool in ("bindgen", "rustfmt") if shutil.which(tool) is None]
    if missing:
        fail(f"not found on PATH: {', '.join(missing)}")
    found = run(["bindgen", "--version", "--verbose"]).strip().splitlines()
    pinned = [toolchain.bindgen, f"Clang: {toolchain.libclang}"]
    if found != pinned:
        fail(f"bindgen --version --verbose printed {found}, expected {pinned}")
    rustfmt = run(["rustfmt", "--version"]).strip()
    if rustfmt != toolchain.rustfmt:
        fail(f"rustfmt --version printed {rustfmt!r}, expected {toolchain.rustfmt!r}")
    print(
        f"host: {platform.system()} {platform.machine()}, Python {platform.python_version()}"
    )
    print(f"tools: {toolchain.bindgen} with libclang {toolchain.libclang}; {rustfmt}")
    print(f"target: {toolchain.target}, Rust edition {toolchain.rust_edition}")


def locate_archives(config: Config, folder: Path) -> dict[str, Path]:
    """The archives the libraries include, found in `folder` by their pinned
    file names and checked against their pinned hashes. Every missing or
    differing archive is reported, a missing one with its URL."""
    needed = dict.fromkeys(
        name for library in config.libraries for name in library.includes
    )
    paths, faults = {}, []
    for name in needed:
        archive = config.archives[name]
        path = folder / archive.file
        if not path.is_file():
            faults.append(f"missing {path}; download it from {archive.url}")
        elif (digest := sha256(path)) != archive.sha256:
            faults.append(f"{path}: SHA-256 {digest}, expected {archive.sha256}")
        else:
            paths[name] = path
            print(f"archive {archive.file}: SHA-256 checked")
    if faults:
        fail("archives:\n  " + "\n  ".join(faults))
    return paths


def extract_includes(archives: dict[str, Path], work: Path) -> dict[str, Path]:
    """Each archive's include/ folder, extracted under `work`."""
    return {
        name: extract_include(path, work / "archives" / name)
        for name, path in archives.items()
    }


def extract_include(archive: Path, destination: Path) -> Path:
    """Extracts the include/ folder under the archive's top-level folder into
    `destination`, and returns it."""
    # The archives are pinned by hash; the data filter, where Python has it,
    # also keeps every member inside `destination`.
    options = {"filter": "data"} if hasattr(tarfile, "data_filter") else {}
    with tarfile.open(archive, "r:xz") as tar:
        members = [
            member for member in tar if member.name.split("/")[1:2] == ["include"]
        ]
        if not members:
            fail(f"{archive}: no include/ folder")
        tar.extractall(destination, members=members, **options)
    return destination / members[0].name.split("/")[0] / "include"


# The generation.


def generate(
    library: Library, config: Config, include_dirs: dict[str, Path], work: Path
) -> str:
    """The library's bindings, with their header, once checked to declare a
    function."""
    includes = [include_dirs[name] for name in library.includes]
    archive, header = find_header(library, include_dirs)
    output = work / f"{library.name}.rs"
    run_bindgen(library, header, includes, config.toolchain.target, output)
    # Bindgen formats with rustfmt's default edition, which wraps some return
    # types differently from the crate's.
    run(["rustfmt", "--edition", config.toolchain.rust_edition, str(output)])
    bindings = output.read_text(encoding="utf-8")
    check_declares_functions(library, bindings)
    file_header = OUTPUT_HEADER.format(
        header=library.header, archive=config.archives[archive].file
    )
    return file_header + bindings


def find_header(library: Library, include_dirs: dict[str, Path]) -> tuple[str, Path]:
    """The library's header, in the first of its archives' include folders
    that has it, as clang would find it, and that archive's name."""
    for name in library.includes:
        if (include_dirs[name] / library.header).is_file():
            return name, include_dirs[name] / library.header
    fail(
        f"{library.name}: {library.header} is in none of the archives {library.includes}"
    )


def run_bindgen(
    library: Library, header: Path, includes: list[Path], target: str, output: Path
) -> None:
    """Runs bindgen on `header` for the library's API, the functions, types
    and constants whose names have its prefix, and the types they reach,
    writing `output`."""
    command = ["bindgen", str(header), "--output", str(output), "--no-layout-tests"]
    api = any_case_prefix(library.prefix)
    for kind in ("function", "type", "var"):
        command += [f"--allowlist-{kind}", api]
    if library.blocklist_types:
        command += ["--blocklist-type", exactly(library.blocklist_types)]
    for line in library.raw_lines:
        command += ["--raw-line", line]
    command += ["--", f"--target={target}", *(f"-I{folder}" for folder in includes)]
    run(command)


def any_case_prefix(prefix: str) -> str:
    """A bindgen pattern matching the names that start with `prefix` in any
    case, such as cudaMalloc and CUDART_VERSION for "cuda": a class per
    letter, ^[Cc][Uu][Dd][Aa].*$. Not (?i): bindgen's regex crate lacks
    Unicode case folding, so it rejects the pattern, and bindgen, warning
    only in its log, then matches nothing."""
    letters = (
        f"[{c.upper()}{c.lower()}]" if c.isalpha() else re.escape(c) for c in prefix
    )
    return f"^{''.join(letters)}.*$"


def exactly(names: list[str]) -> str:
    """A bindgen pattern matching exactly `names`."""
    return f"^({'|'.join(re.escape(name) for name in names)})$"


def check_declares_functions(library: Library, bindings: str) -> None:
    """Checks that `bindings` declare a function: bindgen declares nothing,
    without a word, for a prefix no name of the header has."""
    if not re.search(r"pub fn \w+\s*\(", bindings):
        fail(
            f"{library.name}: no function of {library.header} has the prefix "
            f"{library.prefix!r}"
        )


# The output.


def matches_checked_in(path: Path, generated: str) -> bool:
    """Whether the checked-in file is `generated`; if not, prints the
    difference."""
    current = path.read_text(encoding="utf-8") if path.is_file() else ""
    if current == generated:
        return True
    relative = path.relative_to(CRATE_DIR)
    sys.stdout.writelines(
        difflib.unified_diff(
            current.splitlines(keepends=True),
            generated.splitlines(keepends=True),
            fromfile=f"{relative} (checked in)",
            tofile=f"{relative} (generated)",
        )
    )
    return False


def report(library: Library, verdict: str, bindings: str) -> None:
    digest = hashlib.sha256(bindings.encode()).hexdigest()
    functions = len(re.findall(r"pub fn \w+\s*\(", bindings))
    lines = bindings.count("\n")
    relative = library.output_path.relative_to(CRATE_DIR)
    print(
        f"{relative}: {verdict} ({functions} functions, "
        f"{lines} lines, SHA-256 {digest})"
    )


# Helpers.


def run(command: list[str]) -> str:
    result = subprocess.run(command, capture_output=True, text=True)
    if result.returncode != 0:
        fail(f"{command[0]} failed ({result.returncode}):\n{result.stderr}")
    return result.stdout


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def fail(message: str) -> NoReturn:
    raise SystemExit(f"generate_bindings: {message}")


if __name__ == "__main__":
    sys.exit(main())
