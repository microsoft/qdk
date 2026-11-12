// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! Development branch only (not for the PR): `tensornet`'s conformance kit
//! on the engine over the real cuTensorNet, on CUDA device 0.
//!
//! It needs an NVIDIA GPU, the CUDA Runtime 12.9 and cuTensorNet 2.13, so it
//! is ignored by default; run it with `--ignored` on such a host. The host
//! tests run the same kit on the engine over the test double.

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
#[ignore = "requires an NVIDIA GPU with the audited CUDA Runtime 12.9 and cuTensorNet 2.13"]
fn the_engine_on_the_gpu_conforms_to_the_kit() {
    let availability = qdk_cutensornet::discover().expect("the audited libraries are installed");
    println!("{:#?}", availability.report());
    let engine =
        qdk_cutensornet::CuTensorNetEngine::new(&availability).expect("an engine on CUDA device 0");
    let failures = tensornet::conformance::check(&engine);
    assert!(failures.is_empty(), "{failures:#?}");
}
