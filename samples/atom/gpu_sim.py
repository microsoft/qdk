# Code for directly running the GPU parallel shot simulator

from qsharp import init, eval, compile, TargetProfile
from qsharp._simulation import run_qir_gpu, NoiseConfig

from qsharp._device._atom import AC1000

import time
from pathlib import Path

init(target_profile=TargetProfile.Base)

grover_path = Path(__file__).parent / "GroverBase.qs"
src_grover = grover_path.read_text()

src_bell = """
operation Main() : Result[] {
    use q = Qubit[2];
    H(q[0]);
    CNOT(q[0], q[1]);
    MResetEachZ(q)
}
"""

eval(src_grover)
qir = compile("Main()")

device = AC1000()
ac1000_qir = device.compile(qir)

noise = NoiseConfig()

# Time the next instruction and report
start = time.time()
results = run_qir_gpu(ac1000_qir._ll_str, shots=100, noise=noise, sim="parallel")
# results = run_qir_gpu(ac1000_qir._ll_str, shots=100, noise=noise)
end = time.time()
print(f"GPU parallel shot simulation took {end - start:.2f} seconds")
print(len(results))
