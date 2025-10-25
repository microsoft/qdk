# Code for directly running the GPU parallel shot simulator

from qsharp import init, eval, compile, TargetProfile
from qsharp._simulation import run_qir_gpu, NoiseConfig

init(target_profile=TargetProfile.Base)

src = """
operation Main() : Result[] {
    use q = Qubit[2];
    H(q[0]);
    CNOT(q[0], q[1]);
    MResetEachZ(q)
}
"""

eval(src)
qir = compile("Main()")

noise = NoiseConfig()

results = run_qir_gpu(qir._ll_str, shots=10, noise=noise, sim="parallel")
print(results)
