# Code for directly running the GPU parallel shot simulator

import time
from pathlib import Path

from qsharp import init, eval, compile, TargetProfile, run
from qsharp._simulation import run_qir_gpu, NoiseConfig
from qsharp._device._atom import AC1000

decompose = True
shots = 100
circuit = "grover"
gpu_sim = "parallel"
run_sparse = False

noise = NoiseConfig()
noise.sx.set_depolarizing(0.001)

init(target_profile=TargetProfile.Base)

grover_path = Path(__file__).parent / "GroverBase.qs"
src_grover = grover_path.read_text()
src_ccx = """
operation Main() : Result[] {
    use qs = Qubit[3];
    H(qs[0]);
    CX(qs[0], qs[1]);
    CCNOT(qs[0], qs[1], qs[2]);
    MResetEachZ(qs)
}
"""

eval(src_grover if circuit == "grover" else src_ccx)
qir = compile("Main()")

device = AC1000()
ac1000_qir = device.compile(qir) if decompose == True else qir

# Get a (rought) count of the gates
gate_count = ac1000_qir._ll_str.count("\n") + 1


start = time.time()
results = run_qir_gpu(ac1000_qir._ll_str, shots=shots, noise=noise, sim=gpu_sim)
end = time.time()

print(f"Ran {shots} shots of {gate_count} gates")
print("First 10 results:" + str(results[:10]))
print(f"GPU parallel shot simulation took {end - start:.2f} seconds")

if run_sparse:
    print("\nRunning sparse simulator...")
    start = time.time()
    sparse_results = run("Main()", shots=shots, noise=(0.01, 0.01, 0.01))
    end = time.time()

    print(f"First 10 results:" + str(sparse_results[:10]))
    print(f"Sparse simulation took {end - start:.2f} seconds")
