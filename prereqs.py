#!/usr/bin/env python3

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import os
import platform
import re
import sys
import subprocess
import tempfile
import functools
import shutil
from pathlib import Path

python_ver = (3, 11)  # Python support for Windows on ARM64 requires v3.11 or later
rust_ver = (1, 90, 0)  # Ensure Rust version 1.90 or later is installed
node_ver = (22, 14, 0)
rust_fmt_ver = (1, 8, 0)  # Current version when Rust 1.90 shipped
clippy_ver = (0, 1, 90)
wasm_bindgen_ver = (0, 2, 100)
binaryen_ver = 123

platform_sys = platform.system().lower()  # 'windows', 'darwin', or 'linux'
platform_arch = "arm64" if platform.machine().lower() in ["aarch64", "arm64"] else "x64"

# Disable buffered output so that the log statements and subprocess output get interleaved in proper order
print = functools.partial(print, flush=True)


def get_installed_msrust_targets() -> str:
    if shutil.which("msrustup"):
        try:
            # for some reason msrustup target list --installed doesn't work, so we omit --installed
            args = ["msrustup", "target", "list"]
            return subprocess.check_output(args, universal_newlines=True)
        except subprocess.CalledProcessError as e:
            message = (
                f"Unable to determine installed rust targets using msrustup: {str(e)}"
            )
            raise Exception(message) from e


def get_installed_rust_targets() -> str:
    msrust_targets = get_installed_msrust_targets()
    if msrust_targets:
        return msrust_targets

    if shutil.which("rustup"):
        try:
            args = ["rustup", "target", "list", "--installed"]
            return subprocess.check_output(args, universal_newlines=True)
        except subprocess.CalledProcessError as e:
            message = (
                f"Unable to determine installed rust targets using rustup: {str(e)}"
            )
            raise Exception(message) from e
    raise Exception("Unable to locate rustup or msrustup in PATH")


def add_wasm_tools_to_path():
    # Updating the PATH in a fresh env and using that in subprocess.run() doesn't use the
    # updated PATH to locate the binary on Windows, so we need to modify the current process's PATH.
    # See https://github.com/python/cpython/issues/105889

    bindgen_path = str(Path.home() / "wasm-bindgen")
    wasmopt_path = str(Path.home() / "binaryen" / "bin")

    if bindgen_path not in os.environ["PATH"]:
        print(f"Adding {bindgen_path} to PATH")
        os.environ["PATH"] += os.pathsep + bindgen_path

    if wasmopt_path not in os.environ["PATH"]:
        print(f"Adding {wasmopt_path} to PATH")
        os.environ["PATH"] += os.pathsep + wasmopt_path


