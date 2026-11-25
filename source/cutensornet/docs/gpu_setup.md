# The GPU setup check: its terms and steps

The details behind the README's
[GPU setup check](../README.md#the-gpu-setup-check): the check's terms, and
each step it takes, how and why.

## The check's terms

The steps use these terms, which a user new to CUDA or to the crate meets
first here:

- **CUDA device:** as [the algebra's terms](engine.md#the-algebras-terms)
  define it. The **engine's device** is the one the algebra runs on.
- **Compute capability:** NVIDIA's version of a GPU's architecture,
  major.minor, such as 8.0 for an A100. It says which GPUs a library
  supports.
- **Setup guide:** what a library's module declares for the check
  ([`LibrarySetupGuide`](../src/libraries/setup_guide.rs)): what the check
  shows of the library and advises, and the oldest driver and device the
  library runs on.
- **Trial run:** the smallest GPU run of the engine, made by the check.

## The steps

The check takes them in this order.

| Step                                                                                             | How                                                                                                                                                                                                                                                                                                                                                                                                                                                                        | Why                                                                                                                                                                                                                                                                                                                                    |
| ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **The libraries** ([`discovery.rs`](../src/discovery.rs), [`libraries.rs`](../src/libraries.rs)) | `discover_with`, given the caller's `PackagedLibraries`. [`gpu_setup/advice.rs`](../src/gpu_setup/advice.rs) advises the fix for the first fault, an invalid override first. By the fault's kind, the fix is an install, with the packager's command and a restart of the process; a change of the override, when the copy at fault is the one it names; or access to a path the process cannot reach (its permissions, or a link that loops). It warns of each other copy | the check loads what a GPU run would load. An install displaces neither an override nor a copy already loaded in the process. Another copy can be why a library fails                                                                                                                                                                  |
| **The host** ([`libraries/cuda_runtime.rs`](../src/libraries/cuda_runtime.rs))                   | each library's setup guide reads its report for a problem of the host. Today only the CUDA Runtime's does: no NVIDIA driver, or one older than CUDA 12's floor ([`DRIVER_FLOOR`](../src/libraries/cuda_runtime.rs))                                                                                                                                                                                                                                                        | what a library needs of the host is the library's to say, so a library bound later brings its own check                                                                                                                                                                                                                                |
| **The devices** ([`gpu_setup/device_report.rs`](../src/gpu_setup/device_report.rs))              | how many CUDA devices the process sees, and the compute capability and memory of the engine's device, read through the loaded CUDA Runtime. It is a problem when they cannot be read, when there is no device, or when the engine's device is below the highest compute capability the setup guides require, today cuTensorNet's ([`libraries/cutensornet.rs`](../src/libraries/cutensornet.rs))                                                                           | each library says which devices it supports, so a library bound later brings its own floor. Free memory bounds the max bond dimension a run can reach. No other device is read, since reading one keeps a CUDA context on it for the life of the process                                                                               |
| **The trial run** ([`gpu_setup.rs`](../src/gpu_setup.rs))                                        | a `CudaMpsEngine` on the engine's device prepares two qubits, and applies a Hadamard to the first and a CNOT from it to the second. The second's probability of One must be 1/2 within [`TRIAL_RUN_TOLERANCE`](../src/gpu_setup.rs). A failure is shown with its causes. For a wrong value, the check advises a reinstall                                                                                                                                                  | the smallest run that makes cuTensorNet compute on the device, and checks what it computes. The engine applies a one-qubit gate and reads a probability on the host, so the CNOT's gate split is what runs there. A device the engine cannot use, or libraries that compute wrong results, are found here, not in the user's first run |
