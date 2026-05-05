# Run the stability checker
stability_checker = create("stability_checker", "pyscf")
is_stable, stability_result = stability_checker.run(wfn_hf)

print(f"Overall stable:   {is_stable}")
print(f"Internal stable:  {stability_result.is_internal_stable()}")
print(f"External stable:  {stability_result.is_external_stable()}")
print(f"\nInternal eigenvalues: {stability_result.get_internal_eigenvalues()}")
print(f"External eigenvalues: {stability_result.get_external_eigenvalues()}")

# Interpreting the result:
# - Internal instability: a lower-energy solution exists within the same spin symmetry
# - External instability (RHF → UHF): a broken-symmetry unrestricted solution exists
#   This is common for stretched bonds — N₂ at 1.27 Å is expected to be externally unstable
#   because restricted HF cannot describe bond breaking correctly

# What to do when unstable:
# An external instability confirms that single-reference HF is an inadequate description.
# The correct response is NOT to re-run SCF, but to move to a multi-reference treatment —
# exactly what the active space + CASCI workflow in Chapters 3-4 provides.
# The localized MP2 natural orbital wavefunction (wfn_mp2) is the right starting point.
if not is_stable:
    print("\n→ Wavefunction is unstable. Proceed with multi-reference active space workflow (Chapter 3).")
else:
    print("\n→ Wavefunction is stable. Single-reference treatment may be adequate.")