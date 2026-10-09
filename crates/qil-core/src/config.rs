//! Configuration, paths and VRAM profiles.
//!
//! Everything the app installs lives under one runtime folder (default
//! `%LOCALAPPDATA%\picalum-qwenimage`) so uninstalling is deleting a folder.
//! The models folder can live elsewhere (another disk, or weights you already have).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub const APP_ID: &str = "picalum-qwenimage";
pub const CURRENT_CONFIG_VERSION: u32 = 1;
/// ComfyUI commit this release was tested against.
pub const COMFY_COMMIT: &str = "3b4c0b0e457cf0a51cf3038e0a6750d8f96ce251";
pub const UV_VERSION: &str = "0.12.18";
pub const TORCH_INDEX: &str = "https://download.pytorch.org/whl/cu130";
/// CUDA 13.0 wheels need an R580+ driver.
pub const MIN_DRIVER_MAJOR: u32 = 580;
pub const HF_BASE: &str = "https://huggingface.co/Comfy-Org/Qwen-Image-2.1/resolve/main";

#[derive(Debug, Clone, Copy)]
pub struct ModelFile {
    pub folder: &'static str, // ComfyUI model folder
    pub name: &'static str,
    pub size: u64,
    pub sha256: &'static str,
}

impl ModelFile {
    pub fn url(&self) -> String {
        format!("{HF_BASE}/{}/{}", self.folder, self.name)
    }
    pub fn path(&self, models_dir: &Path) -> PathBuf {
        models_dir.join(self.folder).join(self.name)
    }
}

pub const DIT: ModelFile = ModelFile {
    folder: "diffusion_models",
    name: "qwen_image_2.1_int8_convrot.safetensors",
    size: 7_256_783_064,
    sha256: "cb74113cb03faecd79611b01fd7fd642f0aa60d6f0b95086abee214d75eaa57d",
};
pub const TE_INT8: ModelFile = ModelFile {
    folder: "text_encoders",
    name: "qwen3vl_8b_int8_convrot.safetensors",
    size: 9_350_798_360,
    sha256: "8bfd0f6e12abf2d2d697ecc888e5e90b0d6741d6708f05799f53afa560452e8f",
};
pub const TE_W4A8: ModelFile = ModelFile {
    folder: "text_encoders",
    name: "qwen3vl_8b_w4a8.safetensors",
    size: 6_312_105_364,
    sha256: "7754425e55e7bea2bfde4dde59a4cc236cb44e5ee9c215ea66ef8d47012824eb",
};
pub const VAE: ModelFile = ModelFile {
    folder: "vae",
    name: "qwen_image_2.1_vae_bf16.safetensors",
    size: 675_509_688,
    sha256: "bb21f7473051e1ac368515dd3f2e15cd44d7a11748ee8823e1ddca3e4876b7c9",
};

/// Which text encoder to use. The DiT and VAE are the same for every profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    /// 12 GB+: int8 encoder (8.9 GB resident). Tested on an RTX 3060 12 GB.
    #[default]
    Balanced,
    /// ~10 GB: w4a8 encoder (6.3 GB). Cuts peak VRAM by ~2.5 GB at the same speed.
    /// Experimental: not yet tested on a real 8 GB card.
    Compact,
}

