// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the MIT License. See LICENSE.txt in the project root for
// license information.

/**
 * @file simple_e2e.cpp
 * @brief End-to-end example demonstrating basic quantum chemistry calculations
 * using QDK/Chemistry
 *
 * This example demonstrates a complete workflow for molecular electronic
 * structure calculations:
 * 1. Loading molecular structures from XYZ files
 * 2. Performing Hartree-Fock (HF) self-consistent field (SCF) calculations
 * 3. Active space selection
 * 4. Hamiltonian construction in the molecular orbital basis
 * 5. Complete Active Space Configuration Interaction (CASCI)
 *
 * Usage:
 *   ./simple_e2e molecule.structure.xyz        # Loads XYZ file (STO-3G)
 *   ./simple_e2e molecule.structure.xyz 6-31g  # Sets the basis to 6-31G
 *
 * The program outputs:
 * - Input structure summary
 * - Hartree-Fock energy and orbital information
 * - CASCI energy and wavefunction information
 *
 * This example serves as a starting point for understanding QDK/Chemistry's
 * API and typical workflow for quantum chemistry calculations.
 */

// QDK/Chemistry Header Files
// One can also include <qdk/chemistry.hpp> to get all QDK/Chemistry components
#include <qdk/chemistry/algorithms/active_space.hpp>
#include <qdk/chemistry/algorithms/hamiltonian.hpp>
#include <qdk/chemistry/algorithms/mc.hpp>
#include <qdk/chemistry/algorithms/scf.hpp>
#include <qdk/chemistry/constants.hpp>
#include <qdk/chemistry/data/structure.hpp>
#include <qdk/chemistry/utils/valence_space.hpp>

// Standard Library Header Files
#include <iomanip>   // for std::setprecision
#include <iostream>  // for std::cout, std::endl

// Alias QDK/Chemistry namespaces for convenience
namespace data = qdk::chemistry::data;
namespace algorithms = qdk::chemistry::algorithms;
namespace utils = qdk::chemistry::utils;
namespace constants = qdk::chemistry::constants;

