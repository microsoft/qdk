# OpenQASM Support

The QDK supports a useful subset of OpenQASM 3 for simulation, debugging,
circuit generation, resource estimation, and Azure Quantum submission. Programs
using OpenQASM hardware-control features can still be edited in VS Code even
when the QDK cannot compile them.

The QDK does not compile these construct families:

* Calibration blocks and `defcal` definitions
* Timing and duration operations such as `delay`
* Hardware qubit addressing
* `extern` declarations
* Mutable array references

Use the `qdk.openqasm.mode` setting to choose how the editor treats a file:

* `auto` is the default. It uses QDK mode until the file contains a construct
  the QDK cannot compile, then uses spec mode.
* `qdk` reports unsupported constructs as errors and enables QDK features such
  as Run, Debug, circuit generation, resource estimation, and submission.
* `spec` reports OpenQASM syntax and semantic errors while disabling QDK-only
  features. A code lens and Command Palette commands switch the file back to
  QDK mode when those features are needed.

The sample files in this directory are QDK-compatible examples. The editor's
spec mode is intended for OpenQASM programs that use the standard beyond the
subset the QDK currently compiles.
