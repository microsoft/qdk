// # Sample
// Namespaces
//
// # Description
// Every Q# file defines a namespace. A namespace helps
// you organize related functionality this is useful when you are writing
// libraries or reusable code.
// The name of the file, minus the extension `.qs`, is the name of the namespace.

// The following `import` statement is used to import all types and callables declared in the
// Std.Diagnostics namespace.
import Std.Diagnostics.*;

function Main() : Result[] {
    // `DumpMachine` is in the Std.Diagnostics namespace
    DumpMachine();
    return [];
}
