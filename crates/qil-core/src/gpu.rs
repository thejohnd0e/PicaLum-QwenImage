//! GPU detection through nvidia-smi (ships with every NVIDIA driver).

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
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

pub fn no_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// First NVIDIA GPU, or None if there is no NVIDIA driver.
pub fn detect() -> Option<Gpu> {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=name,memory.total,memory.used,utilization.gpu,driver_version",
        "--format=csv,noheader,nounits",
    ]);
    let out = no_window(&mut cmd).output().ok()?;
    parse(&String::from_utf8_lossy(&out.stdout))
}

fn parse(text: &str) -> Option<Gpu> {
    let line = text.lines().next()?;
    let f: Vec<&str> = line.split(',').map(str::trim).collect();
    Some(Gpu {
        name: f.first()?.to_string(),
        total_mb: f.get(1)?.parse().ok()?,
        used_mb: f.get(2)?.parse().ok()?,
        util: f.get(3)?.parse().unwrap_or(0),
        driver: f.get(4)?.to_string(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_nvidia_smi_line() {
        let g = super::parse("NVIDIA GeForce RTX 3060, 12288, 1611, 3, 610.88\n").unwrap();
        assert_eq!(g.total_mb, 12288);
        assert_eq!(g.driver_major(), 610);
        assert_eq!(g.short_name(), "RTX 3060");
    }
}