def check_prereqs(install=False, skip_wasm=False):
    ### Check the Python version ###
    if (
        sys.version_info.major != python_ver[0]
        or sys.version_info.minor < python_ver[1]
    ):
        print(
            f"Python {python_ver[0]}.{python_ver[1]} or later is required. Please update"
        )
        exit(1)

    ### Check the Node.js version ###
    try:
        node_version = subprocess.check_output(["node", "-v"])
        print(f"Detected node.js version {node_version.decode()}")
    except FileNotFoundError:
        print("Node.js not found. Please install from https://nodejs.org/")
        exit(1)

    ver_match = re.search(r"v(\d+)\.(\d+)\.(\d+)", node_version.decode())
    if ver_match:
        found_ver = tuple(int(g) for g in ver_match.groups())
        if found_ver < node_ver:
            print(
                f"Node.js v{node_ver[0]}.{node_ver[1]}.{node_ver[2]} or later is required. Please update."
            )
            exit(1)
    else:
        raise Exception("Unable to determine the Node.js version.")

    ### Check the rustc compiler version ###
    try:
        rust_version = subprocess.check_output(["rustc", "--version"])
        print(f"Detected Rust version: {rust_version.decode()}")
    except FileNotFoundError:
        print("Rust compiler not found. Install from https://rustup.rs/")
        exit(1)

    ver_match = re.search(r"rustc (\d+)\.(\d+)\.(\d+)", rust_version.decode())
    if ver_match:
        found_ver = tuple(int(g) for g in ver_match.groups())
        if found_ver < rust_ver:
            print(
                f'Rust v{rust_ver[0]}.{rust_ver[1]} is required. Please update with "rustup default {rust_ver[0]}.{rust_ver[1]}"'
            )
            exit(1)
    else:
        raise Exception("Unable to determine the Rust compiler version.")

    ### Check the rustfmt version ###
    try:
        rust_fmt_version = subprocess.check_output(["cargo", "fmt", "--version"])
        print(f"Detected cargo fmt version: {rust_fmt_version.decode()}")
    except FileNotFoundError:
        print("cargo fmt not found. Install via rustup component add rustfmt")
        exit(1)

    ver_match = re.search(r"rustfmt (\d+)\.(\d+)\.(\d+)", rust_fmt_version.decode())
    if ver_match:
        found_ver = tuple(int(g) for g in ver_match.groups())
        if found_ver < rust_fmt_ver:
            print(
                f"cargo fmt v{rust_fmt_ver[0]}.{rust_fmt_ver[1]}.{rust_fmt_ver[2]} or later is required. Please update"
            )
            exit(1)
    else:
        raise Exception("Unable to determine the rustfmt version")

    ### Check the clippy version ###
    try:
        clippy_version = subprocess.check_output(["cargo", "clippy", "--version"])
        print(f"Detected cargo clippy version: {clippy_version.decode()}")
    except FileNotFoundError:
        print("cargo clippy not found. Install via rustup component add clippy")
        exit(1)

    ver_match = re.search(r"clippy (\d+)\.(\d+)\.(\d+)", clippy_version.decode())
    if ver_match:
        found_ver = tuple(int(g) for g in ver_match.groups())
        if found_ver < clippy_ver:
            print(
                f"clippy v{clippy_ver[0]}.{clippy_ver[1]}.{clippy_ver[2]} or later is required. Please update with 'rustup component add clippy'"
            )
            exit(1)
    else:
        raise Exception("Unable to determine the clippy version")

    installed_rust_targets = get_installed_rust_targets()

    if not skip_wasm:
        wasm_checks(install, installed_rust_targets)


def wasm_checks(install, installed_rust_targets):
    add_wasm_tools_to_path()

    ### Check the wasm-bindgen version ###
    try:
        wasm_bindgen_version = subprocess.check_output(["wasm-bindgen", "--version"])
        print(f"Detected wasm-bindgen version: {wasm_bindgen_version.decode()}")
    except FileNotFoundError:
        if install == True:
            print("wasm-bindgen not found. Attempting to install...")
            install_wasm_bindgen()
            wasm_bindgen_version = subprocess.check_output(
                ["wasm-bindgen", "--version"]
            )
        else:
            print(
                "wasm-bindgen not found. Install via 'python ./prereqs.py --install' or see https://github.com/rustwasm/wasm-bindgen"
            )
            exit(1)
    version_match = re.search(
        r"wasm-bindgen (\d+)\.(\d+)\.(\d+)", wasm_bindgen_version.decode()
    )
    if version_match:
        found_ver = tuple(int(g) for g in version_match.groups())
        if found_ver < wasm_bindgen_ver:
            print(
                f"wasm-bindgen v{wasm_bindgen_ver[0]}.{wasm_bindgen_ver[1]}.{wasm_bindgen_ver[2]} or later is required. Please update."
            )
            exit(1)
    else:
        print("Unable to determine the wasm-bindgen version")

    ### Check the binaryen version ###
    try:
        binaryen_version = subprocess.check_output(["wasm-opt", "--version"])
        print(f"Detected wasm-opt version: {binaryen_version.decode()}")
    except FileNotFoundError:
        if install == True:
            print("wasm-opt not found. Attempting to install...")
            install_binaryen()
            binaryen_version = subprocess.check_output(["wasm-opt", "--version"])
        else:
            print(
                "wasm-opt not found. Install via 'python ./prereqs.py --install' or see https://github.com/WebAssembly/binaryen"
            )
            exit(1)
    version_match = re.search(r"wasm-opt version (\d+)", binaryen_version.decode())
    if version_match:
        found_ver = int(version_match.group(1))
        if found_ver < binaryen_ver:
            print(f"wasm-opt version must be {binaryen_ver} or later. Please update.")
            exit(1)
    else:
        print("Unable to determine the wasm-opt version")
        exit(1)

    # Ensure the required wasm target is installed
    if "wasm32-unknown-unknown" not in installed_rust_targets:
        if install == True:
            print("Wasm Rust target not installed. Attempting to install...")
            subprocess.run(
                ["rustup", "target", "add", "wasm32-unknown-unknown"], check=True
            )
        else:
            print(
                "Wasm Rust target not installed. Install via 'rustup target add wasm32-unknown-unknown'"
            )
            exit(1)


