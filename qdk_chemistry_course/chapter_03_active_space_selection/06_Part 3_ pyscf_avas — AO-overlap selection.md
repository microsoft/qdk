<h2 style="color:#D30982;">Part 3: pyscf_avas — AO-overlap selection</h2>

`pyscf_avas` (<a href="https://pubs.acs.org/doi/10.1021/acs.jctc.7b00128" target="_blank">Sayfutyarova et al., 2017</a>) (Atomic Valence Active Space) encodes prior chemical knowledge explicitly: you specify which atomic orbital character the active space should contain (e.g., `["N 2s", "N 2p"]`), and AVAS projects those onto the molecular orbital basis, retaining the MOs with the largest overlap weight.

This makes AVAS the right choice when you already know which bonds or lone pairs drive the chemistry. It translates that knowledge directly into a selection criterion. For N₂ dissociation, including both 2s and 2p captures the σ and π bonds. Like `qdk_valence`, AVAS works directly from the HF wavefunction and no multi-configuration step needed.

Fill in the missing AO label in the cell below and then answer the following question. 