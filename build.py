#!/usr/bin/env python3

# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

import argparse
from glob import glob
import os
import platform
import sys
import time
import venv
import shutil
import subprocess
import functools

from prereqs import check_prereqs, add_wasm_tools_to_path

# Disable buffered output so that the log statements and subprocess output get interleaved in proper order
print = functools.partial(print, flush=True)

parser = argparse.ArgumentParser(
    description="Builds all projects in the repo, unless specific projects to build are passed "
    "as options, in which case only those projects are built."
)

parser.add_argument(
    "--cli", action="store_true", help="Build the command-line compiler"
)
parser.add_argument("--pip", action="store_true", help="Build the pip wheel")
parser.add_argument("--widgets", action="store_true", help="Build the Python widgets")
parser.add_argument("--wasm", action="store_true", help="Build the WebAssembly files")
parser.add_argument("--npm", action="store_true", help="Build the npm package")
parser.add_argument("--play", action="store_true", help="Build the web playground")
parser.add_argument("--vscode", action="store_true", help="Build the VS Code extension")
parser.add_argument(
    "--jupyterlab", action="store_true", help="Build the JupyterLab extension"
)

parser.add_argument(
    "--debug", action="store_true", help="Create a debug build (default is release)"
)
parser.add_argument(
    "--test",
    action=argparse.BooleanOptionalAction,
    default=True,
    help="Run the tests (default is --test)",
)

# Below allows for passing --no-check to avoid the default of True
parser.add_argument(
    "--check",
    action=argparse.BooleanOptionalAction,
    default=True,
    help="Run the linting and formatting checks (default is --check)",
)

parser.add_argument(
    "--check-prereqs",
    action=argparse.BooleanOptionalAction,
    default=True,
    help="Run the prerequisites check (default is --check-prereqs)",
)

parser.add_argument(
    "--integration-tests",
    action=argparse.BooleanOptionalAction,
    default=False,
    help="Build and run the integration tests (default is --no-integration-tests)",
)

parser.add_argument(
    "--web-only",
    action=argparse.BooleanOptionalAction,
    default=False,
    help="Build only the web version of the wasm package",
)

parser.add_argument(
    "--ci-bench",
    action=argparse.BooleanOptionalAction,
    default=False,
    help="Run the benchmarking script that is run in CI (default is --no-ci-bench)",
)

parser.add_argument(
    "--manylinux",
    action=argparse.BooleanOptionalAction,
    default=False,
    help="Run the manylinux auditwheel repair (default is --no-manylinux)",
)

args = parser.parse_args()

if args.check_prereqs:
    check_prereqs()

# If no specific project given then build all
build_all = (
    not args.cli
    and not args.pip
    and not args.widgets
    and not args.wasm
    and not args.npm
    and not args.play
    and not args.vscode
    and not args.jupyterlab
    and not args.ci_bench
)
build_cli = build_all or args.cli
build_pip = build_all or args.pip
build_widgets = build_all or args.widgets
build_wasm = build_all or args.wasm
build_npm = build_all or args.npm
build_play = build_all or args.play
build_vscode = build_all or args.vscode
build_jupyterlab = build_all or args.jupyterlab
ci_bench = args.ci_bench

# JavaScript projects and eslint, prettier depend on npm_install
# However the JupyterLab extension uses yarn in a separate workspace
npm_install_needed = (
    build_npm or build_play or build_vscode or build_jupyterlab or args.check
)
npm_cmd = "npm.cmd" if platform.system() == "Windows" else "npm"

build_type = "debug" if args.debug else "release"
wasm_targets = ["web", "nodejs"] if not args.web_only else ["web"]
run_tests = args.test

root_dir = os.path.dirname(os.path.abspath(__file__))
qdk_src_dir = os.path.join(root_dir, "source")
wasm_src = os.path.join(qdk_src_dir, "wasm")
wasm_bld = os.path.join(root_dir, "target", "wasm32", build_type)
samples_src = os.path.join(root_dir, "samples")
npm_src = os.path.join(qdk_src_dir, "npm", "qsharp")
play_src = os.path.join(qdk_src_dir, "playground")
pip_src = os.path.join(qdk_src_dir, "pip")
widgets_src = os.path.join(qdk_src_dir, "widgets")
wheels_dir = os.path.join(root_dir, "target", "wheels")
vscode_src = os.path.join(qdk_src_dir, "vscode")
jupyterlab_src = os.path.join(qdk_src_dir, "jupyterlab")


