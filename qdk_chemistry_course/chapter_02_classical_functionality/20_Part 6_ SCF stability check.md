<h2 style="color:#D30982;">Part 6: SCF stability check</h2>

For some molecules, the Hartree-Fock wavefunction is not the true variational minimum because the SCF procedure can converge to a saddle point rather than a ground state. The stability checker tests whether the converged solution is stable against orbital rotations.

This is especially relevant for stretched bonds and open-shell systems, where restricted HF often yields an unstable solution. For stretched N₂, this check is important: broken-symmetry solutions may exist.

> **Stability instability vs convergence failure**: these are distinct problems.
> - *Convergence failure*: SCF never reaches a self-consistent solution — the iterations don't settle. Fix by tightening thresholds, changing the initial guess, or switching the SCF algorithm.
> - *Stability instability*: SCF converged, but to a saddle point — a lower-energy solution exists via an orbital rotation. The stability checker diagnoses this *after* a successful convergence.
> Stretched N₂ is a canonical example of the second case: SCF converges cleanly, but the restricted solution is not the ground state.

In the cell below, run the stability checker on the HF wavefunction. Is it stable? What does the checker return?