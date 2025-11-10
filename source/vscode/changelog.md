# QDK Changelog

## v1.22.0

Below are some of the highlights for the 1.22 release of the QDK.

### Python `qdk` package is out of preview

With this release, the `qdk` package on PyPI is now considered stable and out of preview, and is the recommended way to install the QDK for Python users. The package includes a number of 'extras' to add optional functionality, such as Jupyter Notebook support, Azure Quantum integration, and Qiskit interop. For example, to install the QDK with Qiskit, Jupyter and Azure Quantum support:

    pip install "qdk[qiskit,jupyter,azure]"

As a shortcut to install all optional functionality, you can also do:

    pip install "qdk[all]"

See <https://pypi.org/project/qdk/> for more details.

### Qiskit 2 support

With this release, the QDK supports both Qiskit 1.x and 2.x releases for converting a Qiskit circuit into QIR and submitting as a job to the Azure Quantum service.

> Note that this **does not** yet support using Azure Quantum `Backends` directly from Qiskit 2.x; that functionality is planned for a future release of the [azure-quantum](https://pypi.org/project/azure-quantum) Python package.

For an example of submitting a Qiskit circuit by first converting to QIR, see the first sample notebook in the next section.

### Sample notebooks for submitting Qiskit, Cirq, and PennyLane programs to Azure Quantum

We have added sample Jupyter Notebooks demonstrating how to submit quantum programs written in Qiskit, Cirq, and PennyLane to the Azure Quantum service. These samples use the `qdk` Python package to convert the circuits into QIR format, and then submit them as jobs to Azure Quantum.

- [Submit Qiskit Circuit to Azure](https://github.com/microsoft/qdk/blob/main/samples/python_interop/submit_qiskit_circuit_to_azure.ipynb)
- [Circ submission to Azure](https://github.com/microsoft/qdk/blob/main/samples/python_interop/cirq_submission_to_azure.ipynb)
- [PennyLane submission to Azure](https://github.com/microsoft/qdk/blob/main/samples/python_interop/pennylane_submission_to_azure.ipynb)

### Spec compliant QIR code generation

In this release we have updated the QIR code generation to be compliant with the [QIR specification](https://github.com/qir-alliance/qir-spec/tree/main/specification). This has been tested with the quantum targets available on Azure Quantum, and you should see no difference in behavior when submitting jobs. However if you are using the generated QIR in other toolchain, you may be impacted. See the PR at [#2590](https://github.com/microsoft/qdk/pull/2590) for details.

### Code action to create parameterless wrappers

A new Code Action has been added to wrap an existing operation in a new operation that takes no parameters. The new operation can be edited to prepare the parameters before calling the existing operation. This allows for easy circuit generation, execution, debugging, etc. via the CodeLens actions on the new operation, as well as quickly turning the wrapper into a unit test.

![wrapper](https://github.com/user-attachments/assets/c35bc7e5-bea3-4a9a-bcf1-9e2e0bd8cdd7)

### Azure Quantum job cancellation

Jobs submitted to the Azure Quantum service that have not yet completed can now be cancelled directly from the VS Code "Quantum Workspaces" explorer view. As shown below, when a job is in the `Waiting` or `Running` state, a "Cancel Azure Quantum Job" icon is available to the right of the job name. Clicking this icon will prompt for confirmation, and then submit a cancellation request to Azure Quantum.

<img width="575" alt="cancel job" src="https://github.com/user-attachments/assets/9baca94b-38fc-4bd6-b312-1ba6117335ba" />

## Other notable changes

- Emit spec compliant QIR by @swernli in [#2590](https://github.com/microsoft/qdk/pull/2590)
- Improved adjoint Select implementation by @DmitryVasilevsky in [#2729](https://github.com/microsoft/qdk/pull/2729)
- Code Action for Parameterless Wrappers by @ScottCarda-MS in [#2731](https://github.com/microsoft/qdk/pull/2731)
- Housekeeping: Tidy up spelling by @ConradJohnston in [#2734](https://github.com/microsoft/qdk/pull/2734)
- Fix a bug in trivial 1-to-1 distillation unit by @msoeken in [#2736](https://github.com/microsoft/qdk/pull/2736)
- Enable implementation of `prune_error_budget` in custom estimation API by @msoeken in [#2737](https://github.com/microsoft/qdk/pull/2737)
- Better `compile` error when missing call to `init` by @swernli in [#2735](https://github.com/microsoft/qdk/pull/2735)
- Sample Notebook for Submitting Qiskit to Azure Quantum using `qdk` python by @ScottCarda-MS in [#2739](https://github.com/microsoft/qdk/pull/2739)
- Replace Quantinuum H1 with H2 in samples by @swernli in [#2747](https://github.com/microsoft/qdk/pull/2747)
- Fix Webview and Circuit Editor Left Padding by @ScottCarda-MS in [#2748](https://github.com/microsoft/qdk/pull/2748)
- Array error messages for comma issues by @joesho112358 in [#2744](https://github.com/microsoft/qdk/pull/2744)
- Fix OpenQASM `cu` target by @swernli in [#2752](https://github.com/microsoft/qdk/pull/2752)
- Remove `dump_circuit` from top-level `qdk` python module by @ScottCarda-MS in [#2753](https://github.com/microsoft/qdk/pull/2753)
- Cirq Sample Notebook for Azure Submission by @ScottCarda-MS in [#2751](https://github.com/microsoft/qdk/pull/2751)
- Removed References to the QDK Package being "preview" by @ScottCarda-MS in [#2756](https://github.com/microsoft/qdk/pull/2756)
- Circuit diagram snapshot tests (includes Node.js upgrade) by @minestarks in [#2743](https://github.com/microsoft/qdk/pull/2743)
- Add lint warning for ambiguous if-statement followed by unary operator by @swernli in [#2759](https://github.com/microsoft/qdk/pull/2759)
- Automatic estimation of overhead in memory/compute architecture by @msoeken in [#2760](https://github.com/microsoft/qdk/pull/2760)
- Enable Qiskit 2.0 support by @idavis in [#2754](https://github.com/microsoft/qdk/pull/2754)
- Job cancallation by @billti in [#2763](https://github.com/microsoft/qdk/pull/2763)
- PennyLane Sample Notebook for Azure Submission by @ScottCarda-MS in [#2758](https://github.com/microsoft/qdk/pull/2758)
- Unit test for over-large address in Select/Unselect by @DmitryVasilevsky in [#2765](https://github.com/microsoft/qdk/pull/2765)

## New Contributors

- @ConradJohnston made their first contribution in https://github.com/microsoft/qdk/pull/2734
- @joesho112358 made their first contribution in https://github.com/microsoft/qdk/pull/2744

**Full Changelog**: https://github.com/microsoft/qdk/compare/v1.21.0...v1.22.0

## v1.21.0

Below are some of the highlights for the 1.21 release of the QDK.

### QDK Python package

With this release we are also publishing a `qdk` package to PyPI (see <https://pypi.org/project/qdk/>). This is still in the 'preview' stage as we lock down the API, but the goal is that going forward the QDK will be installed in Python via `pip install qdk`, with any optional extras needed (e.g. `pip install "qdk[jupyter,azure,qiskit]"` to add the Jupyter Notebooks, Azure Quantum, and Qiskit integration). Once installed, import from the necessary submodules (e.g. `from qdk.openqasm import compile`)

Please give it a try and open an issue if you have any feedback.

### Complex literals

The Q\# language added support for complex literals. For example,

```qsharp
function GetComplex() : Complex {
    3. + 4.i
}
```

Additionally, Complex values can now be used in arithmetic expressions directly:

```qsharp
let x = 2.0 + 3.0i;
let y = x + (4.0 - 5.0i);
```

## Other notable changes

- Update to latest simulator, new benchmark by @swernli in https://github.com/microsoft/qdk/pull/2690
- Updated wording in 'complex numbers' kata by @DmitryVasilevsky in https://github.com/microsoft/qdk/pull/2694
- Fix decomposition for controlled Rxx/Ryy by @swernli in https://github.com/microsoft/qdk/pull/2699
- Fix panic in RCA when using tuple variables as arguments to a lambda by @swernli in https://github.com/microsoft/qdk/pull/2701
- Support Complex literals, arithmetic operations by @swernli in https://github.com/microsoft/qdk/pull/2709
- Fix panic when interpreter has unbound names in Adaptive/Base by @swernli in https://github.com/microsoft/qdk/pull/2691
- [OpenQASM]: Properly detect zero step in const ranges by @orpuente-MS in https://github.com/microsoft/qdk/pull/2715
- Short-circuiting expressions produce divergent types that propagate too far by @swernli in https://github.com/microsoft/qdk/pull/2700
- Initial QDK Python Package by @ScottCarda-MS in https://github.com/microsoft/qdk/pull/2707
- Extract logical resource counts from a Q# program by @msoeken in https://github.com/microsoft/qdk/pull/2717
- Fix panic in loop unification pass for short-circuiting expressions by @swernli in https://github.com/microsoft/qdk/pull/2723
- Support partial evaluation of `IndexRange` calls by @swernli in https://github.com/microsoft/qdk/pull/2727

**Full Changelog**: <https://github.com/microsoft/qdk/compare/v1.20.0...v1.21.0>

## v1.20.0

Below are some of the highlights for the 1.20 release of the QDK.

### QIR target profile selection redesign

In previous releases, the target QIR profile setting for code generation in VS Code was a global setting, and if switching between projects with different target profiles the user would need to remember to change the editor setting each time. This was cumbersome and a common source of confusion.

With this release, the target profile can be set per project. If working on a multi-file project with a `qsharp.json` manifest file, the target profile can be specified in the manifest file via the `"targetProfile"` property.

If working in a standalone Q# or OpenQASM file, the target profile can be specified via the `@EntryPoint` attribute in Q# files, or the `qdk.qir.profile` pragma in OpenQASM files. For example, to target a Q# file for `base profile` code generation:

```qsharp
@EntryPoint(Base)
operation Main() : Result[] {
    // ...
}
```

If submitting a job to the Azure Quantum service, upon submission the target profile will default to the capabilities of the target hardware if not otherwise specified.

See the [QDK Profile Selection](https://github.com/microsoft/qsharp/wiki/QDK-Profile-Selection) wiki page for more details and examples.

### OpenQASM improvements

- Arrays and complex numbers can now be passed as input and output.
- Qubit aliases and array concatenation are now supported.
- In VS Code, OpenQASM files now support:
  - Rename symbol (F2) for user-defined identifiers, gates, functions, etc; built-ins are not renameable.
  - Go to Definition (F12) and Find All References (Shift+F12).
- The GitHub Copilot tools for the QDK now support OpenQASM (.qasm) files as well as Q#. You can simulate your program, generate a circuit diagram and generate resource estimates for Q# and OpenQASM programs right from the chat panel.

### Python interop improvements

In addition to primitive types, arrays, and tuples, users can now pass [Q# structs](https://github.com/microsoft/qsharp/wiki/Q%23-Structs). For example, the following shows passing a python object to a Q# operation that takes struct:

<img width="899" src="https://github.com/user-attachments/assets/b711a19e-3814-44bc-9df7-cefca2830609" />

Complex numbers are also supported, allowing the passing of Python complex literals and variables directly to Q# callables, e.g.

```python
import qsharp

qsharp.eval("""
    import Std.Math.Complex;

    function SwapComponents(c: Complex) : Complex {
        new Complex { Real = c.Imag, Imag = c.Real }
    }
    """)

from qsharp.code import SwapComponents
assert SwapComponents(2 + 3j) == 3 + 2j
```

For more details on Python and Q# interop see our [wiki page](https://github.com/microsoft/qsharp/wiki/Invoking-Q%23-callables-from-Python).

### Richer Copilot help for Q# APIs

In this release, we added a new Copilot tool that can access the documentation generated by all the APIs available in the current project (included the standard library and referenced libraries). This lets Copilot see up-to-date information on the Q# libraries available and how to use them, improving Copilot's working knowledge of Q#. From generated Q# snippets to answering questions about available APIs, Copilot can use this to provide more accurate and up-to-date information.

<img width="723" src="https://github.com/user-attachments/assets/005cf3f5-7364-4ef4-ac00-b558d2d953c8" />

### Language service improvements

Numerous improvements to the language service have been made for correctness and completeness, especially around import and export declarations. The language service now also provides better diagnostics for QIR generation errors. Please log issues if you encounter any problems!

## Other notable changes

- Profile Selection Redesign by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2636](https://github.com/microsoft/qsharp/pull/2636)
- Import and export resolution improvements by [@minestarks](https://github.com/minestarks) in [#2638](https://github.com/microsoft/qsharp/pull/2638)
- Fix parsing of OPENQASM version statement by [@orpuente-MS](https://github.com/orpuente-MS) in [#2650](https://github.com/microsoft/qsharp/pull/2650)
- Fix completions for import and exports by [@minestarks](https://github.com/minestarks) in [#2640](https://github.com/microsoft/qsharp/pull/2640)
- Python - Q# UDT interop by [@orpuente-MS](https://github.com/orpuente-MS) in [#2635](https://github.com/microsoft/qsharp/pull/2635)
- Fix off-by-one error in stack traces by making line and column numbers 1-based for user display by [@Copilot](https://github.com/Copilot) in [#2628](https://github.com/microsoft/qsharp/pull/2628)
- Allow recursive calls to operations that return `Unit` by [@swernli](https://github.com/swernli) in [#2654](https://github.com/microsoft/qsharp/pull/2654)
- Fix panic when running .qsc circuit file by [@orpuente-MS](https://github.com/orpuente-MS) in [#2666](https://github.com/microsoft/qsharp/pull/2666)
- OpenQASM language service: add rename/definition/references, semantic passes, and VS Code wiring by [@idavis](https://github.com/idavis) in [#2656](https://github.com/microsoft/qsharp/pull/2656)
- Add support for qubit alias decls by [@idavis](https://github.com/idavis) in [#2665](https://github.com/microsoft/qsharp/pull/2665)
- Remove Azure Quantum Credits deprecation message from VS Code extension by [@Copilot](https://github.com/Copilot) in [#2668](https://github.com/microsoft/qsharp/pull/2668)
- Support numpy arrays in the Python interop layer by [@orpuente-MS](https://github.com/orpuente-MS) in [#2671](https://github.com/microsoft/qsharp/pull/2671)
- Definition, References, Hover, Rename for import/export decls by [@minestarks](https://github.com/minestarks) in [#2641](https://github.com/microsoft/qsharp/pull/2641)
- Nicer error messages for QIR generation by [@minestarks](https://github.com/minestarks) in [#2664](https://github.com/microsoft/qsharp/pull/2664)
- Improve interface and output for Submit Job Copilot tool by [@minestarks](https://github.com/minestarks) in [#2673](https://github.com/microsoft/qsharp/pull/2673)
- Lower duration and stretch by [@idavis](https://github.com/idavis) in [#2611](https://github.com/microsoft/qsharp/pull/2611)
- OpenQASM support for Copilot tools by [@minestarks](https://github.com/minestarks) in [#2675](https://github.com/microsoft/qsharp/pull/2675)
- Add support for array concatenation in OpenQASM by [@orpuente-MS](https://github.com/orpuente-MS) in [#2676](https://github.com/microsoft/qsharp/pull/2676)
- Copilot Reads Summaries from DocGen Tool by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2672](https://github.com/microsoft/qsharp/pull/2672)
- OpenQASM input and output interop by [@idavis](https://github.com/idavis) in [#2678](https://github.com/microsoft/qsharp/pull/2678)
- Better symbol resolution for OpenQASM LS support by [@idavis](https://github.com/idavis) in [#2679](https://github.com/microsoft/qsharp/pull/2679)

**Full Changelog**: [v1.19.0...v1.20.0](https://github.com/microsoft/qsharp/compare/v1.19.0...v1.20.0)

## v1.19.0

Below are some of the highlights for the 1.19 release of the QDK.

### Simulating qubit loss

Simulation in the QDK can now model qubit loss, which can occur with some probability on some modalities.

For simulations run directly in VS Code, such as using the 'Histogram' CodeLens, this can be controlled via a VS Code setting. The screenshot below shows setting qubit loss to 0.5% and running a Bell pair simulation. For convenience, the VS Code setting is easily accessible from a link on the histogram window (shown in a red circle below).

<img width="1000" alt="image" src="https://github.com/user-attachments/assets/298250b8-bdda-4529-aa40-804b787153b0" />

The qubit loss probability can also be specified if running a simulation via the Python API.

```python
result = qsharp.run("BellPair()", 100, qubit_loss=0.5)
display(qsharp_widgets.Histogram(result))
```

There is also a new Q# API for detecting a loss result:

```qsharp
operation CheckForLoss() : Unit {
    use q = Qubit();
    H(q);
    let res = MResetZ(q);
    if IsLossResult(res) {
        // Handle qubit loss here
    } else {
        // Handle Zero or One result
    }
}
```

You can find more details in the sample Jupyter Notebook at <https://github.com/microsoft/qsharp/blob/main/samples/notebooks/noise.ipynb>.

### Debugger improvements

When debugging, previously if you navigated up the call stack using the area circled below, the `Locals` view would not change context to reflect the state of the variables in the selected stack frame. This has now been implemented.

<img width="1000" alt="image" src="https://github.com/user-attachments/assets/971ef5f7-d4f3-49dd-b002-923a63fca65b" />

Call stacks reported when a runtime error occurs now also show the source location for each frame in the call stack.

### OpenQASM improvements

We have continued to improve support for OpenQASM. For example, you can now use `readonly` arrays as arguments to subroutines and the builtin `sizeof` function, which allows you to query the size of arrays.

```qasm
def static_array_example(readonly array[int, 3, 4] a) {
    // The returned value for static arrays is const.
    const uint dim_1 = sizeof(a, 0);
    const uint dim_2 = sizeof(a, 1);
}

def dyn_array_example(readonly array[int, #dim = 2] a) {
    // The 2nd argument is inferred to be 0 if missing.
    uint dim_1 = sizeof(a);
    uint dim_2 = sizeof(a, 1);
}
```

This release also adds many other built-in functions, as well as pragmas to specify the semantics and code generation for `box` statements.

For more examples of the OpenQASM support see the samples at <https://github.com/microsoft/qsharp/tree/main/samples/OpenQASM> or the Jupyter Notebook at <https://github.com/microsoft/qsharp/blob/main/samples/notebooks/openqasm.ipynb>.

### Test improvements

A challenge when writing tests for code intended to run on hardware was that the test code would also be restricted to what could run on hardware. For example, if the target profile is set to `base` then mid-circuit measurements and result comparisons are not possible, which limits the validation a test can do. Trying to verify a measurement result in a test would previously result in errors such as `using a bool value that depends on a measurement result is not supported by the configured target profile`.

In this release we have relaxed the checks performed on code marked with the `@Test` attribute, so such code is valid regardless of the target hardware profile:

<img width="977" alt="image" src="https://github.com/user-attachments/assets/dd81c8e4-e957-4e99-9e0e-1ec6f10b5e73" />

### Azure Quantum job reporting

When submitting jobs to Azure using the VS Code "Quantum Workspaces" explorer view, previously jobs would use the v1 reporting format, which does not include details for each shot's results. The default format for job submission in this release is now v2, which includes the results of each shot.

We also added an additional icon beside successfully completed jobs so the results may be shown as a histogram or as the raw text. The below screenshot shows fetching both formats from a completed job.

<img width="1000" alt="image" src="https://github.com/user-attachments/assets/c19bf68e-401c-40b9-bd50-d765217d2323" />

### Other notable changes

- Show local variables of selected frame when debugging. by [@orpuente-MS](https://github.com/orpuente-MS) in [#2572](https://github.com/microsoft/qsharp/pull/2572)
- When an error occurs show line/col info for each frame in the stack by [@orpuente-MS](https://github.com/orpuente-MS) in [#2573](https://github.com/microsoft/qsharp/pull/2573)
- Upgrade rust edition to 2024 by [@orpuente-MS](https://github.com/orpuente-MS) in [#2577](https://github.com/microsoft/qsharp/pull/2577)
- Update to mimalloc v2.2.4 by [@idavis](https://github.com/idavis) in [#2579](https://github.com/microsoft/qsharp/pull/2579)
- Support for qubit loss by [@swernli](https://github.com/swernli) in [#2567](https://github.com/microsoft/qsharp/pull/2567)
- Allow unrestricted capabilities in `@Test` callables by [@swernli](https://github.com/swernli) in [#2584](https://github.com/microsoft/qsharp/pull/2584)
- Upgrade the rust version to 1.88 by [@orpuente-MS](https://github.com/orpuente-MS) in [#2583](https://github.com/microsoft/qsharp/pull/2583)
- Add sizeof bultin to qasm by [@orpuente-MS](https://github.com/orpuente-MS) in [#2586](https://github.com/microsoft/qsharp/pull/2586)
- Adding box pragma support to QASM compiler by [@idavis](https://github.com/idavis) in [#2571](https://github.com/microsoft/qsharp/pull/2571)
- Making MapPauliAxis public with updates to functionality and comments by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2585](https://github.com/microsoft/qsharp/pull/2585)
- Improve runtime error debugging by [@swernli](https://github.com/swernli) in [#2592](https://github.com/microsoft/qsharp/pull/2592)
- Show downloaded results as histograms by [@billti](https://github.com/billti) in [#2595](https://github.com/microsoft/qsharp/pull/2595)
- Changelog View Added by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2576](https://github.com/microsoft/qsharp/pull/2576)
- Switch the default reporting format to v2 by [@billti](https://github.com/billti) in [#2605](https://github.com/microsoft/qsharp/pull/2605)
- Allow indexing into array references in OpenQASM by [@orpuente-MS](https://github.com/orpuente-MS) in [#2616](https://github.com/microsoft/qsharp/pull/2616)
- Fix source offset when printing call stack by [@orpuente-MS](https://github.com/orpuente-MS) in [#2629](https://github.com/microsoft/qsharp/pull/2629)
- Angle from floats handle rounding when operating on values less than epsilon by [@idavis](https://github.com/idavis) in [#2630](https://github.com/microsoft/qsharp/pull/2630)

**Full Changelog**: [v1.18.0...v1.19.0](https://github.com/microsoft/qsharp/compare/v1.18.0...v1.19.0)

## v1.18.0

### What's Changed

- Adding support for QASM 2.0 in the OpenQASM compiler by [@idavis](https://github.com/idavis) in [#2527](https://github.com/microsoft/qsharp/pull/2527)
- Circuit Editor Run Button by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2517](https://github.com/microsoft/qsharp/pull/2517)
- Added ApplyQPE and ApplyOperationPowerCA to Canon namespace by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2473](https://github.com/microsoft/qsharp/pull/2473)
- Fix bug in `Controlled SX` with empty controls by [@swernli](https://github.com/swernli) in [#2507](https://github.com/microsoft/qsharp/pull/2507)
- Support Running Projects from Circuit Files by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2455](https://github.com/microsoft/qsharp/pull/2455)
- Quantum Phase estimation sample via ApplyQPE by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2506](https://github.com/microsoft/qsharp/pull/2506)
- Don't show code lenses for code with compilation errors by [@copilot-swe-agent](https://github.com/copilot-swe-agent) in [#2511](https://github.com/microsoft/qsharp/pull/2511)
- Fix language service to use Unrestricted target profile as default for notebooks by [@copilot-swe-agent](https://github.com/copilot-swe-agent) in [#2528](https://github.com/microsoft/qsharp/pull/2528)
- Generic resource estimation using Python models by [@msoeken](https://github.com/msoeken) in [#2555](https://github.com/microsoft/qsharp/pull/2555)

**Full Changelog**: [v1.17.0...v1.18.0](https://github.com/microsoft/qsharp/compare/v1.17.0...v1.18.0)

## v1.17.0

### OpenQASM support

We've added extensive support for the [OpenQASM](https://openqasm.com/) language. This provides editor support (syntax highlighting, intellisense, semantic errors), simulation, integration with Q#, and QIR code generation, amongst other features.

![image](https://github.com/user-attachments/assets/d6d78f6e-9dd1-4724-882b-a889d4ace4c8)

See the wiki page at <https://github.com/microsoft/qsharp/wiki/OpenQASM> for more details.

### Copilot improvements

We've improved the GitHub Copilot integration with this release. See the details at <https://github.com/microsoft/qsharp/wiki/Make-the-most-of-the-QDK-and-VS-Code-agent-mode>

### Circuit editor improvements

We have further improved the ability to edit circuit diagrams. See the detail at <https://github.com/microsoft/qsharp/wiki/Circuit-Editor>

### Other notable changes

- Support intrinsic `SX` gate by [@swernli](https://github.com/swernli) in [#2338](https://github.com/microsoft/qsharp/pull/2338)
- Improved Drag and Drop by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2351](https://github.com/microsoft/qsharp/pull/2351)
- Support return values from custom intrinsics by [@swernli](https://github.com/swernli) in [#2350](https://github.com/microsoft/qsharp/pull/2350)
- Added tuple unpacking samples by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2381](https://github.com/microsoft/qsharp/pull/2381)
- Add/Remove Qubit Lines through Drag-and-Drop by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2372](https://github.com/microsoft/qsharp/pull/2372)
- Fix bug with Circuit CSS not being applied to notebooks by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2395](https://github.com/microsoft/qsharp/pull/2395)
- Copilot tools for run, estimate, circuit by [@minestarks](https://github.com/minestarks) in [#2380](https://github.com/microsoft/qsharp/pull/2380)
- Break on `fail` during debugging by [@swernli](https://github.com/swernli) in [#2400](https://github.com/microsoft/qsharp/pull/2400)
- Add explicit cast support by [@orpuente-MS](https://github.com/orpuente-MS) in [#2377](https://github.com/microsoft/qsharp/pull/2377)
- Restore fancy error reporting in Python by [@swernli](https://github.com/swernli) in [#2410](https://github.com/microsoft/qsharp/pull/2410)
- OpenQASM Grover's algorithm sample by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2398](https://github.com/microsoft/qsharp/pull/2398)
- OpenQASM Bernstein-Vazirani sample by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2403](https://github.com/microsoft/qsharp/pull/2403)
- Added OpenQASM samples as templates in VSCode by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2416](https://github.com/microsoft/qsharp/pull/2416)
- Support Array Update Syntax by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2414](https://github.com/microsoft/qsharp/pull/2414)
- Needless operation lint should ignore lambdas by [@swernli](https://github.com/swernli) in [#2406](https://github.com/microsoft/qsharp/pull/2406)
- Added OpenQASM Ising sample by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2435](https://github.com/microsoft/qsharp/pull/2435)
- Adding copilot instructions by [@idavis](https://github.com/idavis) in [#2436](https://github.com/microsoft/qsharp/pull/2436)
- Check that non-void functions always return by [@orpuente-MS](https://github.com/orpuente-MS) in [#2434](https://github.com/microsoft/qsharp/pull/2434)
- Added OpenQASM simple teleportation sample by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2441](https://github.com/microsoft/qsharp/pull/2441)
- Add sample Python integration and resource estimation notebooks for OpenQASM by [@idavis](https://github.com/idavis) in [#2437](https://github.com/microsoft/qsharp/pull/2437)
- Fix panic due to missing `Unit` value from assignment by [@swernli](https://github.com/swernli) in [#2452](https://github.com/microsoft/qsharp/pull/2452)
- Added OpenQASM samples into the VSCode Playground by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2458](https://github.com/microsoft/qsharp/pull/2458)

**Full Changelog**: [v1.16.0...v1.17.0](https://github.com/microsoft/qsharp/compare/v1.16.0...v1.17.0)

## v1.16.0

### Copilot integration

With VS Code Copilot integration you can now use Copilot to to assist with many tasks such as writing code, generating tests, connecting to an Azure Quantum workspace, submit jobs to run on hardware, and more!

<img width="547" alt="image" src="https://github.com/user-attachments/assets/f417ef8f-be4c-4ae5-9c0e-c0c18b3e7021" />

See the wiki page at <https://github.com/microsoft/qsharp/wiki/Make-the-most-of-the-QDK-and-VS-Code-agent-mode> for more info, as well as tips and best practices.

### Circuit Editor

You can now add .qsc files to your project which provide a drag-and-drop circuit editor user interface to create quantum operations, which can then be called from your Q# code.

<img width="1133" alt="image" src="https://github.com/user-attachments/assets/d4e492ab-8232-4392-908d-f0a6f9b8d45b" />

See the wiki page at <https://github.com/microsoft/qsharp/wiki/Circuit-Editor> for more details.

### Other notable changes

- Fix Test Explorer issues by [@billti](https://github.com/billti) in [#2291](https://github.com/microsoft/qsharp/pull/2291)
- Circuit Editor by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2238](https://github.com/microsoft/qsharp/pull/2238)
- Add lint groups to Q# by [@orpuente-MS](https://github.com/orpuente-MS) in [#2103](https://github.com/microsoft/qsharp/pull/2103)
- Added RoundHalfAwayFromZero to standard library by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2321](https://github.com/microsoft/qsharp/pull/2321)
- Added BigIntAsInt to Std.Convert by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2325](https://github.com/microsoft/qsharp/pull/2325)
- Added ApplyOperationPowerA to Std.Canon by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2324](https://github.com/microsoft/qsharp/pull/2324)
- Add Python evaluation API by [@idavis](https://github.com/idavis) in [#2345](https://github.com/microsoft/qsharp/pull/2345)
- Add "Update Copilot instructions" command by [@minestarks](https://github.com/minestarks) in [#2343](https://github.com/microsoft/qsharp/pull/2343)
- Add Ising model samples by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2342](https://github.com/microsoft/qsharp/pull/2342)
- Add GitHub Copilot tools for Azure Quantum by [@minestarks](https://github.com/minestarks) in [#2349](https://github.com/microsoft/qsharp/pull/2349)

**Full Changelog**: [v1.15.0...v1.16.0](https://github.com/microsoft/qsharp/compare/v1.15.0...v1.16.0)

## v1.15.0

### New `QuantumArithmetic` library

This release is the first to add the `QuantumArithmetic` library by [@fedimser](https://github.com/fedimser) to the list of suggested libraries! Check out more about the library at <https://github.com/fedimser/quant-arith-re>.

### Measurement and qubit reuse decompositions handled in QIR generation

This change addresses a long-standing point of confusion regarding how programs compiled for QIR Base profile are displayed in places like the circuit visualizer. By delaying application of decompositions for deferred measurement and avoidance of qubit reuse to the QIR generation step, the stdlib implementation of measurement no longer needs to have a different implementation for Base profile vs other profiles. This should make the displayed circuits match the written code more often. See #2230 for more details.

### Other notable changes

- Refactoring of chemistry library by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2208](https://github.com/microsoft/qsharp/pull/2208)
- Distance coefficient power parameter in QEC scheme by [@msoeken](https://github.com/msoeken) in [#2212](https://github.com/microsoft/qsharp/pull/2212)
- Fixed coefficients in qubit kata explanation. by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2225](https://github.com/microsoft/qsharp/pull/2225)
- Fix name resolution from a project's Main.qs by [@swernli](https://github.com/swernli) in [#2217](https://github.com/microsoft/qsharp/pull/2217)
- Enums used with Qiskit can now be deep copied by [@idavis](https://github.com/idavis) in [#2224](https://github.com/microsoft/qsharp/pull/2224)
- Change default font to system-ui by [@billti](https://github.com/billti) in [#2234](https://github.com/microsoft/qsharp/pull/2234)
- Removed usage of deprecated set keyword from the samples by [@filipw](https://github.com/filipw) in [#2233](https://github.com/microsoft/qsharp/pull/2233)
- Update @vscode/extension-telemetry to 0.9.8 by [@minestarks](https://github.com/minestarks) in [#2235](https://github.com/microsoft/qsharp/pull/2235)
- Require ctrl key to zoom histogram by [@billti](https://github.com/billti) in [#2249](https://github.com/microsoft/qsharp/pull/2249)
- Use fresh env for `qsharp.run` with Python interop functions wrapping Q# operations by [@swernli](https://github.com/swernli) in [#2255](https://github.com/microsoft/qsharp/pull/2255)
- Rework of simple samples by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2240](https://github.com/microsoft/qsharp/pull/2240)

**Full Changelog**: [v1.14.0...v1.15.0](https://github.com/microsoft/qsharp/compare/v1.14.0...v1.15.0)

## v1.14.0

### Notable Changes

- fix `qsharp.init(project_root='.')` by [@minestarks](https://github.com/minestarks) in [#2147](https://github.com/microsoft/qsharp/pull/2147)
- `Relabel` in adaptive conditional block should be disallowed by [@swernli](https://github.com/swernli) in [#2155](https://github.com/microsoft/qsharp/pull/2155)
- add UDT to list of terms when processing reexport by [@sezna](https://github.com/sezna) in [#2154](https://github.com/microsoft/qsharp/pull/2154)
- Chemistry library, SPSA sample, and notebook by [@swernli](https://github.com/swernli) in [#2105](https://github.com/microsoft/qsharp/pull/2105)
- Update registry for chemistry by [@billti](https://github.com/billti) in [#2161](https://github.com/microsoft/qsharp/pull/2161)
- LS: Hide errors from github sources only when they're not associated with a real project by [@minestarks](https://github.com/minestarks) in [#2139](https://github.com/microsoft/qsharp/pull/2139)
- Update devcontainer to Ubuntu noble base image by [@swernli](https://github.com/swernli) in [#2159](https://github.com/microsoft/qsharp/pull/2159)
- Fixes possible infinite loop in RE by [@msoeken](https://github.com/msoeken) in [#2128](https://github.com/microsoft/qsharp/pull/2128)
- Annotate conditional compilation for fixed_point so it compiles in all profiles by [@sezna](https://github.com/sezna) in [#2156](https://github.com/microsoft/qsharp/pull/2156)
- Fix bug in Unselect operation when selecting a single bit string by [@msoeken](https://github.com/msoeken) in [#2181](https://github.com/microsoft/qsharp/pull/2181)
- Support code lenses on more callables by [@swernli](https://github.com/swernli) in [#2174](https://github.com/microsoft/qsharp/pull/2174)
- Fix Circuit codelens by [@swernli](https://github.com/swernli) in [#2192](https://github.com/microsoft/qsharp/pull/2192)
- Fix RCA panic on lambda with explicit return by [@swernli](https://github.com/swernli) in [#2194](https://github.com/microsoft/qsharp/pull/2194)
- Fix Run and Debug buttons by [@swernli](https://github.com/swernli) in [#2196](https://github.com/microsoft/qsharp/pull/2196)
- Uniform superposition preparation moved to std by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2195](https://github.com/microsoft/qsharp/pull/2195)
- Multi-target gate circuit art improvement by [@Morcifer](https://github.com/Morcifer) in [#2185](https://github.com/microsoft/qsharp/pull/2185)
- Fix typo in code action by [@cesarzc](https://github.com/cesarzc) in [#2197](https://github.com/microsoft/qsharp/pull/2197)
- Add Azure credits notification message. by [@swernli](https://github.com/swernli) in [#2201](https://github.com/microsoft/qsharp/pull/2201)
- Chemistry lib: readablility, lints, syntax, tests by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2202](https://github.com/microsoft/qsharp/pull/2202)
- Bump version for 1.14 by [@billti](https://github.com/billti) in [#2203](https://github.com/microsoft/qsharp/pull/2203)

**Full Changelog**: [v1.13...v1.14.0](https://github.com/microsoft/qsharp/compare/v1.13...v1.14.0)

## v1.13.0

We are excited to release v1.13 of the Azure Quantum Development Kit! Here are some highlights of features included in this month's release:

### `@Test` Attribute and VS Code Test Explorer Integration

[#2095](https://github.com/microsoft/qsharp/pull/2095) Introduced a new attribute, `@Test`, which identifies unit tests written in Q#. By integrating with the Text Explorer feature in VS Code, you can now explore, run, and review Q# unit test execution:

<img width="874" alt="image" src="https://github.com/user-attachments/assets/f9db628d-59b3-4f5a-8b52-8e466bd1b69c" />

See the wiki page on [Testing Q# Code in VS Code](https://github.com/microsoft/qsharp/wiki/Testing-Q%23-Code-in-VS-Code) for more information.

### "Q#: Add Project Reference" VS Code Command Enhancements

[#2079](https://github.com/microsoft/qsharp/pull/2079) enhanced the VS Code command for adding references to a Q# project, available when editing a `qsharp.json` file:

<img width="835" alt="image" src="https://github.com/user-attachments/assets/c2060070-9628-4723-8750-bb9b62442755" />

When invoking the command, you'll now see a choice to either import from GitHub or search the currently opened workspace for other Q# projects. When choosing GitHub, you'll get a suggestion of known libraries and their available versions to choose from, and the corresponding external project reference snippet will automatically be added to your current `qsharp.json`:

<img width="835" alt="image" src="https://github.com/user-attachments/assets/d09d750b-ed50-4a79-b0b6-e6971574b88d" />

<img width="835" alt="image" src="https://github.com/user-attachments/assets/026f1aba-a043-46e4-82d6-223248dd10a0" />

### More Python Interoperability for Callables

[#2091](https://github.com/microsoft/qsharp/pull/2091) added more support for using Python functions that wrap Q# callables across our Python package APIs. This makes it easier to pass Python arguments into Q# for features like resource estimation with `qsharp.estimate()`, running multiple shots with `qsharp.run()`, compiling to QIR with `qsharp.compile()`, or generating circuits with `qsharp.circuit()`:
![image](https://github.com/user-attachments/assets/cf3c4da1-ed2c-494d-8c04-d2db8a57fccd)

For more information on using Q# callables directly in Python, see the [Invoking Q# Callables from Python](https://github.com/microsoft/qsharp/wiki/Invoking-Q%23-callables-from-Python) wiki page.

### Adaptive Profile Floating-Point Computation Extension

[#2078](https://github.com/microsoft/qsharp/pull/2078) added support for an additional QIR Adaptive profile extension: floating-point computation. By choosing `QIR Adaptive RIF` as your compilation profile, you can enable **R**eset, **I**nteger computation, and **F**loating-point computation for code generation. This allows you to write programs where the values of variables with Q# type `Double` can be dyanmically calculated from measurement results at runtime, and the output QIR will include arithmetic and comparison instructions corresponding to your code, enabling even more adaptive algorithms.

<img width="1018" alt="image" src="https://github.com/user-attachments/assets/35f92103-d0f9-4737-9d79-7d3d4244af36" />

Note that this profile extension must be supported by the target backend or runtime environment for the resulting code to execute. See the QIR specification section on [Classical Computation extensions](https://github.com/qir-alliance/qir-spec/blob/main/specification/under_development/profiles/Adaptive_Profile.md#bullet-5-classical-computations) to the Adaptive profile for more details.

### Other notable changes

- Fix `Relabel` for odd size arrays by [@swernli](https://github.com/swernli) in [#2082](https://github.com/microsoft/qsharp/pull/2082)
- Syntax highlighting for functions, variables and numbers by [@Morcifer](https://github.com/Morcifer) in [#2088](https://github.com/microsoft/qsharp/pull/2088)
- Fix `Exp` on qubit arrays larger than 2 with single `PauliI` by [@swernli](https://github.com/swernli) in [#2086](https://github.com/microsoft/qsharp/pull/2086)
- Mutable variables in dynamic branches prevent full constant folding in partial evaluation by [@swernli](https://github.com/swernli) in [#2089](https://github.com/microsoft/qsharp/pull/2089)
- Add `TestMatrix` functionality to qtest by [@sezna](https://github.com/sezna) in [#2037](https://github.com/microsoft/qsharp/pull/2037)
- Added simple VQE sample by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2073](https://github.com/microsoft/qsharp/pull/2073)
- Fix global phase for controlled-T, R1 by [@swernli](https://github.com/swernli) in [#2112](https://github.com/microsoft/qsharp/pull/2112)
- Fix widgets sometimes rendering in light theme when VS Code is in a dark theme by [@billti](https://github.com/billti) in [#2120](https://github.com/microsoft/qsharp/pull/2120)
- LookAheadDKRSAddLE now accepts carry-in by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2119](https://github.com/microsoft/qsharp/pull/2119)
- Add lint for double (in)equality by [@orpuente-MS](https://github.com/orpuente-MS) in [#2104](https://github.com/microsoft/qsharp/pull/2104)
- Replaced custom ApplyAndAssuming0Target with AND from std by [@DmitryVasilevsky](https://github.com/DmitryVasilevsky) in [#2123](https://github.com/microsoft/qsharp/pull/2123)
- Fix language service panic when file isn't listed in the `files` field of `qsharp.json` by [@minestarks](https://github.com/minestarks) in [#2109](https://github.com/microsoft/qsharp/pull/2109)
- Long gate in ASCII art circuits - lengthen column width when necessary by [@Morcifer](https://github.com/Morcifer) in [#2126](https://github.com/microsoft/qsharp/pull/2126)
- Fix UDT re-exports by [@sezna](https://github.com/sezna) in [#2137](https://github.com/microsoft/qsharp/pull/2137)

**Full Changelog**: [v1.12...v1.13](https://github.com/microsoft/qsharp/compare/v1.12...v1.13)

## v1.12.0

We are excited to release v1.12 of the Azure Quantum Development Kit! Here are some highlights of features included in this month's release:

### Python interoperability improvements

You can now import and invoke your Q# callables for simulation directly from Python as functions. Your callables defined in `%%qsharp` magic cells, through calls to `qsharp.eval`, or loaded from projects in `qsharp.init` can now be imported from the `qsharp.code` module:

```python
import qsharp

qsharp.eval("""
    operation Superposition() : Result {
        use q = Qubit();
        H(q);
        Std.Diagnostics.DumpMachine();
        MResetZ(q)
    }
    """)

from qsharp.code import Superposition
result = Superposition()
```

For more details and current limitations, see [Invoking Q# callables from Python](https://github.com/microsoft/qsharp/wiki/Invoking-Q%23-callables-from-Python) in the wiki.

Syntax for capturing state dumps from `DumpMachine` or `DumpRegister` and operation matrices from `DumpOperation` calls in your Q# code has also been improved (see #2042)

### Deprecation of `set` keyword

The `set` keyword used for updating mutable values is now deprecated, so where you previously had to use `set x += 1` you can now just write `x += 1`. In addition, the compiler includes a new lint that defaults to "allow" that you can use to warn or error on usage of `set` in your code (see #2062).

### `ApplyUnitary` operation for simulation

When running against the simulator, your Q# code can call `ApplyUnitary` and pass a unitary matrix represented by a `Std.Math.Complex[][]` along with an array of qubit targets and have the simulator directly apply that unitary to the current sparse state vector.

### Increase minimum versions for Python and Ubuntu

Starting with v1.12, the minimum supported Python version for the qsharp package is Python 3.9. Along with this change, the minimum compatible version of Ubuntu has been increased to 22.04 (see #2061)

**Full Changelog**: [v1.11.1...v1.12](https://github.com/microsoft/qsharp/compare/v1.11.1...v1.12)

## v1.11.1

We are excited to release v1.11.1 of the Azure Quantum Development Kit! This month's release includes features and bug fixes, such as:

### Configure Pauli Noise Dynamically within Q#

You can now use the `ConfigurePauliNoise` function to dynamically update noise settings during simulation, allowing samples, exercises, or test code to directly set noise used in testing [#1997](https://github.com/microsoft/qsharp/pull/1997)

### Stabilization of the Microsoft.Quantum.Unstable libraries

The `Arithmetic`, `StatePreparation`, and `TableLookup` libraries have been stabilized and are now available under `Std`. Several samples and libraries have been updated to reflect the new location, while the `Microsoft.Quantum.Unstable` namespace will be preserved for backward compatibility [#2022](https://github.com/microsoft/qsharp/pull/2022), [#2043](https://github.com/microsoft/qsharp/pull/2043)

### Support for Qiskit v1.3.0

Changes made to the Qiskit target class in v1.3.0 that broke interoperability with the qsharp Python package are now handled dynamically allowing use of both v1.2 and v1.3 versions of Qiskit [#2050](https://github.com/microsoft/qsharp/pull/2050)

### Other notable changes

- Add three qubit repetition sample to playground by [@swernli](https://github.com/swernli) in [#2003](https://github.com/microsoft/qsharp/pull/2003)
- Add eval and cell events by [@idavis](https://github.com/idavis) in [#2004](https://github.com/microsoft/qsharp/pull/2004)
- Avoid flooding iPython display by [@swernli](https://github.com/swernli) in [#2006](https://github.com/microsoft/qsharp/pull/2006)
- Fix to RCA panic when mapping a tuple input pattern to a non-tuple expression by [@cesarzc](https://github.com/cesarzc) in [#2011](https://github.com/microsoft/qsharp/pull/2011)
- Fix RCA panic by [@orpuente-MS](https://github.com/orpuente-MS) in [#2017](https://github.com/microsoft/qsharp/pull/2017)
- Add class constraints for built-in classes by [@sezna](https://github.com/sezna) in [#2007](https://github.com/microsoft/qsharp/pull/2007)
- Track qubit live-ness during simulation by [@swernli](https://github.com/swernli) in [#2020](https://github.com/microsoft/qsharp/pull/2020)
- Add `Qtest` library that uses class constraints by [@sezna](https://github.com/sezna) in [#2013](https://github.com/microsoft/qsharp/pull/2013)
- Include samples in completions when the document is empty by [@minestarks](https://github.com/minestarks) in [#2009](https://github.com/microsoft/qsharp/pull/2009)
- Remove Refs to Microsoft.Quantum in Libraries by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2041](https://github.com/microsoft/qsharp/pull/2041)
- Add custom operations sample by [@orpuente-MS](https://github.com/orpuente-MS) in [#1995](https://github.com/microsoft/qsharp/pull/1995)
- VS Code extension throws on launch if Q# notebook cell is open by [@minestarks](https://github.com/minestarks) in [#2044](https://github.com/microsoft/qsharp/pull/2044)
- Library for rotation operations by [@msoeken](https://github.com/msoeken) in [#2040](https://github.com/microsoft/qsharp/pull/2040)
- Remove Refs to Microsoft.Quantum in Samples and Katas by [@ScottCarda-MS](https://github.com/ScottCarda-MS) in [#2030](https://github.com/microsoft/qsharp/pull/2030)

**Full Changelog**: [v1.10.1...v1.11.1](https://github.com/microsoft/qsharp/compare/v1.10.1...v1.11.1)

## v1.10.1

We are excited to release v1.10 of the Azure Quantum Development Kit! This month's release includes several new features and improvements including:

### Code editing improvements

Code editing is now greatly improved. A couple of examples of the many improvements:

- **Context aware completions** (#1947) show only the relevant completions for the location. For example, only showing types when in a type position:

  <img width="346" alt="image" src="https://github.com/user-attachments/assets/347c4fc9-c746-438d-bb1a-998b65375e79">

- **Namespace member** (#1947) completion lists are now provided when drilling into namespaces:

  <img width="346" alt="image" src="https://github.com/user-attachments/assets/09c92c4d-68e6-4fbd-b647-9a4fc064b50c">

- **User Defined Type** (#1954) member completions are now populated

  <img width="383" alt="image" src="https://github.com/user-attachments/assets/577addec-32f7-465f-9c3d-b865e033b42c">

And much more! Parser error recovery has also been greatly improved so that editor assistance is available whilst mid-edit in many more scenarios.

### Noisy simulation

You can now add Pauli noise to simulations run from Python or VS Code (#1971, #1975, #1980). This can help model the results of running on a real quantum machine for education purposes, and to help develop and test the effectiveness of error correction.

Below shows the results of configuring 5% bit-flip noise in VS Code and running a histogram on the GHZ sample. This would return only $\ket{000}$ and $\ket{111}$ shot results if run in a noise free simulation.

<img width="944" alt="image" src="https://github.com/user-attachments/assets/5274198c-ece5-4049-ac44-ae9e829fc51d">

To see how to use noisy simulation in Python, check out the sample notebook at <https://github.com/microsoft/qsharp/blob/main/samples/notebooks/noise.ipynb>

### Refreshed API docs interface

The in-editor Q# API documentation has had a UI refresh (#1978). This is accessed via the "Q#: Show API documentation" command in the command palette when editing a Q# file. The new UX allows you to quickly &amp; easily search &amp; navigate the APIs within your project, referenced projects, and the standard library.

<img width="792" alt="image" src="https://github.com/user-attachments/assets/0b6e6026-492f-461d-a9b1-30762915020f">

### File icons

The Q# file extension (.qs) now gets a unique icon in VS Code (#1976)

<img width="279" alt="image" src="https://github.com/user-attachments/assets/6af754e0-b6fa-45b4-b68e-6e379e988cd4">

### Custom measurements and resets

Previously you could define custom gates, but not custom measurement or reset operations. With #1967, #1981, and #1985 this is now possible. This allows for the definition and use of custom operations for quantum simulation and QIR code generation.

Samples for this feature will be added shortly, in the meantime see the test code at <https://github.com/microsoft/qsharp/blob/v1.10.1/compiler/qsc/src/codegen/tests.rs#L529> for an example of how this may be used.

### Python telemetry

In this release we have added telemetry to our `qsharp` Python package to collect minimal and anonymous metrics on feature usage and performance. This will allow us to focus our investments going forward on the most valuable areas. Please see the [notes in the package readme](https://github.com/microsoft/qsharp/blob/v1.10.1/pip/README.md#telemetry) for details on what is collected and how to disable it.

### Other notable changes

- Added DoubleAsStringWithPrecision function - Multiple Katas by @devikamehra in https://github.com/microsoft/qsharp/pull/1897
- Add summary lines back into stdlib readmes by @sezna in https://github.com/microsoft/qsharp/pull/1952
- add Q# package registry document by @sezna in https://github.com/microsoft/qsharp/pull/1932
- Error budget pruning strategy in resource estimator core by @msoeken in https://github.com/microsoft/qsharp/pull/1951
- Use all github dependencies in published libraries by @sezna in https://github.com/microsoft/qsharp/pull/1956
- Fix to partial evaluation generating branch instructions on constant conditions by @cesarzc in https://github.com/microsoft/qsharp/pull/1963
- Show no quantum state when debugging code with no qubits by @swernli in https://github.com/microsoft/qsharp/pull/1953
- More precise completions and namespace member completions by @minestarks in https://github.com/microsoft/qsharp/pull/1947
- UDT field completions by @minestarks in https://github.com/microsoft/qsharp/pull/1954
- Added Pauli noise support to sparse simulator by @DmitryVasilevsky in https://github.com/microsoft/qsharp/pull/1971
- Add custom measurement operations to Q# by @orpuente-MS in https://github.com/microsoft/qsharp/pull/1967
- Expose pauli noise settings in VS Code by @billti in https://github.com/microsoft/qsharp/pull/1975
- Python clients can now run simulation with Pauli noise by @DmitryVasilevsky in https://github.com/microsoft/qsharp/pull/1974
- Add .qs file icons by @billti in https://github.com/microsoft/qsharp/pull/1976
- Python: Result should implement comparison operators by @minestarks in https://github.com/microsoft/qsharp/pull/1979
- Improve the built-in API docs UX by @billti in https://github.com/microsoft/qsharp/pull/1978
- Added sample notebook with noise by @DmitryVasilevsky in https://github.com/microsoft/qsharp/pull/1980
- Add support for custom resets using the `@Reset()` attribute by @orpuente-MS in https://github.com/microsoft/qsharp/pull/1981
- Unify implementations of custom measurements and custom resets by @orpuente-MS in https://github.com/microsoft/qsharp/pull/1985
- Initial Python telemetry by @billti in https://github.com/microsoft/qsharp/pull/1972
- More parser error recovery, unlocking completions in more locations by @minestarks in https://github.com/microsoft/qsharp/pull/1987
- `DumpMachine` output in Python and console should be empty with no qubits allocated by @swernli in https://github.com/microsoft/qsharp/pull/1984
- Disable completions in attribute arguments by @minestarks in https://github.com/microsoft/qsharp/pull/1986
- No completions in comments by @minestarks in https://github.com/microsoft/qsharp/pull/1999

**Full Changelog**: https://github.com/microsoft/qsharp/compare/v1.9.0...v1.10.1

## v1.9.0

The 1.9.0 release of the QDK includes interoperability with Qiskit circuits built upon the core Q# compiler infrastructure.

The Qiskit interop provided by the QDK includes:

- Resource estimation for their Qiskit circuits locally
- Q# Simulation of Qiskit circuits using Q#'s simulation capabilities
- QIR generation from Qiskit circuits leveraging the [modern QDKs advanced code generation capabilities](https://devblogs.microsoft.com/qsharp/integrated-hybrid-support-in-the-azure-quantum-development-kit/).

The [Qiskit interop wiki page](https://github.com/microsoft/qsharp/wiki/Qiskit-Interop) provides a brief overview of the integration while detailed examples, potential errors, and usage with parameterized circuits are demonstrated in the [sample Qiskit interop notebook](https://github.com/microsoft/qsharp/tree/main/samples/python_interop/qiskit.ipynb).

In addition to the Qiskit interop feature, the language service for Q# will now auto-suggest the new standard library API instead of the legacy `Microsoft.Quantum`-prefixed standard library API. For example, when typing `DumpMachine`, you'll now get a suggested import for `Std.Diagnostics.DumpMachine` instead of `Microsoft.Quantum.Diagnostics.DumpMachine`.

### Other notable changes

- Port signed integer math to modern QDK by @sezna in https://github.com/microsoft/qsharp/pull/1841
- Add samples of testing Q# code that prepares a quantum state by @tcNickolas in https://github.com/microsoft/qsharp/pull/1873
- Simplify display of evaluation results in VS Code by @swernli in https://github.com/microsoft/qsharp/pull/1882
- Update samples to reflect latest 1.7 changes; Update katas and stdlib to use structs by @sezna in https://github.com/microsoft/qsharp/pull/1797
- Remove profile selection for Katas by @JPark1023 in https://github.com/microsoft/qsharp/pull/1881
- Include CompareGTSI in the Signed math API by @sezna in https://github.com/microsoft/qsharp/pull/1888
- Update Placeholder.qs by @HopeAnnihilator in https://github.com/microsoft/qsharp/pull/1890
- Implements serialization for physical resource estimation by @msoeken in https://github.com/microsoft/qsharp/pull/1892
- Added DoubleAsStringWithPrecision function - Complex Arithmetics by @devikamehra in https://github.com/microsoft/qsharp/pull/1883
- Generic code with code distance and threshold by @msoeken in https://github.com/microsoft/qsharp/pull/1896
- Basic interop with Qiskit by @idavis in https://github.com/microsoft/qsharp/pull/1899
- Use T gate time for physical factories by @msoeken in https://github.com/microsoft/qsharp/pull/1906
- Basic samples for RE API by @msoeken in https://github.com/microsoft/qsharp/pull/1915
- Introduce `Relabel` API by @swernli in https://github.com/microsoft/qsharp/pull/1905
- Support Adjoint of `Relabel` by @swernli in https://github.com/microsoft/qsharp/pull/1920
- Migrate the standard library to the project system by @sezna in https://github.com/microsoft/qsharp/pull/1912
- Add ProtocolSpecification to API by @msoeken in https://github.com/microsoft/qsharp/pull/1931
- Update circuits widget sizing behavior by @swernli in https://github.com/microsoft/qsharp/pull/1921
- Fix bug preventing display of circuits where same qubit measured more than once by @swernli in https://github.com/microsoft/qsharp/pull/1939
- Add DumpOperation support in Q# by @billti in https://github.com/microsoft/qsharp/pull/1885
- Control how physical qubits are computed in factories by @msoeken in https://github.com/microsoft/qsharp/pull/1940

**Full Changelog**: https://github.com/microsoft/qsharp/compare/v1.8.0...v1.9.0

## v1.8.0

The 1.8.0 release of the QDK includes a number of improvements and fixes, with a focus on refining the project references and editor completions experience.

**Full Changelog**: https://github.com/microsoft/qsharp/compare/v1.7.0...v1.8.0

## v1.7.0

The team is _very_ excited to ship this release. It has some of the most significant improvements to the Q# language in a long time.

### External project references

The biggest feature in this release is the ability to reference other projects and consume their APIs. The projects can be in a separate local directory or published to GitHub. As part of this change, we also introduced `import` and `export` syntax, and generate an implicit namespace hierarchy based on file layout, removing the need for the `namespace` syntax.

For more details see the wiki page at <https://github.com/microsoft/qsharp/wiki/Q%23-External-Dependencies-(Libraries)>. (The official documentation will be updated shortly with more details and examples).

### New struct syntax

We're also introducing a new `struct` syntax, and long term see this as the replacement for the current `UDT` syntax. The custom types created by either are largely compatible, but the new syntax is simpler, cleaner, and similar to several popular languages. See more details at <https://github.com/microsoft/qsharp/wiki/Q%23-Structs> until the official docs are updated.

### Optional EntryPoint

As well as removing the need to wrap code in a `namespace`, we're also removing the need to specify the `EntryPoint` attribute. If you have one callable called `Main` in your project, this will be the default entry point. (Note: Any specified `@EntryPoint` will still take precedence).

### A new standard library namespace

We've also simplified the namespaces for our standard library. What was previously all under `Microsoft.Quantum` can now be accessed under the `Std` namespace. This reduces visual clutter and highlights what APIs are part of the "standard" library included with Q#.

### Example

Taken together the above provides for a much cleaner language with a simple code sharing mechanism. For example, if your project references another project named `Sparkle` which exports an operation named `Correct` that takes a custom type `Input` with a Double and a Qubit, your entire Q# code to call this can be as simple as:

```qsharp
import Std.Diagnostics.DumpMachine;
import Sparkle.Input, Sparkle.Correct;

operation Main() : Unit {
    use q = Qubit[1];
    let x = new Input { A = 3.14, B = q[0] };

    Correct(x);

    DumpMachine();
    MResetZ(q[0]);
}
```

(Note these changes are additional capabilities. There are no breaking changes or requirements to change code to adopt this release).

### Other notable changes

Many other changes have gone into this release. Some of the main ones include:

- Unitary Hack contributions
  - Save RE widget to .png (#1604)
  - Lint rule: Use Functions (#1579)
  - Add doc for internal AND (#1580)
- New DrawRandomBool API (#1645)
- New DoubleAsStringWithPrecision API (#1664)
- Fix display of CCX in a circuit (#1685)
- Completion list improvements (#1682, #1715)
- New samples (e.g. #1721)
- Many more Katas additions and updates
- Various bug fixes and perf improvements
- Lots of engineering improvements to build times, testing, pipelines, etc.

**Full Changelog**: https://github.com/microsoft/qsharp/compare/v1.6.0...v1.7.0

We hope you enjoy this release. Please log an issue if you need any assistance or to provide feedback. Thanks!

## v1.6.0

Welcome to the v1.6.0 release of the Azure Quantum Development Kit!

The big feature in this release is the ability to compile Q# programs to QIR that require "Adaptive Profile" capabilities. This enables programs to take advantage of the latest capabilities of quantum hardware, such as the ability to perform mid-circuit measurement of qubits, branch based on the results, and perform some classical computations at runtime. For more details, see <https://aka.ms/qdk.qir>.

We've added or updated a number of samples that can leverage Adaptive Profile capabilities, such as the [Three Qubit Repetition Code](https://github.com/microsoft/qsharp/blob/main/samples/algorithms/ThreeQubitRepetitionCode.qs) and the [Iterative Phase Estimation notebook](https://github.com/microsoft/qsharp/blob/main/samples/notebooks/iterative_phase_estimation.ipynb). Please do try it out and give us your feedback!

As part of the above work, the previous code generation approach was replaced, even in the non-Adaptive (_"base profile"_) case. Please log an issue if you see any unexpected change in behavior.

Other notable new features include Q# linting support in Jupyter Notebooks, CodeActions in VS Code to fix certain Q# errors, Q# library documentation inside VS Code, and more!

### Other notable changes

- Add linting support to notebooks (Closes #1277) by @orpuente-MS in https://github.com/microsoft/qsharp/pull/1313
- Use new QIR gen API for Base Profile by @idavis in https://github.com/microsoft/qsharp/pull/1400
- Allow generating circuits for operations despite no entrypoint error by @minestarks in https://github.com/microsoft/qsharp/pull/1432
- Fix lint message formatting by @orpuente-MS in https://github.com/microsoft/qsharp/pull/1444
- Change the default level of the DivisionByZero lint to "error" by @orpuente-MS in https://github.com/microsoft/qsharp/pull/1445
- Adding Adaptive RI profile by @idavis in https://github.com/microsoft/qsharp/pull/1451
- Handle impossible factories in RE API by @msoeken in https://github.com/microsoft/qsharp/pull/1463
- Fix global phase for `PauliI` rotation and `DumpRegister` by @swernli in https://github.com/microsoft/qsharp/pull/1461
- Documentation in the VSCode - core, std, and current project by @DmitryVasilevsky in https://github.com/microsoft/qsharp/pull/1466
- Three qubit repetition code sample works in Adaptive Profile by @DmitryVasilevsky in https://github.com/microsoft/qsharp/pull/1534
- GHZ and CAT samples work in Adaptive and Base Profiles by @DmitryVasilevsky in https://github.com/microsoft/qsharp/pull/1532
- Add messages to samples in /samples/language by @goshua13 in https://github.com/microsoft/qsharp/pull/1509
- Make `SpreadZ` utility iterative instead of recursive by @swernli in https://github.com/microsoft/qsharp/pull/1545
- Support target name in Python, remove Adaptive warnings by @swernli in https://github.com/microsoft/qsharp/pull/1549
- Avoid panic from `DumpRegister` in circuit display by @swernli in https://github.com/microsoft/qsharp/pull/1554
- Respect configured target profile for histogram in VS Code by @swernli in https://github.com/microsoft/qsharp/pull/1565
- Update to Rust 1.78 by @orpuente-MS in https://github.com/microsoft/qsharp/pull/1570
- Respect target setting for "Estimate" command by @swernli in https://github.com/microsoft/qsharp/pull/1576
- Add support for CodeActions in the Language Service by @orpuente-MS in https://github.com/microsoft/qsharp/pull/1495
- Read correct field in QEC scheme by @msoeken in https://github.com/microsoft/qsharp/pull/1602
- Fix panic when updating array with dynamic value by @swernli in https://github.com/microsoft/qsharp/pull/1606
- Added dot product via iterative phase estimation sample by @DmitryVasilevsky in https://github.com/microsoft/qsharp/pull/1562
- Reset zoom level when circuit window is resized by @minestarks in https://github.com/microsoft/qsharp/pull/1592
- Fix normalization math in `DumpRegister` by @swernli in https://github.com/microsoft/qsharp/pull/1608
- Adaptive quantum computing notebook samples by @cesarzc in https://github.com/microsoft/qsharp/pull/1614

**Full Changelog**: https://github.com/microsoft/qsharp/compare/v1.4.0...v1.6.0

## v1.4.0

Welcome to the v1.4.0 release of the Azure Quantum Development Kit. The main highlights of this release are:

- Circuit visualization by @minestarks in #1247, #1267 #1269, #1295, #1318, #1361, and more! See more details on this feature in the official docs, or in the [repository wiki](https://github.com/microsoft/qsharp/wiki/Circuit-Diagrams-from-Q%23-Code)
- Formatting improvements by @ScottCarda-MS in #1289, #1303, #1310, #1329
- Update language service when manifest is saved by @orpuente-MS in #1366

Other notable fixes and improvements include:

- Fix DumpMachine() output in VS Code debug console by @minestarks in https://github.com/microsoft/qsharp/pull/1299
- Fix completion auto-open position in notebook cells by @minestarks in https://github.com/microsoft/qsharp/pull/1398
- Update doc comments in std library by @DmitryVasilevsky in https://github.com/microsoft/qsharp/pull/1401

And lots of Katas updates! Including:

- Add state flip task to Single-Qubit Gates kata by @WWhitedogi in https://github.com/microsoft/qsharp/pull/1343
- Add tasks 1.8, 1.9, 1.10 to Superposition Kata by @jkingdon-ms in https://github.com/microsoft/qsharp/pull/1346
- Add sign flip, basis change, amplitude change tasks to Single-Qubit Gates kata by @WWhitedogi in https://github.com/microsoft/qsharp/pull/1352
- Add global phase -1, relative phase i, and complex relative phase tasks to Single-Qubit Gates kata by @WWhitedogi in https://github.com/microsoft/qsharp/pull/1369
- Add tasks 1.11, 1.12 to Superposition Kata by @jkingdon-ms in https://github.com/microsoft/qsharp/pull/1381
- Add task 2.1 to Superposition kata by @tcNickolas in https://github.com/microsoft/qsharp/pull/1395
- Update READMEs to add details on building playground and katas by @Manvi-Agrawal in https://github.com/microsoft/qsharp/pull/1402
- Add tasks on Bell states changes to Multi-Qubit States kata by @WWhitedogi in https://github.com/microsoft/qsharp/pull/1385
- Add CZ section and CNOT and CZ tasks to Multi-Qubit Gates kata by @WWhitedogi in https://github.com/microsoft/qsharp/pull/1389
- Adds task 1.13 to Superposition Kata by @frtibble in https://github.com/microsoft/qsharp/pull/1382

**Full Changelog**: https://github.com/microsoft/qsharp/compare/v1.3.1...v1.4.0

## v1.3.1

Includes a fix for an issue rendering DumpMachine calls in VS Code.

## v1.3.0

Welcome to the v1.3.0 release of the Azure Quantum Development Kit. The main highlights of this release are:

- Initial support for linting (#1140)
- Document and selection formatting (#1172 and #1275)
- Authenticate to Azure Quantum workspaces via a connection string (#1238)
- Add a 'Create Q# project' command (#1286)
- Significant performance improvements from using mimalloc (#1249)
- More significant performance improvements via CFG usage (#1261)
- Add `Microsoft.Quantum.Measurement` to the prelude (#1233)
- Changes to the data returned by `dump_machine` and `dump_operation` (#1227)

And more! See <https://github.com/microsoft/qsharp/compare/v1.2.0...v1.3.0> for the full list of changes.

## v1.2.0

Welcome to the v1.2.0 release of the Azure Quantum Development Kit. The main highlights of this release are:

- Added the DumpRegister API (#1173)
- Added code distance to Resource Estimation tooltips (#1205)
- Use optimized AND for decomposition (#1202)
- Remove the "Message:" prefix from Message output by @colommar (#1175)
- Generate Q# API docs for [learn.microsoft.com](https://learn.microsoft.com/en-us/qsharp/api/qsharp-lang/) (#1150)
- Show codelens on entry point in VS Code to Run, Debug, Histogram, and Estimate (#1142)
- Support generating QIR with custom intrinsics (#1141)
- Fix hover info for lambdas passed to generic functions (#1161)
- Fix panic on in-place update optimization (#1149)
- Add boolean Xor API (#1100)

And much more! See <https://github.com/microsoft/qsharp/compare/v1.1.3...v1.2.0> for the full change log.

## v1.1.3

Welcome to the v1.1.3 release of the Azure Quantum Development Kit. This release is largely a bug fixing release of v1.1. Some notable changes include:

- Use fixed seed for random circuit generation in resource estimation sample in https://github.com/microsoft/qsharp/pull/1097
- Consolidate samples and run notebooks in build in https://github.com/microsoft/qsharp/pull/1070
- Fix typos in Q# standard lib documentation by @filipw in https://github.com/microsoft/qsharp/pull/1101
- Session now exits when there is a runtime failure when running without debugging in https://github.com/microsoft/qsharp/pull/1103
- Pure state preparation added to unstable standard library in https://github.com/microsoft/qsharp/pull/1068
- Use relevant icon for locals completion by @filipw in https://github.com/microsoft/qsharp/pull/1111
- Prefer open file contents to disk contents in https://github.com/microsoft/qsharp/pull/1110
- Fix BOM handling in Python in https://github.com/microsoft/qsharp/pull/1112
- Update spans used for some type mismatch errors in https://github.com/microsoft/qsharp/pull/1098
- Evaluator performance improvements in https://github.com/microsoft/qsharp/pull/1116
- Fix state ordering in Python in https://github.com/microsoft/qsharp/pull/1122
- Set notebook cell language back to Python if `%%qsharp` magic isn't there in https://github.com/microsoft/qsharp/pull/1118
- Clarify instructions on running the playground in https://github.com/microsoft/qsharp/pull/1134
- New factoring algorithm sample for resource estimation in https://github.com/microsoft/qsharp/pull/1058

**Full Changelog**: https://github.com/microsoft/qsharp/compare/v1.1.1...v1.1.3

## v1.1.1

Welcome to the v1.1 release of the Azure Quantum Development Kit. The main highlights of this release are:

- Space-time scatter charts for resource estimation via #985
- Additional samples targeted for use with resource estimation via #1019, #1033, and #1067
- Changes to the order of bits in the |ket\> representation via #1079
- Highlighting of errors in cells in Jupyter Notebooks via #1071
- New `dump_operation` API in Python via #1055
- Added `BoolArrayAsBigInt` to the standard library via #1047 (thanks @filipw)
- Added ability to set random seeds for quantum or classical simulation via #1053
- Various other minor fixes and improvements

## v1.0.33

Welcome to the v1.0 release of the Azure Quantum Development Kit. Being a version 1.0 release, this release includes all of our initial features, including:

- VS Code extension for desktop and web
- Rich Q# language service support
- A Q# compiler and simulator
- Vastly improved performance over the prior QDK
- Q# debugging
- The `qsharp` and `qsharp-widgets` Python packages.
- Jupyter Notebook integration
- Quantum Resource Estimation
- Azure Quantum service integration

And more! See the release blog post for more details at <https://devblogs.microsoft.com/qsharp/announcing-v1-0-of-the-azure-quantum-development-kit/>
