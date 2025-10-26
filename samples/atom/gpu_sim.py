# Code for directly running the GPU parallel shot simulator

import time
from pathlib import Path

from qsharp import init, eval, compile, TargetProfile
from qsharp._simulation import run_qir_gpu, NoiseConfig
from qsharp._device._atom import AC1000


init(target_profile=TargetProfile.Base)

grover_path = Path(__file__).parent / "GroverBase.qs"
src_grover = grover_path.read_text()

eval(src_grover)
qir = compile("Main()")

device = AC1000()
ac1000_qir = device.compile(qir)

# Get a (rought) count of the gates
gate_count = ac1000_qir._ll_str.count("\n") + 1

noise = NoiseConfig()
shots = 100

start = time.time()
results = run_qir_gpu(ac1000_qir._ll_str, shots=shots, noise=noise, sim="parallel")
# results = run_qir_gpu(ac1000_qir._ll_str, shots=shots, noise=noise)
end = time.time()

print(f"Result count: {len(results)} from {gate_count} gates")
print("First 10 results:" + str(results[:10]))
print(f"GPU parallel shot simulation took {end - start:.2f} seconds")
