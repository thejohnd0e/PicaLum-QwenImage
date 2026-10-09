//! GPU detection through nvidia-smi (ships with every NVIDIA driver).

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    pub uuid: String,
    pub pci_bus_id: String,
    pub name: String,
    pub total_mb: u32,
    pub used_mb: u32,
    pub util: u32,
    pub driver: String,
}

impl Gpu {
    pub fn driver_major(&self) -> u32 {
        self.driver
            .split('.')
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    }
    pub fn short_name(&self) -> String {
        self.name
            .replace("NVIDIA GeForce ", "")
            .replace("NVIDIA ", "")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuDetectError {
    CommandUnavailable(String),
    CommandFailed(String),
    MalformedRow { line: usize, message: String },
}

impl fmt::Display for GpuDetectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandUnavailable(message) => {
                write!(f, "could not run nvidia-smi: {message}")
            }
            Self::CommandFailed(message) => write!(f, "nvidia-smi failed: {message}"),
            Self::MalformedRow { line, message } => {
                write!(f, "malformed nvidia-smi row {line}: {message}")
            }
        }
    }
}

impl std::error::Error for GpuDetectError {}

pub fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Every NVIDIA GPU reported by nvidia-smi, in its reported order.
pub fn detect_all() -> Result<Vec<Gpu>, GpuDetectError> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=uuid,pci.bus_id,name,memory.total,memory.used,utilization.gpu,driver_version",
        "--format=csv,noheader,nounits",
    ]);
    let output = no_window(&mut cmd)
        .output()
        .map_err(|error| GpuDetectError::CommandUnavailable(error.to_string()))?;

    parse_command_result(
        output.status.success(),
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
    )
}

/// The selected NVIDIA GPU, or the first reported GPU when no UUID is supplied.
pub fn detect_selected(uuid: Option<&str>) -> Option<Gpu> {
    let gpus = detect_all().ok()?;
    select_by_uuid(gpus, uuid)
}

/// Selects telemetry by stable UUID; absent UUID falls back to first GPU for legacy callers.
fn select_by_uuid(gpus: Vec<Gpu>, uuid: Option<&str>) -> Option<Gpu> {
    match uuid {
        Some(uuid) => gpus.into_iter().find(|gpu| gpu.uuid == uuid),
        None => gpus.into_iter().next(),
    }
}
/// First NVIDIA GPU, or None if there is no NVIDIA driver or valid result.
pub fn detect() -> Option<Gpu> {
    detect_selected(None)
}

fn parse_command_result(
    success: bool,
    stdout: &str,
    stderr: &str,
) -> Result<Vec<Gpu>, GpuDetectError> {
    if !success {
        return Err(GpuDetectError::CommandFailed(stderr.trim().to_string()));
    }

    stdout
        .lines()
        .enumerate()
        .filter(|(_, row)| !row.trim().is_empty())
        .map(|(index, row)| parse_row(index + 1, row))
        .collect()
}

fn parse_row(line: usize, row: &str) -> Result<Gpu, GpuDetectError> {
    let fields: Vec<&str> = row.split(',').map(str::trim).collect();
    if fields.len() != 7 {
        return malformed(line, format!("expected 7 fields, got {}", fields.len()));
    }

    for (index, field) in fields.iter().enumerate() {
        if field.is_empty() {
            return malformed(line, format!("field {} is empty", index + 1));
        }
    }

    let total_mb = parse_u32(line, "memory.total", fields[3])?;
    let used_mb = parse_u32(line, "memory.used", fields[4])?;
    let util = parse_u32(line, "utilization.gpu", fields[5])?;
    if fields[6]
        .split('.')
        .next()
        .and_then(|major| major.parse::<u32>().ok())
        .is_none()
    {
        return malformed(line, "driver_version is not numeric".to_string());
    }

    Ok(Gpu {
        uuid: fields[0].to_string(),
        pci_bus_id: fields[1].to_string(),
        name: fields[2].to_string(),
        total_mb,
        used_mb,
        util,
        driver: fields[6].to_string(),
    })
}

fn parse_u32(line: usize, field: &str, value: &str) -> Result<u32, GpuDetectError> {
    value.parse().map_err(|_| GpuDetectError::MalformedRow {
        line,
        message: format!("{field} is not an unsigned integer"),
    })
}

fn malformed<T>(line: usize, message: String) -> Result<T, GpuDetectError> {
    Err(GpuDetectError::MalformedRow { line, message })
}

#[cfg(test)]
mod tests {
    use super::{parse_command_result, Gpu, GpuDetectError};

