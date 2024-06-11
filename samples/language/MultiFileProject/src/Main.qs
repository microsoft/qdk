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
    operation Main() : Unit {

        // TODO: This is a hacky way of referencing the dependency for now -
        // I'm just pulling in all the sources into the same compilation

        DependencyA.MagicFunction();
        DependencyB.MagicFunction();
        DependencyC.MagicFunction();

        let particleA = Particle(0, 0, 0);
        let particleB = Particle(1, 1, 1);

        let particleC = addParticles(particleA, particleB);
    }
}