impl Profile {
    pub fn encoder(self) -> ModelFile {
        match self {
            Profile::Balanced => TE_INT8,
            Profile::Compact => TE_W4A8,
        }
    }
    pub fn models(self) -> [ModelFile; 3] {
        [DIT, self.encoder(), VAE]
    }
    pub fn label(self) -> &'static str {
        match self {
            Profile::Balanced => "12 GB+ (int8 encoder)",
            Profile::Compact => "10 GB, experimental; 8 GB untested (w4a8 encoder)",
        }
    }
    /// Pick a profile from total VRAM in MB.
    pub fn for_vram(total_mb: u32) -> Profile {
        if total_mb >= 12_000 {
            Profile::Balanced
        } else {
            Profile::Compact
        }
    }
    pub fn parse(s: &str) -> Option<Profile> {
        match s.to_ascii_lowercase().as_str() {
            "balanced" | "12gb" | "12" => Some(Profile::Balanced),
            "compact" | "8gb" | "8" => Some(Profile::Compact),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProfileMode {
    #[default]
    Auto,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuSelection {
    pub uuid: Option<String>,
    #[serde(default)]
    pub profile_mode: ProfileMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerificationStatus {
    Passed,
    Skipped,
    Failed,
}

pub const MAX_VERIFICATION_MESSAGE_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuVerification {
    pub gpu_uuid: String,
    pub profile: Profile,
    pub engine_fingerprint: String,
    pub status: VerificationStatus,
    pub checked_at: String,
    pub seconds: Option<f64>,
    pub peak_vram_mb: Option<u32>,
    #[serde(
        default,
        deserialize_with = "deserialize_verification_message",
        serialize_with = "serialize_verification_message"
    )]
    pub message: Option<String>,
}

impl GpuVerification {
    pub fn matches_context(
        &self,
        gpu_uuid: &str,
        profile: Profile,
        engine_fingerprint: &str,
    ) -> bool {
        self.gpu_uuid == gpu_uuid
            && self.profile == profile
            && self.engine_fingerprint == engine_fingerprint
    }
}

fn serialize_verification_message<S>(
    message: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match message.as_deref().and_then(bounded_verification_message) {
        Some(message) => serializer.serialize_some(&message),
        None => serializer.serialize_none(),
    }
}

fn deserialize_verification_message<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
        .map(|message| message.as_deref().and_then(bounded_verification_message))
}

fn bounded_verification_message(message: &str) -> Option<String> {
    let normalized = message.split_whitespace().collect::<Vec<_>>().join(" ");
    let bounded: String = normalized
        .chars()
        .take(MAX_VERIFICATION_MESSAGE_CHARS)
        .collect();
    (!bounded.is_empty()).then_some(bounded)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub runtime_dir: PathBuf,
    pub models_dir: PathBuf,
    #[serde(default)]
    pub profile: Profile,
    #[serde(default)]
    pub gpu_selection: GpuSelection,
    #[serde(default)]
    pub gpu_verification: Option<GpuVerification>,
    /// Where generated images go.
    pub output_dir: PathBuf,
    #[serde(default = "default_save_json")]
    pub save_json: bool,
    #[serde(default)]
    pub config_version: u32,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 {
    8188
}

fn default_save_json() -> bool {
    true
}

fn legacy_output_dir() -> PathBuf {
    dirs::picture_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"))
        .join("PicaLum QwenImage")
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_ID)
        .join("config.json")
}

pub fn default_runtime_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_ID)
}

impl Default for Config {
    fn default() -> Self {
        let runtime_dir = default_runtime_dir();
        Config {
            models_dir: runtime_dir.join("models"),
            runtime_dir: runtime_dir.clone(),
            profile: Profile::default(),
            gpu_selection: GpuSelection::default(),
            gpu_verification: None,
            output_dir: runtime_dir.join("ComfyUI").join("output"),
            save_json: default_save_json(),
            config_version: CURRENT_CONFIG_VERSION,
            port: default_port(),
        }
    }
}

