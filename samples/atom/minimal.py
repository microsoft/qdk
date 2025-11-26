# %% Import the necessary QDK modules and define the OpenQASM source code
from qdk import init, TargetProfile
from qdk.openqasm import compile, run
from qdk.simulation import NeutralAtomDevice, NoiseConfig
from qdk.widgets import Histogram

qasm_src = """include "stdgates.inc";
qubit[2] qs;
bit[2] r;

h qs[0];
cx qs[0], qs[1];
r = measure qs;
"""

# %% Initialize the QDK and compile the code
init(target_profile=TargetProfile.Base)
qir = compile(qasm_src)

# %% Create machine model and visualize execution
NeutralAtomDevice = NeutralAtomDevice()
NeutralAtomDevice.trace(qir)

# %% Configure a noise model and run a full-state simulation
noise = NoiseConfig()
noise.cz.set_depolarizing(0.05)
noise.sx.set_bitflip(0.01)
noise.mov.loss = 0.001
results = NeutralAtomDevice.simulate(qir, shots=1000, noise=noise, type="gpu")
Histogram(results, labels="kets")

# %%
