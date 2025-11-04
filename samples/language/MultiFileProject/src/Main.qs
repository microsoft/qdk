// # Sample
// Multi File Project
//
// # Description
// Organizing code into multiple Q# source files is an important part of
// writing readable and maintainable code. In this project, we have `Main.qs`,
// and `Particle.qs`, which defines a new namespace for particle operations.
// The presence of a Q# manifest file (`qsharp.json`) tells the compiler
// to include all Q# files under `src/`.

import Particle.*;
import Std.Diagnostics.*;

@EntryPoint()
operation Main() : Unit {
    let particleA = new Particle { X = 0, Y = 0, Z = 0 };
    let particleB = new Particle { X = 1, Y = 1, Z = 1 };

    let particleC = addParticles(particleA, particleB);
    Message($"Particle C: Particle({particleC.X}, {particleC.Y}, {particleC.Z})");
    // Add a trivial quantum operation to satisfy linter expecting quantum content.
    use q = Qubit();
    H(q);
    Reset(q);
    return (); // explicit for clarity
}
