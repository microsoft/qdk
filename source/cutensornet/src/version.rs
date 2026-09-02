use crate::AvailabilityError;

pub(crate) const POLICY: VersionPolicy = VersionPolicy {
    cutensornet_runtime: 21_300,
    cuda_runtime: 12_090,
};

pub(crate) struct VersionPolicy {
    cutensornet_runtime: usize,
    cuda_runtime: i32,
}

impl VersionPolicy {
    pub(crate) fn validate_cutensornet(&self, found: usize) -> Result<(), AvailabilityError> {
        if found == self.cutensornet_runtime {
            Ok(())
        } else {
            Err(AvailabilityError::UnsupportedVersion {
                component: "cuTensorNet runtime",
                found: found as u64,
                supported: "21300",
            })
        }
    }

    pub(crate) fn validate_cuda_runtime(&self, found: i32) -> Result<(), AvailabilityError> {
        if found == self.cuda_runtime {
            Ok(())
        } else {
            Err(AvailabilityError::UnsupportedVersion {
                component: "CUDA Runtime",
                found: u64::try_from(found).unwrap_or_default(),
                supported: "12090",
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::POLICY;
    use crate::AvailabilityError;

    #[test]
    fn accepts_only_audited_cutensornet_runtime() {
        assert!(POLICY.validate_cutensornet(21_300).is_ok());
        assert!(matches!(
            POLICY.validate_cutensornet(21_400),
            Err(AvailabilityError::UnsupportedVersion {
                component: "cuTensorNet runtime",
                found: 21_400,
                supported: "21300"
            })
        ));
    }

    #[test]
    fn accepts_only_audited_cuda_runtime() {
        assert!(POLICY.validate_cuda_runtime(12_090).is_ok());
        assert!(matches!(
            POLICY.validate_cuda_runtime(13_000),
            Err(AvailabilityError::UnsupportedVersion {
                component: "CUDA Runtime",
                found: 13_000,
                supported: "12090"
            })
        ));
    }
}