def download_and_extract(url_base, tar_file, out_dir):
    os.makedirs(out_dir, exist_ok=True)
    temp_file = tempfile.gettempdir() + os.sep + tar_file

    # Note: Using curl and tar as subprocesses rather than Python libraries for features such as --strip-components
    subprocess.run(["curl", "-L", "-o", temp_file, url_base + tar_file], check=True)
    subprocess.run(
        ["tar", "-xzf", temp_file, "--strip-components=1", "-C", out_dir], check=True
    )

    os.remove(temp_file)  # Clean up the tar file


def install_wasm_bindgen():
    ver_str = ".".join(str(v) for v in wasm_bindgen_ver)
    # Maintain the below mappings as filenames are inconsistent, and we want x64 builds on Windows ARM64
    wasm_bindgen_tar_map = {
        "darwin": {
            "arm64": f"wasm-bindgen-{ver_str}-aarch64-apple-darwin.tar.gz",
            "x64": f"wasm-bindgen-{ver_str}-x86_64-apple-darwin.tar.gz",
        },
        "linux": {
            "arm64": f"wasm-bindgen-{ver_str}-aarch64-unknown-linux-gnu.tar.gz",
            "x64": f"wasm-bindgen-{ver_str}-x86_64-unknown-linux-musl.tar.gz",
        },
        "windows": {
            "arm64": f"wasm-bindgen-{ver_str}-x86_64-pc-windows-msvc.tar.gz",
            "x64": f"wasm-bindgen-{ver_str}-x86_64-pc-windows-msvc.tar.gz",
        },
    }
    wasm_bindgen_base_url = (
        f"https://github.com/rustwasm/wasm-bindgen/releases/download/{ver_str}/"
    )
    wasm_bindgen_filename = wasm_bindgen_tar_map[platform_sys][platform_arch]
    out_dir = Path.home() / "wasm-bindgen"

    download_and_extract(wasm_bindgen_base_url, wasm_bindgen_filename, str(out_dir))
    # File of interest will be in "~/wasm-bindgen/wasm-bindgen"


def install_binaryen():
    binaryen_tar_map = {
        "darwin": {
            "arm64": f"binaryen-version_{binaryen_ver}-arm64-macos.tar.gz",
            "x64": f"binaryen-version_{binaryen_ver}-x86_64-macos.tar.gz",
        },
        "linux": {
            "arm64": f"binaryen-version_{binaryen_ver}-aarch64-linux.tar.gz",
            "x64": f"binaryen-version_{binaryen_ver}-x86_64-linux.tar.gz",
        },
        "windows": {
            "arm64": f"binaryen-version_{binaryen_ver}-x86_64-windows.tar.gz",
            "x64": f"binaryen-version_{binaryen_ver}-x86_64-windows.tar.gz",
        },
    }
    binaryen_base_url = f"https://github.com/WebAssembly/binaryen/releases/download/version_{binaryen_ver}/"
    binaryen_filename = binaryen_tar_map[platform_sys][platform_arch]

    out_dir = Path.home() / "binaryen"
    download_and_extract(binaryen_base_url, binaryen_filename, str(out_dir))
    # File of interest will be in "~/binaryen/bin/wasm-opt"


if __name__ == "__main__":
    skip_wasm = "--skip-wasm" in sys.argv
    install = "--install" in sys.argv
    check_prereqs(install=install, skip_wasm=skip_wasm)
