# Azure Quantum Development Kit (QDK)

This extension brings rich Q# language support to VS Code. Develop, build, and run your Q# code from VS Code either locally on simulators, or by submitting a job to Azure Quantum.

You can also try out this extension in VS Code for Web at [vscode.dev/quantum](https://vscode.dev/quantum).

## Features

The Q# extension currently supports:

- Syntax highlighting and basic syntax features (e.g. brace matching)
- Q# cell support in Jupyter notebooks. The extension will detect `%%qsharp` magic cells and automatically update the cell language to Q#
- Error checking in Q# source files
- Breakpoint debugging and script execution for Q# source files
- Integration with Azure Quantum for job submission to quantum hardware providers
- Hover-definition and docs
- Go-to-definition
- Function signature help
- Snippet and sample support
- Completions

For more information about the QDK and Azure Quantum, visit [https://aka.ms/AQ/Documentation](https://aka.ms/AQ/Documentation).

## Contributing

To log issues, contribute to the project, or build the extension yourself, visit the repository at <https://github.com/microsoft/qsharp>

## Data and telemetry

This extension collects usage data and sends it to Microsoft to help improve our products and services.
Details of the telemetry sent can be seen in the source file at <https://github.com/microsoft/qsharp/blob/main/vscode/src/telemetry.ts>.
This extension respects the `telemetry.enableTelemetry` setting which you can learn more about at
<https://code.visualstudio.com/docs/supporting/faq#_how-to-disable-telemetry-reporting>.
