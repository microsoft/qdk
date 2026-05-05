The summary tells you the shape of the orbital space after localization:

- **Active Orbitals**: how many orbitals are in the active space, which directly determines problem size downstream
- **Virtual Orbitals**: orbitals outside the active space; a non-zero count means some were excluded
- **Has inactive space**: whether frozen-core orbitals exist below the active space

For **MP2 natural orbitals** specifically, the key signal is occupation number: orbitals with occupations near 1 (far from 0 or 2) are strongly correlated. In stretched N₂ you should see at least two orbitals with occupations around 1: these are the bonding/antibonding σ pair being broken. That's the multi-reference character that motivates the active space workflow in Chapter 3.

For **Pipek-Mezey and Foster-Boys**, the active space size and orbital count reflect how compactly the electrons are described — a smaller active space with the same physics means a cheaper quantum circuit.