int main(int argc, char** argv) {
  // ==========================================================================
  // STEP 1: INPUT PARSING AND VALIDATION
  // ==========================================================================

  // Validate command line arguments
  if (argc < 2) {
    std::cout << "Usage: " << argv[0] << " <molecule.structure.xyz> [basis_set]"
              << std::endl;
    std::cout << "Example: " << argv[0] << " water.structure.xyz 6-31g"
              << std::endl;
    return 1;
  }

  // Parse molecular structure from XYZ file
  // See https://en.wikipedia.org/wiki/XYZ_file_format for details on the format
  auto structure = data::Structure::from_xyz_file(argv[1]);

  // Parse basis set from command line (default: STO-3G)
  std::string basis_set = (argc > 2) ? argv[2] : "sto-3g";

  // Display input information
  std::cout << "\n";
  std::cout << "========================================\n";
  std::cout << "              QDK/Chemistry             \n";
  std::cout << "========================================\n\n";

  std::cout << "Input Structure:\n";
  std::cout << "----------------\n";
  std::cout << structure->get_summary();
  std::cout << "Basis Set: " << basis_set << "\n\n";

  // ==========================================================================
  // STEP 2: SELF-CONSISTENT FIELD (SCF) CALCULATION
  //
  // SCF calculations provide the optimal single-determinant wavefunction
  // of the molecular system within the mean-field approximation. Key outputs
  // include the optimized molecular orbitals which form the single particle
  // basis for the Slater determinant, and the total, mean-field electronic
  // energy of the system.
  // ==========================================================================

  std::cout << "=========================================\n";
  std::cout << " SELF-CONSISTENT FIELD (SCF) CALCULATION  \n";
  std::cout << "=========================================\n\n";

  // Create a self-consistent field (SCF) solver using QDK/Chemistry as the
  // backend. The ScfSolverFactory can be invoked with no arguments for default
  // behaviour ("qdk")
  auto scf_solver = algorithms::ScfSolverFactory::create("qdk");

  // Configure the SCF solver
  // Different solvers support different settings - check documentation
  // or use ScfSolverFactory::available() to list available implementations
  // {
  //   Optional: You can configure additional settings here
  //   auto& settings = scf_solver->settings();
  //   settings.set("convergence_threshold", 1e-8);
  //   settings.set("max_iterations", 100);
  // }

  // Define the molecular system's charge and spin
  // - Charge: net electronic charge (0 = neutral)
  // - Spin multiplicity: 2S+1, where S is total spin
  //   (1 = singlet, 2 = doublet, 3 = triplet, etc.)
  int charge = 0;
  int spin_multiplicity = 1;

  // Run the SCF optimization
  // Returns the HF energy and wavefunction (single Slater determinant)
  std::cout << "Running SCF optimization...\n";
  auto [E_hf, wfn_hf] =
      scf_solver->run(structure, charge, spin_multiplicity, basis_set);

  // Display SCF results
  std::cout << "\nSCF Results:\n";
  std::cout << "---------------------\n";
  std::cout << "E(SCF) = " << std::scientific << std::setprecision(8) << E_hf
            << " Eh (Hartree)\n";

  // The optimized molecular orbitals can be accessed via
  // `Wavefunction::get_orbitals()`
  std::cout << "Number of orbitals: "
            << wfn_hf->get_orbitals()->get_num_molecular_orbitals() << "\n\n";

  // ==========================================================================
  // STEP 3: ACTIVE SPACE SELECTION
  //
  // Active space methods reduce computational cost of the many-body problem by
  // focusing on important (in this case, chemically relevant) orbitals and
  // electrons, rather than attempting to treat all orbitals and electrons
  // explicitly.
  //
  // QDK/Chemistry provides several active space selectors which select
  // active spaces using different heuristics. See
  // `ActiveSpaceSelectorFactory::available()` for a list of available
  // selectors.
  // ==========================================================================

  std::cout << "========================================\n";
  std::cout << "      ACTIVE SPACE SELECTION           \n";
  std::cout << "========================================\n\n";

  // Create an active space selector
  // This example uses a valence active space selector which selects
  // the active orbitals and electrons based on their proximity to the
  // frontier orbitals of the system (i.e. HOMO/LUMO).
  auto valence_active_space_selector =
      algorithms::ActiveSpaceSelectorFactory::create("qdk_valence");

  // Determine valence active space parameters
  // This is a conservative heuristic that identifies the largest
  // sensible valence active space for the system
  auto [num_active_electrons, num_active_orbitals] =
      utils::compute_valence_space_parameters(wfn_hf, charge);

  std::cout << "Active Space Parameters:\n";
  std::cout << "------------------------\n";
  std::cout << "Active electrons: " << num_active_electrons << "\n";
  std::cout << "Active orbitals: " << num_active_orbitals << "\n\n";

  // Configure the active space selector with computed parameters
  valence_active_space_selector->settings().set("num_active_electrons",
                                                (int)num_active_electrons);
  valence_active_space_selector->settings().set("num_active_orbitals",
                                                (int)num_active_orbitals);

  // Select the active orbitals
  // For valence selection, this returns a copy of SCF orbitals
  // with active space information populated (i.e. orbital coefficients are
  // unchanged).
  auto wfn_active = valence_active_space_selector->run(wfn_hf);

  // ==========================================================================
  // STEP 4: HAMILTONIAN CONSTRUCTION
  //
  // Given a set of molecular orbitals, we can construct the (second quantized)
  // electronic Hamiltonian in that basis. This Hamiltonian includes one- and
  // two-electron integrals transformed to the molecular orbital basis.
  //
  // QDK/Chemistry only provides one Hamiltonian constructor currently, using
  // default key = "".
  // ==========================================================================

  std::cout << "========================================\n";
  std::cout << "     HAMILTONIAN CONSTRUCTION           \n";
  std::cout << "========================================\n\n";

  // Construct the molecular Hamiltonian in the active orbital basis
  auto hamiltonian_constructor =
      algorithms::HamiltonianConstructorFactory::create();

  std::cout << "Building Hamiltonian in active orbital basis...\n";
  auto ham = hamiltonian_constructor->run(wfn_active->get_orbitals());
  std::cout << "Hamiltonian construction complete.\n\n";

  // ==========================================================================
  // STEP 5: MULTI-CONFIGURATION CALCULATION
  // ==========================================================================

  std::cout << "========================================\n";
  std::cout << "     MULTI-CONFIGURATION CALCULATION    \n";
  std::cout << "========================================\n\n";

  // Create a multi-configuration calculator
  // ASCI (Adaptive Sampling Configuration Interaction), provided by the MACIS
  // library, is an efficient method for selecting important configurations in
  // the active space, rather than considering the full configuration space.
  // QDK/Chemistry provides several multi-configuration calculator
  // implementations, which can be listed via
  // `MultiConfigurationCalculatorFactory::available()`.
  auto mc =
      algorithms::MultiConfigurationCalculatorFactory::create("macis_asci");

  // Configure the ASCI solver
  mc->settings().set("refine_energy_tol", 1e-3);

  // Run the ASCI calculation
  // For closed-shell singlets: n_alpha = n_beta = num_active_electrons/2
  const size_t n_alpha = num_active_electrons / 2;
  const size_t n_beta = n_alpha;
  std::cout << "Running ASCI calculation...\n";
  auto [E_cas, wfn_cas] = mc->run(ham, n_alpha, n_beta);

  // Display ASCI results
  std::cout << "\nASCI Results:\n";
  std::cout << "-------------\n";
  std::cout << "E(ASCI) = " << std::scientific << std::setprecision(8) << E_cas
            << " Eh\n";
  std::cout << "Wavefunction size: " << wfn_cas->size() << " determinants\n";

  // Calculate and display correlation energy
  double E_corr = E_cas - E_hf;
  std::cout << "Correlation energy: " << E_corr << " Eh\n\n";

  // ==========================================================================
  // SUMMARY
  // ==========================================================================

  std::cout << "========================================\n";
  std::cout << "              SUMMARY                   \n";
  std::cout << "========================================\n\n";

  std::cout << std::fixed << std::setprecision(6);
  std::cout << "Method       Energy (Eh)     Energy (kcal/mol)\n";
  std::cout << "------       -----------     -----------------\n";
  std::cout << "HF           " << std::setw(12) << E_hf << "    "
            << std::setw(12) << E_hf * constants::hartree_to_kcal_per_mol
            << "\n";
  std::cout << "ASCI         " << std::setw(12) << E_cas << "    "
            << std::setw(12) << E_cas * constants::hartree_to_kcal_per_mol
            << "\n";
  std::cout << "Correlation  " << std::setw(12) << E_corr << "    "
            << std::setw(12) << E_corr * constants::hartree_to_kcal_per_mol
            << "\n\n";

  std::cout << "Calculation completed successfully!\n\n";

  return 0;
}
