# Setup: imports and molecule path
from pathlib import Path
import numpy as np

import qdk_chemistry.plugins.pyscf  # Required for scf_solver

from qdk_chemistry.data import Structure
from qdk_chemistry.algorithms import available, create, print_settings
from qdk_chemistry.constants import ANGSTROM_TO_BOHR
from qdk_chemistry.utils import Logger

Logger.set_global_level(Logger.LogLevel.off)

N2_XYZ = Path("../examples/data/stretched_n2.structure.xyz")