impl Config {
    /// Loads the saved config. `QIL_RUNTIME` / `QIL_MODELS` override paths (handy for dev and CI).
    pub fn load() -> Config {
        let mut cfg: Config = std::fs::read(config_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        if migrate_config(&mut cfg, &legacy_output_dir()) {
            let _ = cfg.save();
        }
        if let Ok(v) = std::env::var("QIL_RUNTIME") {
            cfg.runtime_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("QIL_MODELS") {
            cfg.models_dir = PathBuf::from(v);
        }
        cfg
    }

    pub fn exists() -> bool {
        config_path().is_file()
    }

    pub fn save(&self) -> std::io::Result<()> {
        self.save_to_path(&config_path())
    }

    fn save_to_path(&self, p: &Path) -> std::io::Result<()> {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(
            p,
            serde_json::to_vec_pretty(self).expect("config serializes"),
        )
    }

    pub fn comfy_dir(&self) -> PathBuf {
        self.runtime_dir.join("ComfyUI")
    }
    pub fn python(&self) -> PathBuf {
        self.comfy_dir()
            .join(".venv")
            .join("Scripts")
            .join("python.exe")
    }
    pub fn uv(&self) -> PathBuf {
        self.runtime_dir.join("bin").join("uv.exe")
    }
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    pub fn ws_url(&self, client_id: &str) -> String {
        format!("ws://127.0.0.1:{}/ws?clientId={client_id}", self.port)
    }
    pub fn engine_log(&self) -> PathBuf {
        self.runtime_dir.join("engine.log")
    }
    /// ComfyUI reads extra model folders from this file, so the weights can live anywhere.
    pub fn extra_paths_yaml(&self) -> PathBuf {
        self.runtime_dir.join("extra_model_paths.yaml")
    }
    pub fn deps_marker(&self) -> PathBuf {
        self.runtime_dir
            .join(format!(".deps-{}", &COMFY_COMMIT[..12]))
    }
}

/// Applies one-time schema migrations without accessing any output directories.
/// Returns whether the caller should persist the updated configuration.
fn migrate_config(cfg: &mut Config, legacy_output: &Path) -> bool {
    if cfg.config_version >= CURRENT_CONFIG_VERSION {
        return false;
    }

    if cfg.config_version == 0 && cfg.output_dir == legacy_output {
        cfg.output_dir = cfg.comfy_dir().join("output");
    }
    if cfg.config_version == 0 {
        cfg.save_json = true;
    }
    cfg.config_version = CURRENT_CONFIG_VERSION;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_by_vram() {
        assert_eq!(Profile::for_vram(10_000), Profile::Compact);
        assert_eq!(Profile::for_vram(11_999), Profile::Compact);
        assert_eq!(Profile::for_vram(12_000), Profile::Balanced);
    }

    #[test]
    fn model_urls() {
        assert!(DIT
            .url()
            .ends_with("/diffusion_models/qwen_image_2.1_int8_convrot.safetensors"));
        assert_eq!(
            Profile::Compact.encoder().name,
            "qwen3vl_8b_w4a8.safetensors"
        );
    }

    #[test]
    fn defaults_use_comfy_output_and_enable_json() {
        let cfg = Config::default();
        assert_eq!(cfg.output_dir, cfg.comfy_dir().join("output"));
        assert!(cfg.save_json);
        assert_eq!(cfg.config_version, CURRENT_CONFIG_VERSION);
    }

    #[test]
    fn version_zero_migrates_only_the_exact_legacy_output() {
        let legacy = PathBuf::from(r"D:\Users\person\Pictures\PicaLum QwenImage");
        let mut cfg = Config::default();
        cfg.config_version = 0;
        cfg.output_dir = legacy.clone();
        cfg.save_json = false;

        assert!(migrate_config(&mut cfg, &legacy));
        assert_eq!(cfg.output_dir, cfg.comfy_dir().join("output"));
        assert!(cfg.save_json);
        assert_eq!(cfg.config_version, CURRENT_CONFIG_VERSION);

        let mut custom = Config::default();
        custom.config_version = 0;
        custom.output_dir = PathBuf::from(r"D:\Art\Qwen");
        let custom_path = custom.output_dir.clone();
        assert!(migrate_config(&mut custom, &legacy));
        assert_eq!(custom.output_dir, custom_path);
        assert_eq!(custom.config_version, CURRENT_CONFIG_VERSION);
    }

    #[test]
    fn legacy_config_json_defaults_gpu_selection_and_verification() {
        let legacy_json = r#"{
            "runtimeDir": "C:/runtime",
            "modelsDir": "C:/models",
            "profile": "compact",
            "outputDir": "C:/output",
            "saveJson": false,
            "configVersion": 1,
            "port": 9000
        }"#;

        let config: Config = serde_json::from_str(legacy_json).unwrap();

        assert_eq!(config.gpu_selection.uuid, None);
        assert_eq!(config.gpu_selection.profile_mode, ProfileMode::Auto);
        assert_eq!(config.gpu_verification, None);
    }

    #[test]
    fn auto_gpu_selection_round_trips() {
        let mut config = Config::default();
        config.gpu_selection = GpuSelection {
            uuid: Some("GPU-aaaaaaaa".to_string()),
            profile_mode: ProfileMode::Auto,
        };

        let restored: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();

        assert_eq!(restored.gpu_selection.uuid.as_deref(), Some("GPU-aaaaaaaa"));
        assert_eq!(restored.gpu_selection.profile_mode, ProfileMode::Auto);
    }

    #[test]
    fn manual_gpu_selection_round_trips() {
        let mut config = Config::default();
        config.gpu_selection = GpuSelection {
            uuid: Some("GPU-bbbbbbbb".to_string()),
            profile_mode: ProfileMode::Manual,
        };

        let restored: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();

        assert_eq!(restored.gpu_selection.uuid.as_deref(), Some("GPU-bbbbbbbb"));
        assert_eq!(restored.gpu_selection.profile_mode, ProfileMode::Manual);
    }

    #[test]
    fn verification_state_round_trips() {
        let verification = GpuVerification {
            gpu_uuid: "GPU-aaaaaaaa".to_string(),
            profile: Profile::Balanced,
            engine_fingerprint: "comfy-3b4c0b0e".to_string(),
            status: VerificationStatus::Passed,
            checked_at: "2026-10-09T12:00:00Z".to_string(),
            seconds: Some(31.5),
            peak_vram_mb: Some(11_400),
            message: Some("Draft verification passed.".to_string()),
        };
        let mut config = Config::default();
        config.gpu_verification = Some(verification.clone());

        let restored: Config =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();

        assert_eq!(restored.gpu_verification, Some(verification));
    }

    #[test]
    fn verification_matches_only_the_current_gpu_profile_and_engine() {
        let verification = GpuVerification {
            gpu_uuid: "GPU-aaaaaaaa".to_string(),
            profile: Profile::Balanced,
            engine_fingerprint: "comfy-3b4c0b0e".to_string(),
            status: VerificationStatus::Passed,
            checked_at: "2026-10-09T12:00:00Z".to_string(),
            seconds: None,
            peak_vram_mb: None,
            message: None,
        };

        assert!(verification.matches_context("GPU-aaaaaaaa", Profile::Balanced, "comfy-3b4c0b0e"));
        assert!(!verification.matches_context("GPU-bbbbbbbb", Profile::Balanced, "comfy-3b4c0b0e"));
        assert!(!verification.matches_context("GPU-aaaaaaaa", Profile::Compact, "comfy-3b4c0b0e"));
        assert!(!verification.matches_context("GPU-aaaaaaaa", Profile::Balanced, "comfy-other"));
    }

    #[test]
    fn verification_message_serialization_is_bounded_and_single_line() {
        let mut config = Config::default();
        config.gpu_verification = Some(GpuVerification {
            gpu_uuid: "GPU-aaaaaaaa".to_string(),
            profile: Profile::Balanced,
            engine_fingerprint: "comfy-3b4c0b0e".to_string(),
            status: VerificationStatus::Failed,
            checked_at: "2026-10-09T12:00:00Z".to_string(),
            seconds: None,
            peak_vram_mb: None,
            message: Some(format!(
                "CUDA failed:\n{}",
                "x".repeat(MAX_VERIFICATION_MESSAGE_CHARS + 1)
            )),
        });

        let serialized = serde_json::to_value(&config).unwrap();
        let message = serialized["gpuVerification"]["message"].as_str().unwrap();

        assert!(message.len() <= MAX_VERIFICATION_MESSAGE_CHARS);
        assert!(!message.contains('\n'));
    }

    #[test]
    fn failed_config_save_leaves_in_memory_state_unchanged() {
        let config = Config::default();
        let before = serde_json::to_value(&config).unwrap();
        let blocking_path = std::env::temp_dir().join(format!(
            "qil-config-save-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&blocking_path, "config").unwrap();

        let result = config.save_to_path(&blocking_path.join("config.json"));

        assert!(result.is_err());
        assert_eq!(serde_json::to_value(&config).unwrap(), before);
        std::fs::remove_file(blocking_path).unwrap();
    }
}