def step_start(description):
    global start_time
    prefix = "::group::" if os.getenv("GITHUB_ACTIONS") == "true" else ""
    print(f"{prefix}build.py: {description}")
    start_time = time.time()


def step_end():
    global start_time
    duration = time.time() - start_time
    print(f"build.py: Finished in {duration:.3f}s.")
    if os.getenv("GITHUB_ACTIONS") == "true":
        print(f"::endgroup::")


def use_python_env(folder):
    # Check if in a virtual environment
    if (
        os.environ.get("VIRTUAL_ENV") is None
        and os.environ.get("CONDA_PREFIX") is None
        and os.environ.get("CI") is None
    ):
        print("Not in a virtual python environment")

        venv_dir = os.path.join(folder, ".venv")
        # Create virtual environment under repo root
        if not os.path.exists(venv_dir):
            print(f"Creating a virtual environment under {venv_dir}")
            venv.main([venv_dir])

        # Check if .venv/bin/python exists, otherwise use .venv/Scripts/python.exe (Windows)
        python_bin = os.path.join(venv_dir, "bin", "python")
        if not os.path.exists(python_bin):
            python_bin = os.path.join(venv_dir, "Scripts", "python.exe")
        print(f"Using python from {python_bin}")
    else:
        # Already in a virtual environment, use current Python
        python_bin = sys.executable

    return python_bin


if npm_install_needed:
    step_start("Running npm install")
    subprocess.run([npm_cmd, "install"], check=True, text=True, cwd=root_dir)
    step_end()

