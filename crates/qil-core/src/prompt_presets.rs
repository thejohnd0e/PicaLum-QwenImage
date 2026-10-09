use std::collections::HashSet;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const CURRENT_VERSION: u32 = 1;
const BUILTIN_NAME_KEYS: [(&str, &str); 6] = [
    ("remove-background", "qRemoveBg"),
    ("snowy-night", "qSnow"),
    ("watercolor", "qWatercolor"),
    ("golden-hour", "qGolden"),
    ("remove-people", "qNoPeople"),
    ("combine-1-2", "qCombine"),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptPreset {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_key: Option<String>,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptPresetFile {
    pub version: u32,
    pub presets: Vec<PromptPreset>,
}

#[derive(Debug)]
pub enum PromptPresetError {
    Io(std::io::Error),
    Json(serde_json::Error),
    UnsupportedVersion(u32),
    Validation(String),
}

impl fmt::Display for PromptPresetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "prompt preset I/O error: {error}"),
            Self::Json(error) => write!(f, "invalid prompt preset JSON: {error}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported prompt preset version: {version}")
            }
            Self::Validation(message) => write!(f, "invalid prompt presets: {message}"),
        }
    }
}

impl std::error::Error for PromptPresetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            Self::UnsupportedVersion(_) | Self::Validation(_) => None,
        }
    }
}

pub fn defaults() -> PromptPresetFile {
    let presets = [
        (
            "remove-background",
            "Remove background",
            "qRemoveBg",
            "Remove the background, and output a PNG image",
        ),
        (
            "snowy-night",
            "Snowy night",
            "qSnow",
            "Change the scene to a snowy winter night with warm lights in the windows. Keep everything else the same.",
        ),
        (
            "watercolor",
            "Watercolor",
            "qWatercolor",
            "Turn this photo into a hand-painted watercolor illustration. Keep the composition.",
        ),
        (
            "golden-hour",
            "Golden hour",
            "qGolden",
            "Relight this photo as a golden-hour scene with soft warm sunlight. Keep the subject and composition identical.",
        ),
        (
            "remove-people",
            "Remove people",
            "qNoPeople",
            "Remove all the people from the photo and fill the background naturally. Keep everything else identical.",
        ),
        (
            "combine-1-2",
            "Combine 1 + 2",
            "qCombine",
            "Place the subject from <image2> into the scene of <image1>, matching lighting and perspective, with a realistic contact shadow.",
        ),
    ]
    .into_iter()
    .map(|(id, name, name_key, prompt)| PromptPreset {
        id: id.into(),
        name: name.into(),
        name_key: Some(name_key.into()),
        prompt: prompt.into(),
    })
    .collect();

    PromptPresetFile {
        version: CURRENT_VERSION,
        presets,
    }
}

pub fn path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("presets.json")
}

pub fn validate(document: &PromptPresetFile) -> Result<(), PromptPresetError> {
    if document.version != CURRENT_VERSION {
        return Err(PromptPresetError::UnsupportedVersion(document.version));
    }

    let mut ids = HashSet::with_capacity(document.presets.len());
    for preset in &document.presets {
        if preset.id.trim().is_empty() {
            return Err(PromptPresetError::Validation(
                "preset IDs must not be empty".into(),
            ));
        }
        if !ids.insert(&preset.id) {
            return Err(PromptPresetError::Validation(format!(
                "duplicate preset ID: {}",
                preset.id
            )));
        }
        if preset.name.trim().is_empty() {
            return Err(PromptPresetError::Validation(format!(
                "preset {} has an empty name",
                preset.id
            )));
        }
        if preset.prompt.trim().is_empty() {
            return Err(PromptPresetError::Validation(format!(
                "preset {} has an empty prompt",
                preset.id
            )));
        }
        if let Some(key) = &preset.name_key {
            if !BUILTIN_NAME_KEYS
                .iter()
                .any(|(id, builtin_key)| preset.id == *id && key == builtin_key)
            {
                return Err(PromptPresetError::Validation(format!(
                    "name key {key} is not reserved for preset ID {}",
                    preset.id
                )));
            }
        }
    }
    Ok(())
}

pub fn load(runtime_dir: &Path) -> Result<PromptPresetFile, PromptPresetError> {
    let source = path(runtime_dir);
    if !source.try_exists().map_err(PromptPresetError::Io)? {
        return Ok(defaults());
    }
    let bytes = std::fs::read(source).map_err(PromptPresetError::Io)?;
    let document: PromptPresetFile =
        serde_json::from_slice(&bytes).map_err(PromptPresetError::Json)?;
    validate(&document)?;
    Ok(document)
}

