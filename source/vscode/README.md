# Microsoft Quantum Development Kit (QDK)

This extension brings rich language support for Q# and OpenQASM to VS Code. Develop, build, and run your quantum code from VS Code either locally on simulators, or by submitting a job to Azure Quantum.

You can also try out this extension in VS Code for Web at [vscode.dev/quantum/playground](https://vscode.dev/quantum/playground/).

## Features

The QDK extension currently supports:

- Syntax highlighting and syntax features (e.g. brace matching) for Q#, OpenQASM, and DEQ
- Editing features such as go-to-definition, suggestions and signature help for Q# and OpenQASM
- Error checking in Q# and OpenQASM source files
- Local quantum simulation, including support for Pauli noise and qubit loss
- Breakpoint debugging for Q# and OpenQASM
- Code samples for Q# and OpenQASM demonstrating well known algorithms
- Circuit diagram visualization and editing
- Q# cell support in Jupyter notebooks. The extension will detect `%%qsharp` magic cells and automatically update the cell language to Q#
- Integration with Azure Quantum for job submission to quantum hardware providers

## Selecting a local simulator

The **Q# › Simulation: Type** setting controls the simulator used by local runs, histogram generation, Test Explorer, and debugging:

- `sparse` (default) uses the sparse state-vector simulator and supports arbitrary quantum programs.
- `clifford` uses the stabilizer simulator for Clifford-only programs. Set **Q# › Simulation › Clifford: Max Qubits** to the maximum number of simultaneously allocated qubits required by the program.

The Clifford simulator reports an error for non-Clifford gates and rotations. `DumpMachine` reports a warning and continues without amplitude output, and the debugger's amplitude-based **Quantum State** scope is not available with Clifford simulation; use the debugger's locals and circuit scopes instead. The Pauli noise setting is supported for Clifford histogram generation. Qubit loss is currently supported only by the sparse simulator.

For more information about the QDK and Microsoft Quantum, visit [https://aka.ms/qdk](https://aka.ms/qdk).

## Contributing

To log issues, contribute to the project, or build the extension yourself, visit the repository at <https://github.com/microsoft/qdk>

## Data and telemetry

This extension collects usage data and sends it to Microsoft to help improve our products and services.
Details of the telemetry sent can be seen in the source file at <https://github.com/microsoft/qdk/blob/main/source/vscode/src/telemetry.ts>.
This extension respects the `telemetry.enableTelemetry` setting which you can learn more about at
<https://code.visualstudio.com/docs/supporting/faq#_how-to-disable-telemetry-reporting>.