if args.check:
    step_start("Running eslint and prettier checks")
    try:
        subprocess.run([npm_cmd, "run", "check"], check=True, text=True, cwd=root_dir)
    except subprocess.CalledProcessError:
        print("Consider running 'npm run prettier:fix' to fix prettier errors.")
        raise

    if build_wasm or build_cli:
        # If we're going to check the Rust code, do this before we try to compile it
        print("Running the cargo fmt and clippy checks")
        subprocess.run(
            ["cargo", "fmt", "--all", "--", "--check"],
            check=True,
            text=True,
            cwd=root_dir,
        )
        subprocess.run(
            [
                "cargo",
                "clippy",
                "--all-targets",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
            check=True,
            text=True,
            cwd=root_dir,
        )

    if build_cli:
        print("Running Q# format check")
        subprocess.run(
            [
                "cargo",
                "run",
                "--bin",
                "qsc_formatter",
                "--",
                "./library/",
                "./samples/",
                "-r",
            ],
            check=True,
            text=True,
            cwd=root_dir,
        )
    step_end()

if build_cli:
    if run_tests:
        step_start("Running Rust unit tests")
        cargo_test_args = ["cargo", "test"]
        if build_type == "release":
            cargo_test_args.append("--release")
            # Disable LTO for release tests to speed up compilation
            cargo_test_args.append("--config")
            cargo_test_args.append('profile.release.lto="off"')
        subprocess.run(cargo_test_args, check=True, text=True, cwd=root_dir)
        step_end()


def install_qsharp_python_package(cwd, wheelhouse, interpreter):
    command_args = [
        interpreter,
        "-m",
        "pip",
        "install",
        "--force-reinstall",
        "--no-index",
        "--find-links=" + wheelhouse,
        "qsharp",
    ]
    subprocess.run(command_args, check=True, text=True, cwd=cwd)


def repair_manylinux_wheels(cwd, wheelhouse, interpreter):
    if platform.system() != "Linux":
        print("Not on Linux, skipping manylinux repair")
        return

    tag = "manylinux_2_35_x86_64"

    if platform.machine() == "aarch64":
        tag = "manylinux_2_35_aarch64"

    command_args = [
        interpreter,
        "-m",
        "pip",
        "install",
        "auditwheel",
        "patchelf",
    ]
    subprocess.run(command_args, check=True, text=True, cwd=cwd)

    wheel_files = glob(wheelhouse + "/qsharp-*.whl")

    for wheel in wheel_files:
        command_args = [
            interpreter,
            "-m",
            "auditwheel",
            "repair",
            "--plat",
            tag,
            "--wheel-dir",
            wheelhouse,
            wheel,
        ]
        subprocess.run(command_args, check=True, text=True, cwd=cwd)

        if "manylinux" not in wheel:
            subprocess.run(["rm", wheel], check=True, text=True, cwd=cwd)


# If any package fails to install when using a requirements file, the entire
# process will fail with unpredicatable state of installed packages. To avoid
# this, we install each package individually from the requirements file.
#
# The reason we allow failures is that tooling for integration tests may not
# be available on all platforms, so we don't want to fail the build if we can't
# run the tests. The CI will run the tests on the platforms where the tooling
# is available giving us the confidence that the tests pass on those platforms.
def install_python_test_requirements(cwd, interpreter, check: bool = True):
    requirements_file_path = os.path.join(cwd, "test_requirements.txt")
    with open(requirements_file_path, "r", encoding="utf-8") as f:
        # Skip empty lines
        requirements = [line for line in f if line.strip()]
    for requirement in requirements:
        command_args = [
            interpreter,
            "-m",
            "pip",
            "install",
            requirement,
            "--only-binary",
            "qirrunner",
            "--only-binary",
            "pyqir",
        ]
        subprocess.run(command_args, check=check, text=True, cwd=cwd)


def build_qsharp_wheel(cwd, out_dir, interpreter, pip_env_dir):
    command_args = [
        interpreter,
        "-m",
        "pip",
        "wheel",
        "--wheel-dir",
        out_dir,
        "-v",
        cwd,
    ]
    subprocess.run(command_args, check=True, text=True, cwd=cwd, env=pip_env_dir)


def run_python_tests(cwd, interpreter):
    command_args = [interpreter, "-m", "pytest"]
    subprocess.run(command_args, check=True, text=True, cwd=cwd)


def run_python_integration_tests(cwd, interpreter):
    # don't check to see if pip succeeds. We'll see if the import works later.
    # If it doesn't, we'll skip the tests.
    command_args = [interpreter, "-m", "pytest"]
    subprocess.run(command_args, check=True, text=True, cwd=cwd)


def run_ci_historic_benchmark():
    branch = "main"
    output = subprocess.check_output(
        [
            "git",
            "rev-list",
            "--since=1 week ago",
            "--pretty=format:%ad__%h",
            "--date=short",
            branch,
        ]
    ).decode("utf-8")
    print("\n".join([line for i, line in enumerate(output.split("\n")) if i % 2 == 1]))

    output = subprocess.check_output(
        [
            "git",
            "rev-list",
            "--since=1 week ago",
            "--pretty=format:%ad__%h",
            "--date=short",
            branch,
        ]
    ).decode("utf-8")
    date_and_commits = [line for i, line in enumerate(output.split("\n")) if i % 2 == 1]

    for date_and_commit in date_and_commits:
        print("benching commit", date_and_commit)
        result = subprocess.run(
            [
                "cargo",
                "criterion",
                "--message-format=json",
                "--history-id",
                date_and_commit,
            ],
            capture_output=True,
            text=True,
        )
        with open(f"{date_and_commit}.json", "w") as f:
            f.write(result.stdout)


if build_pip:
    step_start("Building the pip package")

    python_bin = use_python_env(pip_src)

    # copy the process env vars
    pip_env: dict[str, str] = os.environ.copy()
    if platform.system() == "Darwin":
        # if on mac, add the arch flags for universal binary
        pip_env["ARCHFLAGS"] = "-arch x86_64 -arch arm64"

    build_qsharp_wheel(pip_src, wheels_dir, python_bin, pip_env)
    step_end()

    if run_tests:
        step_start("Running tests for the pip package")

        install_python_test_requirements(pip_src, python_bin)
        install_qsharp_python_package(pip_src, wheels_dir, python_bin)
        run_python_tests(os.path.join(pip_src, "tests"), python_bin)

        step_end()

    if args.integration_tests:
        step_start("Running integration tests for the pip package")
        test_dir = os.path.join(pip_src, "tests-integration")

        install_python_test_requirements(test_dir, python_bin, check=False)
        install_qsharp_python_package(pip_src, wheels_dir, python_bin)

        run_python_integration_tests(test_dir, python_bin)

        step_end()

    if args.manylinux:
        step_start("Repairing manylinux wheels")

        repair_manylinux_wheels(pip_src, wheels_dir, python_bin)

        step_end()

if build_widgets:
    step_start("Building the Python widgets")

    widgets_build_args = [
        sys.executable,
        "-m",
        "pip",
        "wheel",
        "--no-deps",
        "--wheel-dir",
        wheels_dir,
        widgets_src,
    ]
    subprocess.run(widgets_build_args, check=True, text=True, cwd=widgets_src)

    step_end()

if build_wasm:
    step_start("Building the wasm files")

    add_wasm_tools_to_path()  # Run again here in case --skip-prereqs was passed

    platform_sys = platform.system().lower()  # 'windows', 'darwin', or 'linux'

    # First build the wasm crate with something like:
    #   cargo build --lib [--release] --target wasm32-unknown-unknown --target-dir ./target
    #
    # Binary will be written to ./target/wasm32-unknown-unknown/{debug,release}/qsc_wasm.wasm
    cargo_args = [
        "cargo",
        "build",
        "--lib",
        "--target",
        "wasm32-unknown-unknown",
    ]
    if build_type == "release":
        cargo_args.append("--release")
    subprocess.run(cargo_args, check=True, text=True, cwd=wasm_src)

    # Next, create the JavaScript glue code using wasm-bindgen with something like:
    #   wasm-bindgen --target <nodejs|web> [--debug] --out-dir ./target/wasm32/{release,debug}/{nodejs|web} <infile>
    #
    # The output will be written to {out-dir}/qsc_wasm_bg.wasm
    for target_platform in wasm_targets:
        out_dir = os.path.join(wasm_bld, target_platform)
        in_file = os.path.join(
            root_dir, "target", "wasm32-unknown-unknown", build_type, "qsc_wasm.wasm"
        )

        wasm_bindgen_args = [
            "wasm-bindgen",
            "--target",
            target_platform,
            "--out-dir",
            out_dir,
        ]
        if build_type == "debug":
            wasm_bindgen_args.append("--debug")
        wasm_bindgen_args.append(in_file)

        subprocess.run(wasm_bindgen_args, check=True, text=True, cwd=wasm_src)

        # Also run wasm-opt to optimize the wasm file for release builds with:
        #   wasm-opt -Oz --enable-{<as needed>} --output <outfile> <infile>
        #
        # -Oz does extra size optimizations, and features are enabled to match Rust defaults
        # to avoid mismatch errors, as wasm-opt currently disables some of these by default.
        # See <https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html#enabled-webassembly-features>
        #
        # This updates the wasm file in place.
        #
        # Note: wasm-opt is not needed for debug builds, so we only run it for release builds.
        if build_type == "release":
            wasm_file = os.path.join(out_dir, "qsc_wasm_bg.wasm")
            wasm_opt_args = [
                "wasm-opt",
                "-Oz",
                "--enable-bulk-memory",
                "--enable-nontrapping-float-to-int",
                "--output",
                wasm_file,
                wasm_file,
            ]
            subprocess.run(wasm_opt_args, check=True, text=True, cwd=wasm_src)

        # After building, copy the artifacts to the npm location
        lib_dir = os.path.join(npm_src, "lib", target_platform)
        os.makedirs(lib_dir, exist_ok=True)

        for filename in ["qsc_wasm_bg.wasm", "qsc_wasm.d.ts", "qsc_wasm.js"]:
            fullpath = os.path.join(out_dir, filename)

            # To make the node files CommonJS modules, the extension needs to change
            # (This is because the package is set to ECMAScript modules by default)
            if target_platform == "nodejs" and filename == "qsc_wasm.js":
                filename = "qsc_wasm.cjs"
            if target_platform == "nodejs" and filename == "qsc_wasm.d.ts":
                filename = "qsc_wasm.d.cts"

            shutil.copy2(fullpath, os.path.join(lib_dir, filename))

    step_end()

if build_npm:
    step_start("Building the npm package")

    npm_args = [npm_cmd, "run", "build"]
    subprocess.run(npm_args, check=True, text=True, cwd=npm_src)

    if run_tests:
        print("Running tests for the npm package")
        npm_test_args = ["node", "--test"]
        subprocess.run(npm_test_args, check=True, text=True, cwd=npm_src)
    step_end()

if build_play:
    step_start("Building the playground")
    play_args = [npm_cmd, "run", "build"]
    subprocess.run(play_args, check=True, text=True, cwd=play_src)
    step_end()

if build_vscode:
    step_start("Building the VS Code extension")
    vscode_args = [npm_cmd, "run", "build"]
    subprocess.run(vscode_args, check=True, text=True, cwd=vscode_src)
    step_end()
    if args.integration_tests:
        step_start("Running the VS Code integration tests")
        vscode_args = [npm_cmd, "test"]
        subprocess.run(vscode_args, check=True, text=True, cwd=vscode_src)
        step_end()


if build_jupyterlab:
    step_start("Building the JupyterLab extension")

    python_bin = use_python_env(jupyterlab_src)

    pip_build_args = [
        python_bin,
        "-m",
        "pip",
        "wheel",
        "--wheel-dir",
        wheels_dir,
        jupyterlab_src,
    ]
    subprocess.run(pip_build_args, check=True, text=True, cwd=jupyterlab_src)
    step_end()

if build_pip and build_widgets and args.integration_tests:
    step_start("Running notebook samples integration tests")
    # Find all notebooks in the samples directory. Skip the basic sample and the azure submission sample, since those won't run
    # nicely in automation.
    notebook_files = [
        os.path.join(dp, f)
        for dp, _, filenames in os.walk(samples_src)
        for f in filenames
        if f.endswith(".ipynb")
        and not (
            f.startswith("sample.")
            or f.startswith("azure_submission.")
            or f.startswith("circuits.")
            or f.startswith("iterative_phase_estimation.")
            or f.startswith("repeat_until_success.")
            or f.startswith("python-deps.")
        )
    ]
    python_bin = use_python_env(samples_src)

    # copy the process env vars
    pip_env: dict[str, str] = os.environ.copy()

    # Install the qsharp package
    pip_install_args = [
        python_bin,
        "-m",
        "pip",
        "install",
        "--force-reinstall",
        "--no-index",
        "--find-links=" + wheels_dir,
        f"qsharp",
    ]
    subprocess.run(pip_install_args, check=True, text=True, cwd=pip_src)

    # Install the widgets package from the folder so dependencies are installed properly
    pip_install_args = [
        python_bin,
        "-m",
        "pip",
        "install",
        widgets_src,
    ]
    subprocess.run(
        pip_install_args, check=True, text=True, cwd=widgets_src, env=pip_env
    )

    # Install other dependencies
    pip_install_args = [
        python_bin,
        "-m",
        "pip",
        "install",
        "ipykernel",
        "nbconvert",
        "pandas",
        "qutip",
        "qiskit>=1.3.0,<2.0.0",
    ]
    subprocess.run(pip_install_args, check=True, text=True, cwd=root_dir, env=pip_env)

    for notebook in notebook_files:
        print(f"Running {notebook}")
        # Run the notebook process, capturing stdout and only displaying it if there is an error
        result = subprocess.run(
            [
                python_bin,
                "-m",
                "nbconvert",
                "--to",
                "notebook",
                "--stdout",
                "--ExecutePreprocessor.timeout=60",
                "--sanitize-html",
                "--execute",
                notebook,
            ],
            check=False,
            text=True,
            cwd=root_dir,
            env=pip_env,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            encoding="utf-8",
        )
        if result.returncode != 0:
            print(result.stdout)
            raise Exception(f"Error running {notebook}")

    step_end()

    step_start("Running qsharp testing samples")
    project_directories = [
        dir for dir in os.walk(samples_src) if "qsharp.json" in dir[2]
    ]

    test_projects_directories = [
        dir for dir, _, _ in project_directories if dir.find("testing") != -1
    ]

    install_python_test_requirements(pip_src, python_bin)
    for test_project_dir in test_projects_directories:
        run_python_tests(test_project_dir, python_bin)
    step_end()

if ci_bench:
    step_start("Running CI benchmarking script")
    run_ci_historic_benchmark()
    step_end()
