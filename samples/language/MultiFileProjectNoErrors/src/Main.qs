/// # Sample
/// Multi File Project
///
/// # Description
/// Organizing code into multiple Q# source files is an important part of
/// writing readable and maintainable code. In this project, we have `Main.qs`,
/// and `Particle.qs`, which defines a new namespace for particle operations.
/// The presence of a Q# manifest file (`qsharp.json`) tells the compiler
/// to include all Q# files under `src/`.
namespace MyQuantumApp {
    open Particle;
    @EntryPoint()
    function Main() : Unit {

        // this is coming from local deps
        // Foo.DependencyA.MagicFunction(); <--- COMMENT THIS BACK IN, WORKS IN VS CODE

        // this is coming from github - minestarks/qsharp-project-template
        // GitHub.Diagnostics.DumpMachine_();  <--- COMMENT THIS BACK IN, WORKS IN VS CODE

        let particleA = Particle(0, 0, 0);
        let particleB = Particle(1, 1, 1);

        let particleC = addParticles(particleA, particleB);
    }
}
