# Microsoft Quantum Development Kit (QDK)

This extension brings rich language support for Q# and OpenQASM to VS Code. Develop, build, and run your quantum code from VS Code either locally on simulators, or by submitting a job to Azure Quantum.

You can also try out this extension in VS Code for Web at [vscode.dev/quantum](https://vscode.dev/quantum).

## Features

The QDK extension currently supports:

- Syntax highlighting and syntax features (e.g. brace matching) for Q# and OpenQASM
- Editing features such as go-to-definition, suggestions and signature help for Q# and OpenQASM
- Error checking in Q# and OpenQASM source files
- Local quantum simulation, including support for Pauli noise and qubit loss
- Breakpoint debugging for Q# and OpenQASM
- Code samples for Q# and OpenQASM demonstrating well known algorithms
- Circuit visualization
- Q# cell support in Jupyter notebooks. The extension will detect `%%qsharp` magic cells and automatically update the cell language to Q#
- Integration with Azure Quantum for job submission to quantum hardware providers

For more information about the QDK and Microsoft Quantum, visit [https://aka.ms/qdk](https://aka.ms/qdk).

## Contributing

To log issues, contribute to the project, or build the extension yourself, visit the repository at <https://github.com/microsoft/qsharp>

## Data and telemetry

This extension collects usage data and sends it to Microsoft to help improve our products and services.
Details of the telemetry sent can be seen in the source file at <https://github.com/microsoft/qsharp/blob/main/source/vscode/src/telemetry.ts>.
This extension respects the `telemetry.enableTelemetry` setting which you can learn more about at
<https://code.visualstudio.com/docs/supporting/faq#_how-to-disable-telemetry-reporting>.