    const TWO_GPUS: &str = " GPU-aaaaaaaa , 00000000:01:00.0 , NVIDIA GeForce RTX 3060 , 12288 , 1611 , 3 , 610.88 \nGPU-bbbbbbbb, 00000000:02:00.0, NVIDIA RTX A4000, 16384, 42, 0, 610.88\n";

    #[test]
    fn parses_every_nvidia_smi_row_with_stable_identity() {
        let gpus = parse_command_result(true, TWO_GPUS, "").unwrap();

        assert_eq!(gpus.len(), 2);
        assert_eq!(gpus[0].uuid, "GPU-aaaaaaaa");
        assert_eq!(gpus[0].pci_bus_id, "00000000:01:00.0");
        assert_eq!(gpus[0].name, "NVIDIA GeForce RTX 3060");
        assert_eq!(gpus[0].total_mb, 12_288);
        assert_eq!(gpus[0].used_mb, 1_611);
        assert_eq!(gpus[0].util, 3);
        assert_eq!(gpus[0].driver, "610.88");
        assert_eq!(gpus[1].uuid, "GPU-bbbbbbbb");
        assert_eq!(gpus[1].pci_bus_id, "00000000:02:00.0");
    }

    #[test]
    fn accepts_whitespace_and_a_final_newline() {
        let gpus = parse_command_result(
            true,
            "  GPU-a , 00000000:01:00.0 , NVIDIA RTX A4000 , 16384 , 0 , 7 , 580.01  \n",
            "",
        )
        .unwrap();

        assert_eq!(gpus[0].uuid, "GPU-a");
        assert_eq!(gpus[0].util, 7);
    }

    #[test]
    fn rejects_malformed_required_fields() {
        let malformed = [
            "GPU-a, 00000000:01:00.0, RTX 3060, 12288, 0, 580.01",
            "GPU-a, 00000000:01:00.0, RTX 3060, twelve, 0, 0, 580.01",
            " , 00000000:01:00.0, RTX 3060, 12288, 0, 0, 580.01",
            "GPU-a, , RTX 3060, 12288, 0, 0, 580.01",
            "GPU-a, 00000000:01:00.0, RTX 3060, 12288, 0, 0, current",
        ];

        for row in malformed {
            assert!(matches!(
                parse_command_result(true, row, ""),
                Err(GpuDetectError::MalformedRow { .. })
            ));
        }
    }

    #[test]
    fn reports_nonzero_nvidia_smi_exit() {
        let result = parse_command_result(false, "", "NVIDIA-SMI has failed");

        assert_eq!(
            result.unwrap_err(),
            GpuDetectError::CommandFailed("NVIDIA-SMI has failed".to_string())
        );
    }

    #[test]
    fn empty_output_means_zero_gpus() {
        assert!(parse_command_result(true, "\n  \n", "").unwrap().is_empty());
    }

    #[test]
    fn gpu_identity_survives_serialization() {
        let gpu = parse_command_result(true, TWO_GPUS, "").unwrap().remove(0);
        let json = serde_json::to_string(&gpu).unwrap();
        let restored: Gpu = serde_json::from_str(&json).unwrap();

        assert_eq!(restored, gpu);
    }

    #[test]
    fn keeps_existing_gpu_helpers() {
        let gpu = parse_command_result(true, TWO_GPUS, "").unwrap().remove(0);

        assert_eq!(gpu.driver_major(), 610);
        assert_eq!(gpu.short_name(), "RTX 3060");
    }
}

#[cfg(test)]
mod task4_selection_tests {
    use super::{select_by_uuid, Gpu};

    fn gpu(uuid: &str, used_mb: u32) -> Gpu {
        Gpu {
            uuid: uuid.into(),
            pci_bus_id: "0000:01:00.0".into(),
            name: uuid.into(),
            total_mb: 12_000,
            used_mb,
            util: 0,
            driver: "610.88".into(),
        }
    }

    #[test]
    fn telemetry_uses_selected_uuid_not_display_order() {
        let selected = select_by_uuid(
            vec![gpu("GPU-first", 10), gpu("GPU-selected", 20)],
            Some("GPU-selected"),
        )
        .unwrap();
        assert_eq!(selected.uuid, "GPU-selected");
        assert_eq!(selected.used_mb, 20);
    }

    #[test]
    fn missing_selected_uuid_is_not_silently_remapped() {
        assert!(select_by_uuid(vec![gpu("GPU-first", 10)], Some("GPU-missing")).is_none());
    }
}
