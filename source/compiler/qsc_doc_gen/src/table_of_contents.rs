// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// Returns the string of the table of contents for the standard library.
/// This table of contents will be the content of the top-level index.md
/// created during documentation generation.
pub(super) fn table_of_contents() -> String {
    "# Q# standard library

The Q# standard library contains the following namespaces:

| Namespace                                                       | Description                                                          |
| --------------------------------------------------------------- | -------------------------------------------------------------------- |
| [`Microsoft.Quantum.Core`](xref:Qdk.Microsoft.Quantum.Core-toc) | Re-exported functions.                                               |
| [`Std.Arithmetic`](xref:Qdk.Std.Arithmetic-toc)                 | Items for working with quantum arithmetic operations.                |
| [`Std.Arrays`](xref:Qdk.Std.Arrays-toc)                         | Items for working with arrays.                                       |
| [`Std.Canon`](xref:Qdk.Std.Canon-toc)                           | Canonical implementations of common classical and quantum utilities. |
| [`Std.Convert`](xref:Qdk.Std.Convert-toc)                       | Items for converting between different types.                        |
| [`Std.Core`](xref:Qdk.Std.Core-toc)                             | Items for language built-in operations.                              |
| [`Std.Diagnostics`](xref:Qdk.Std.Diagnostics-toc)               | Items for debugging and testing quantum programs.                    |
| [`Std.Intrinsic`](xref:Qdk.Std.Intrinsic-toc)                   | Items that provide core quantum operations.                          |
| [`Std.Logical`](xref:Qdk.Std.Logical-toc)                       | Boolean Logic functions.                                             |
| [`Std.Math`](xref:Qdk.Std.Math-toc)                             | Items for classical math operations.                                 |
| [`Std.Measurement`](xref:Qdk.Std.Measurement-toc)               | Items for measuring quantum results.                                 |
| [`Std.Random`](xref:Qdk.Std.Random-toc)                         | Items for creating random values.                                    |
| [`Std.Range`](xref:Qdk.Std.Range-toc)                           | Items for working with ranges.                                       |
| [`Std.ResourceEstimation`](xref:Qdk.Std.ResourceEstimation-toc) | Items for working with the Microsoft Quantum Resource Estimator.     |
| [`Std.StatePreparation`](xref:Qdk.Std.StatePreparation-toc)     | Items for preparing a quantum state.                                 |
| [`Std.TableLookup`](xref:Qdk.Std.TableLookup-toc)               | Items for performing quantum table lookups.                          |
".to_string()
}
