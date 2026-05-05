<h2 style="color:#D30982;">Part 3: Scaling with number of determinants</h2>

Real chemistry wavefunctions have many determinants. The SCI result for N₂ has 3136 — requiring a depth-194,000 circuit if prepared exactly. In practice, the dominant determinants carry most of the wavefunction weight: keeping only the top-10 reduces depth to ~314 with minimal state error.

Depth grows roughly linearly with determinant count for `sparse_isometry_gf2x`: each additional determinant adds one binary column to the amplitude matrix that GF2X must synthesize into CNOT+X gates. Truncation is the primary lever for controlling hardware cost — at the expense of a small approximation in the trial state.

The `truncate_wavefunction` utility sorts determinants by |coefficient| and keeps the top-N, renormalized. Run the cell to see how depth scales.