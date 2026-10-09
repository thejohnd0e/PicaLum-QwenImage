use serde::{Deserialize, Serialize};

use crate::{
    config::{Profile, MIN_DRIVER_MAJOR},
    gpu::Gpu,
};

/// A capability result obtained from a reliable probe or reviewed device policy.
/// It is never inferred from the GPU marketing name.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GpuCapability {
    pub major: u32,
    pub minor: u32,
    pub compatible: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum GpuSupportReason {
    UnknownCapability,
    InsufficientVram {
        minimum_mb: u32,
        actual_mb: u32,
    },
    DriverTooOld {
        minimum_major: u32,
        actual_major: u32,
    },
    IncompatibleCapability {
        major: u32,
        minor: u32,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GpuRecommendation {
    pub supported: bool,
    pub profile: Option<Profile>,
    pub reason: Option<GpuSupportReason>,
}

/// Recommends a profile using decimal product thresholds: 10,000 and 12,000 MB.
pub fn recommend(gpu: &Gpu, capability: Option<&GpuCapability>) -> GpuRecommendation {
    let driver_major = gpu.driver_major();
    if driver_major < MIN_DRIVER_MAJOR {
        return rejected(GpuSupportReason::DriverTooOld {
            minimum_major: MIN_DRIVER_MAJOR,
            actual_major: driver_major,
        });
    }

    if capability.is_none() {
        return rejected(GpuSupportReason::UnknownCapability);
    }

    if let Some(capability) = capability.filter(|capability| !capability.compatible) {
        return rejected(GpuSupportReason::IncompatibleCapability {
            major: capability.major,
            minor: capability.minor,
        });
    }

    match by_vram(gpu) {
        Ok(profile) => GpuRecommendation {
            supported: true,
            profile: Some(profile),
            reason: None,
        },
        Err(reason) => rejected(reason),
    }
}

/// Driver and VRAM eligibility only. `supported: true` here means the device may receive
/// prerequisites (Python/PyTorch) and be probed; it is NOT proof of architecture support.
pub fn recommend_pending(gpu: &Gpu) -> GpuRecommendation {
    let driver_major = gpu.driver_major();
    if driver_major < MIN_DRIVER_MAJOR {
        return rejected(GpuSupportReason::DriverTooOld {
            minimum_major: MIN_DRIVER_MAJOR,
            actual_major: driver_major,
        });
    }
    match by_vram(gpu) {
        Ok(profile) => GpuRecommendation {
            supported: true,
            profile: Some(profile),
            reason: None,
        },
        Err(reason) => rejected(reason),
    }
}

fn by_vram(gpu: &Gpu) -> Result<Profile, GpuSupportReason> {
    if gpu.total_mb < 10_000 {
        Err(GpuSupportReason::InsufficientVram {
            minimum_mb: 10_000,
            actual_mb: gpu.total_mb,
        })
    } else if gpu.total_mb < 12_000 {
        Ok(Profile::Compact)
    } else {
        Ok(Profile::Balanced)
    }
}

fn rejected(reason: GpuSupportReason) -> GpuRecommendation {
    GpuRecommendation {
        supported: false,
        profile: None,
        reason: Some(reason),
    }
}

#[cfg(test)]
mod tests {
    use super::{recommend, GpuCapability, GpuSupportReason};
    use crate::{gpu::Gpu, Profile};

    fn gpu(total_mb: u32) -> Gpu {
        Gpu {
            uuid: "GPU-aaaaaaaa".to_string(),
            pci_bus_id: "00000000:01:00.0".to_string(),
            name: "NVIDIA GeForce RTX 3060".to_string(),
            total_mb,
            used_mb: 0,
            util: 0,
            driver: "610.88".to_string(),
        }
    }

    #[test]
    fn applies_decimal_vram_boundaries() {
        let cases = [
            (9_999, false, None),
            (10_000, true, Some(Profile::Compact)),
            (11_999, true, Some(Profile::Compact)),
            (12_000, true, Some(Profile::Balanced)),
        ];

        for (total_mb, supported, profile) in cases {
            let recommendation = recommend(
                &gpu(total_mb),
                Some(&GpuCapability {
                    major: 8,
                    minor: 6,
                    compatible: true,
                }),
            );
            assert_eq!(recommendation.supported, supported, "{total_mb} MB");
            assert_eq!(recommendation.profile, profile, "{total_mb} MB");
        }
    }

    #[test]
    fn rejects_vram_below_product_floor() {
        let recommendation = recommend(
            &gpu(9_999),
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        );

        assert_eq!(
            recommendation.reason,
            Some(GpuSupportReason::InsufficientVram {
                minimum_mb: 10_000,
                actual_mb: 9_999,
            })
        );
    }

    #[test]
    fn rejects_old_driver_independently_of_vram() {
        let mut old_driver_gpu = gpu(24_000);
        old_driver_gpu.driver = "579.99".to_string();

        let recommendation = recommend(
            &old_driver_gpu,
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        );

        assert_eq!(recommendation.profile, None);
        assert_eq!(
            recommendation.reason,
            Some(GpuSupportReason::DriverTooOld {
                minimum_major: 580,
                actual_major: 579,
            })
        );
    }

    #[test]
    fn rejects_explicitly_incompatible_capability() {
        let capability = GpuCapability {
            major: 6,
            minor: 1,
            compatible: false,
        };

        let recommendation = recommend(&gpu(24_000), Some(&capability));

        assert_eq!(recommendation.profile, None);
        assert_eq!(
            recommendation.reason,
            Some(GpuSupportReason::IncompatibleCapability { major: 6, minor: 1 })
        );
    }

    #[test]
    fn accepts_explicitly_compatible_capability() {
        let capability = GpuCapability {
            major: 8,
            minor: 6,
            compatible: true,
        };

        let recommendation = recommend(&gpu(12_000), Some(&capability));

        assert!(recommendation.supported);
        assert_eq!(recommendation.profile, Some(Profile::Balanced));
        assert_eq!(recommendation.reason, None);
    }

    #[test]
    fn recommendation_uses_total_not_currently_free_vram() {
        let mut busy_gpu = gpu(10_000);
        busy_gpu.used_mb = 9_900;

        let recommendation = recommend(
            &busy_gpu,
            Some(&GpuCapability {
                major: 8,
                minor: 6,
                compatible: true,
            }),
        );

        assert!(recommendation.supported);
        assert_eq!(recommendation.profile, Some(Profile::Compact));
    }
}

#[cfg(test)]
mod pending_tests {
    use super::{recommend_pending, GpuSupportReason};
    use crate::{gpu::Gpu, Profile};

    fn gpu(total_mb: u32, driver: &str) -> Gpu {
        Gpu {
            uuid: "GPU-p".into(),
            pci_bus_id: "0000:01:00.0".into(),
            name: "NVIDIA GPU".into(),
            total_mb,
            used_mb: 0,
            util: 0,
            driver: driver.into(),
        }
    }

    #[test]
    fn pending_applies_vram_thresholds_without_capability() {
        assert_eq!(
            recommend_pending(&gpu(10_000, "610.1")).profile,
            Some(Profile::Compact)
        );
        assert_eq!(
            recommend_pending(&gpu(12_000, "610.1")).profile,
            Some(Profile::Balanced)
        );
        assert_eq!(
            recommend_pending(&gpu(9_999, "610.1")).reason,
            Some(GpuSupportReason::InsufficientVram {
                minimum_mb: 10_000,
                actual_mb: 9_999
            })
        );
    }

    #[test]
    fn pending_still_rejects_old_driver() {
        assert!(matches!(
            recommend_pending(&gpu(24_000, "579.9")).reason,
            Some(GpuSupportReason::DriverTooOld { .. })
        ));
    }
}

#[cfg(test)]
mod unknown_capability_tests {
    use super::{recommend, GpuSupportReason};
    use crate::gpu::Gpu;

    #[test]
    fn unknown_capability_is_blocked_instead_of_supported() {
        let gpu = Gpu {
            uuid: "GPU-unknown".into(),
            pci_bus_id: "0000:01:00.0".into(),
            name: "NVIDIA GPU".into(),
            total_mb: 12_000,
            used_mb: 0,
            util: 0,
            driver: "610.88".into(),
        };
        let result = recommend(&gpu, None);
        assert!(!result.supported);
        assert_eq!(result.reason, Some(GpuSupportReason::UnknownCapability));
    }
}