pub fn save(runtime_dir: &Path, document: &PromptPresetFile) -> Result<(), PromptPresetError> {
    validate(document)?;
    let bytes = serde_json::to_vec_pretty(document).map_err(PromptPresetError::Json)?;
    std::fs::create_dir_all(runtime_dir).map_err(PromptPresetError::Io)?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(runtime_dir).map_err(PromptPresetError::Io)?;
    temporary.write_all(&bytes).map_err(PromptPresetError::Io)?;
    temporary
        .as_file()
        .sync_all()
        .map_err(PromptPresetError::Io)?;
    temporary
        .persist(path(runtime_dir))
        .map_err(|error| PromptPresetError::Io(error.error))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn preset(id: &str, name: &str, prompt: &str) -> PromptPreset {
        PromptPreset {
            id: id.into(),
            name: name.into(),
            name_key: None,
            prompt: prompt.into(),
        }
    }

    fn document(presets: Vec<PromptPreset>) -> PromptPresetFile {
        PromptPresetFile {
            version: 1,
            presets,
        }
    }

    #[test]
    fn missing_file_returns_defaults_without_creating_it() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(dir.path()).unwrap();
        assert_eq!(loaded, defaults());
        assert_eq!(loaded.presets.len(), 6);
        assert!(!path(dir.path()).exists());
    }

    #[test]
    fn builtins_preserve_the_six_original_quick_prompts_and_names() {
        let value = defaults();
        let expected = [
            ("remove-background", "Remove background", "qRemoveBg", "Remove the background, and output a PNG image"),
            ("snowy-night", "Snowy night", "qSnow", "Change the scene to a snowy winter night with warm lights in the windows. Keep everything else the same."),
            ("watercolor", "Watercolor", "qWatercolor", "Turn this photo into a hand-painted watercolor illustration. Keep the composition."),
            ("golden-hour", "Golden hour", "qGolden", "Relight this photo as a golden-hour scene with soft warm sunlight. Keep the subject and composition identical."),
            ("remove-people", "Remove people", "qNoPeople", "Remove all the people from the photo and fill the background naturally. Keep everything else identical."),
            ("combine-1-2", "Combine 1 + 2", "qCombine", "Place the subject from <image2> into the scene of <image1>, matching lighting and perspective, with a realistic contact shadow."),
        ];
        for (preset, (id, name, name_key, prompt)) in value.presets.iter().zip(expected) {
            assert_eq!(preset.id, id);
            assert_eq!(preset.name, name);
            assert_eq!(preset.name_key.as_deref(), Some(name_key));
            assert_eq!(preset.prompt, prompt);
        }
    }

    #[test]
    fn save_and_load_preserve_multiline_unicode_quotes_and_image_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let value = "Сохрани строку\nСкажи: \"да\" <image2>\n  пробелы  ";
        let expected = document(vec![preset("custom", "Мой стиль", value)]);
        save(dir.path(), &expected).unwrap();
        assert_eq!(load(dir.path()).unwrap(), expected);
    }

    #[test]
    fn saving_replaces_the_existing_document() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &document(vec![preset("one", "One", "First")])).unwrap();
        let updated = document(vec![preset("two", "Two", "Second")]);
        save(dir.path(), &updated).unwrap();
        assert_eq!(load(dir.path()).unwrap(), updated);
    }

    #[test]
    fn removed_builtin_stays_removed() {
        let dir = tempfile::tempdir().unwrap();
        let mut value = defaults();
        value.presets.retain(|p| p.id != "snowy-night");
        save(dir.path(), &value).unwrap();
        assert_eq!(load(dir.path()).unwrap(), value);
    }

    #[test]
    fn empty_preset_list_persists() {
        let dir = tempfile::tempdir().unwrap();
        let empty = document(vec![]);
        save(dir.path(), &empty).unwrap();
        assert_eq!(load(dir.path()).unwrap(), empty);
    }

    #[test]
    fn malformed_json_errors_without_changing_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let target = path(dir.path());
        fs::write(&target, b"{broken json").unwrap();
        assert!(load(dir.path()).is_err());
        assert_eq!(fs::read(target).unwrap(), b"{broken json");
    }

    #[test]
    fn unknown_version_errors_without_changing_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let target = path(dir.path());
        let bytes = br#"{"version":99,"presets":[]}"#;
        fs::write(&target, bytes).unwrap();
        assert!(load(dir.path()).is_err());
        assert_eq!(fs::read(target).unwrap(), bytes);
    }

    #[test]
    fn validation_rejects_duplicate_ids_and_blank_names_or_prompts() {
        assert!(validate(&document(vec![
            preset("same", "One", "p"),
            preset("same", "Two", "q")
        ]))
        .is_err());
        assert!(validate(&document(vec![preset("id", "  ", "p")])).is_err());
        assert!(validate(&document(vec![preset("id", "Name", "\n ")])).is_err());
    }

    #[test]
    fn validation_reserves_builtin_name_keys_for_their_builtin_ids() {
        let mut custom = preset("user-preset", "Custom", "Prompt");
        custom.name_key = Some("qRemoveBg".into());
        assert!(validate(&document(vec![custom])).is_err());

        let mut builtin = defaults().presets.remove(0);
        builtin.prompt = "Edited prompt".into();
        assert!(validate(&document(vec![builtin])).is_ok());
    }

    #[test]
    fn invalid_save_does_not_alter_previously_valid_file() {
        let dir = tempfile::tempdir().unwrap();
        let valid = document(vec![preset("ok", "Okay", "Prompt")]);
        save(dir.path(), &valid).unwrap();
        let before = fs::read(path(dir.path())).unwrap();
        assert!(save(dir.path(), &document(vec![preset("bad", "", "Prompt")])).is_err());
        assert_eq!(fs::read(path(dir.path())).unwrap(), before);
    }

    #[test]
    fn path_uses_the_supplied_runtime_directory() {
        let dir = std::path::Path::new("chosen-runtime");
        assert_eq!(path(dir), dir.join("presets.json"));
    }
}
