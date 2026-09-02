use qdk_cutensornet::discover;

#[test]
#[ignore = "requires the audited CUDA Runtime 12.9 and cuTensorNet 2.13 libraries"]
fn discovers_audited_native_libraries_without_gpu_work() {
    let availability = discover().expect("audited native libraries should be discoverable");
    let report = availability.report();

    assert_eq!(report.cutensornet_version, 21_300);
    assert_eq!(report.cutensornet_cuda_runtime_version, 12_090);
    assert_eq!(report.cuda_runtime_version, 12_090);
    println!("{report:#?}");
}
