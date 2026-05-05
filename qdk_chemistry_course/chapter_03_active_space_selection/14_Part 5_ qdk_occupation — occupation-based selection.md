<h2 style="color:#D30982;">Part 5: qdk_occupation — occupation-based selection</h2>

`qdk_occupation` selects orbitals based on their **natural orbital occupation numbers** from the 1-RDM. In a strongly correlated system, orbitals with occupations near 1 (far from the fully-occupied value of 2 or fully-empty value of 0) are partially filled, and are a direct indicator of correlation.

This is complementary to entropy: entropy measures *entanglement* between an orbital and the rest; occupation numbers measure the orbital's *partial filling*. For stretched N₂, the σ-bonding and σ*-antibonding orbitals should both show occupation near 1. Like the autoCAS methods, it reads from the 1-RDM and so takes the post-HF wavefunction from Part 4.

Now, let's apply `qdk_occupation` to `wfn_sci`. Inspect the threshold setting, then compare to the autoCAS result.