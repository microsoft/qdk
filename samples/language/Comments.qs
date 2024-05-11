/// # Sample
/// Comments
///
/// # Description
/// Comments begin with two forward slashes (`//`) and continue until the
/// end of line. Comments may appear anywhere in the source code.
/// Q# does not currently support block comments.
/// Documentation comments, or doc comments, are denoted with three
/// forward slashes (`///`) instead of two.
namespace MyQuantumApp {
    open Microsoft.Quantum.Diagnostics;

    /// This is a doc-comment for the `Main` operation.
    @EntryPoint()
    operation Main() : Result[] {
        // Comments can go anywhere in a program, although they typically
        // preface what they refer to.
        return [];
    }
}
