---
name: setup-qdk-python
description: 'Set up Python QDK (Quantum Development Kit) environment. Use when: user asks to "install qdk", "set up python for quantum", "configure qdk packages", "install qsharp", troubleshoot missing QDK Python packages, or needs guidance on qdk extras like qdk[qiskit], qdk[azure], qdk[mcp]. Checks current environment and provides tailored installation steps.'
---

# Set Up Python QDK Environment

Install and configure the QDK (Quantum Development Kit) Python packages for quantum computing with Q# and OpenQASM.

## When to Use

- User wants to run Q# or OpenQASM from Python
- User needs Qiskit, Cirq, or Azure Quantum interop
- User asks about missing `qdk`, `qsharp`, or `qdk-mcp` packages
- User wants to set up Jupyter notebooks for quantum development
- Tool call to `qdk-init-python-environment` reports missing packages

## Step 1: Check Current Environment

Call the `qdk-init-python-environment` tool to detect the user's current setup. This returns:

- Whether the Python Environments VS Code extension is installed
- The active Python environment and version
- Which QDK packages are already installed
- Specific install commands tailored to the user's environment

Use the tool output to skip steps that are already satisfied.

## Step 2: Ensure Python is Available

- **Required**: Python >= 3.10
- If Python is not found, direct the user to https://www.python.org/downloads/
- Recommend installing the **Python VS Code extension** (`ms-python.python`) for the best experience

## Step 3: Create or Select a Virtual Environment

Always recommend using a virtual environment.

**In VS Code (preferred):** If the user has the **Python** extension pack (`ms-python.python`) installed, they should create and select environments through the VS Code UI or command palette. This ensures VS Code actually uses the correct environment for IntelliSense, debugging, terminal activation, and tool integration. Simply activating a venv in a standalone terminal does not make VS Code aware of it.

Recommend installing `ms-python.python` if it's not already installed — it provides the Python environment management UI, Jupyter support, and other essentials.

**From the command line** (if not using VS Code, or for CI/scripts):

```bash
python -m venv .venv
```

Activate it:
- **Windows (PowerShell)**: `.venv\Scripts\Activate.ps1`
- **Windows (cmd)**: `.venv\Scripts\activate.bat`
- **macOS/Linux**: `source .venv/bin/activate`

## Step 4: Install the QDK Metapackage

The `qdk` package is a metapackage that bundles `qsharp` (the core Q# compiler and simulator as a native Rust module) along with optional extras for specific workflows.

### Base Install

```bash
pip install qdk
```

This provides: Q# and OpenQASM compilation, local quantum simulation, and resource estimation.

### Optional Extras

Install extras using bracket syntax. Multiple extras can be combined with commas:

```bash
pip install "qdk[jupyter,qiskit,azure]"
```

| Extra | Command | What It Adds |
|-------|---------|-------------|
| `jupyter` | `pip install "qdk[jupyter]"` | Jupyter widgets, syntax highlighting, `%%qsharp` magic |
| `azure` | `pip install "qdk[azure]"` | Azure Quantum workspace connectivity and job submission |
| `qiskit` | `pip install "qdk[qiskit]"` | Qiskit interop — run Qiskit circuits on Azure Quantum |
| `cirq` | `pip install "qdk[cirq]"` | Cirq interop — run Cirq circuits on Azure Quantum |
| `mcp` | `pip install "qdk[mcp]"` | QDK MCP server for LLM tool integration |
| `all` | `pip install "qdk[all]"` | All of the above |

### Choosing Extras

Ask the user what they need:

- **"I want to use Jupyter notebooks"** → `qdk[jupyter]`
- **"I want to submit jobs to Azure Quantum"** → `qdk[azure]`
- **"I use Qiskit and want Azure Quantum"** → `qdk[qiskit,azure]`
- **"I want everything"** → `qdk[all]`
- **"I just want to simulate Q# locally"** → `qdk` (no extras)
- **"I want to use QDK tools from an AI assistant"** → `qdk[mcp]`

## Step 5: Version Alignment

These QDK packages are versioned together and must be kept in sync:

- `qdk` (metapackage)
- `qsharp` (core compiler/simulator)
- `qsharp-widgets`
- `qsharp-jupyterlab`
- `qdk-mcp`

These all share the same version number (e.g., `1.26.1234`). **Never mix versions** across these packages — if `qsharp` is at `1.26.x` but `qdk-mcp` is at `1.25.x`, things may break.

Third-party and external dependencies have their own versioning:
- `azure-quantum` — separate Microsoft product, versioned independently
- `qiskit`, `cirq-core` — third-party frameworks with their own release cycles
- `pyqir` — versioned independently

The `qdk` metapackage pins compatible ranges for these external dependencies, so installing via `qdk` ensures compatibility.

When upgrading:

```bash
pip install --upgrade qdk
```

Or with extras:

```bash
pip install --upgrade "qdk[jupyter,azure]"
```

Always upgrade via the `qdk` metapackage to keep the QDK-internal versions aligned.

## Step 6: Verify Installation

```python
import qdk
print(qdk.qsharp.__version__)
```

Or run a quick Q# program:

```python
import qdk
result = qdk.qsharp.eval("Message(\"Hello, quantum world!\")")
print(result)
```

## Package Architecture Reference

```
qdk (metapackage)
├── qsharp          — Core: compiler, simulator, resource estimator (Rust native module)
├── qsharp-widgets  — [jupyter] Jupyter widget rendering
├── qsharp-jupyterlab — [jupyter] JupyterLab extension
├── azure-quantum   — [azure] Azure Quantum service client
├── qiskit          — [qiskit] Qiskit framework
├── cirq-core       — [cirq] Cirq framework
├── cirq-ionq       — [cirq] Cirq IonQ provider
├── qdk-mcp         — [mcp] MCP server (FastMCP-based, tools: eval, circuit)
└── pyqir           — QIR code generation (always included)
```

## Troubleshooting

| Symptom | Cause | Fix |
|---------|-------|-----|
| `ModuleNotFoundError: No module named 'qsharp'` | Package not installed | `pip install qdk` |
| `ModuleNotFoundError: No module named 'qdk.azure'` | Missing extra | `pip install "qdk[azure]"` |
| Import errors after upgrade | Version mismatch | `pip install --upgrade qdk` (reinstalls all) |
| `pip install qdk` fails on build | Missing Rust toolchain (rare, only building from source) | Install from PyPI wheels: `pip install --only-binary=:all: qdk` |
| Wrong Python version | Python < 3.10 | Install Python 3.10+ |
