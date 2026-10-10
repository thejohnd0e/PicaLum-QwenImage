use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

use base64::Engine as _;
use qil_core::prompt_presets::PromptPresetFile;
use qil_core::{
    engine, generate, gpu, prompt_presets, setup, surprise, verification, Config, Engine,
    EngineState, GenRequest, GenResult, Profile,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// Guard against double generation; released on Drop even if the task fails.
static BUSY: AtomicBool = AtomicBool::new(false);
static INSTALLING: AtomicBool = AtomicBool::new(false);
struct Flag(&'static AtomicBool);
impl Drop for Flag {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct Settings {
    config: RwLock<Config>,
    active_generation: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
    active_verification: Mutex<Option<tokio::sync::watch::Sender<bool>>>,
}

struct ActiveGenerationGuard<'a>(&'a Settings);

impl Drop for ActiveGenerationGuard<'_> {
    fn drop(&mut self) {
        match self.0.active_generation.lock() {
            Ok(mut active) => *active = None,
            Err(poisoned) => *poisoned.into_inner() = None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    output_dir: String,
    save_json: bool,
}

fn settings_snapshot(config: &Config) -> AppSettings {
    AppSettings {
        output_dir: config.output_dir.to_string_lossy().into_owned(),
        save_json: config.save_json,
    }
}

fn validated_output_dir(path: &str) -> Result<PathBuf, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Output folder cannot be empty.".into());
    }
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err("Output folder must be an absolute path.".into());
    }
    Ok(path)
}

fn cfg(s: &State<'_, Settings>) -> Config {
    s.config.read().unwrap().clone()
}

// ── Setup ──

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SetupInfo {
    status: setup::Status,
    config: Config,
    configured: bool,
    recommended: Option<Profile>,
}

#[tauri::command]
async fn setup_status(settings: State<'_, Settings>) -> Result<SetupInfo, String> {
    let c = cfg(&settings);
    let status = tauri::async_runtime::spawn_blocking({
        let c = c.clone();
        move || setup::status(&c)
    })
    .await
    .map_err(|e| e.to_string())?;
    let recommended = status.recommendation.as_ref().and_then(|r| r.profile);
    Ok(SetupInfo {
        status,
        config: c,
        configured: Config::exists(),
        recommended,
    })
}

#[tauri::command]
fn setup_configure(
    settings: State<'_, Settings>,
    profile: Profile,
    models_dir: Option<String>,
    selected_uuid: Option<String>,
    profile_mode: Option<qil_core::config::ProfileMode>,
) -> Result<Config, String> {
    let mut c = cfg(&settings);
    c.profile = profile;
    if let Some(uuid) = selected_uuid {
        c.gpu_selection.uuid = Some(uuid);
    }
    if let Some(mode) = profile_mode {
        c.gpu_selection.profile_mode = mode;
    }
    let gpus = gpu::detect_all().map_err(|e| e.to_string())?;
    setup::resolve_prerequisites(&c, &gpus).map_err(|e| e.to_string())?;
    c.save()
        .map_err(|e| format!("Cannot save setup settings: {e}"))?;
    if let Some(d) = models_dir.filter(|d| !d.trim().is_empty()) {
        c.models_dir = PathBuf::from(d);
    }
    *settings
        .config
        .write()
        .map_err(|_| "Settings unavailable.".to_string())? = c.clone();
    Ok(c.clone())
}

#[tauri::command]
fn setup_run(app: AppHandle, settings: State<'_, Settings>) -> Result<(), String> {
    if INSTALLING.swap(true, Ordering::SeqCst) {
        return Err("Setup is already running.".into());
    }
    let c = cfg(&settings);
    // A system thread, not an async command: the installer blocks on pipes and downloads.
    std::thread::spawn(move || {
        let _flag = Flag(&INSTALLING);
        let res = setup::run(&c, &mut |ev| {
            let _ = app.emit("setup", &ev);
        });
        let _ = app.emit("setup-finished", res.err());
    });
    Ok(())
}

#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose the models folder")
        .blocking_pick_folder()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn app_settings(settings: State<'_, Settings>) -> AppSettings {
    settings_snapshot(&cfg(&settings))
}

#[tauri::command]
async fn pick_output_folder(app: AppHandle) -> Option<String> {
    app.dialog()
        .file()
        .set_title("Choose where generated images are saved")
        .blocking_pick_folder()
        .and_then(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
fn save_app_settings(
    app: AppHandle,
    settings: State<'_, Settings>,
    output_dir: String,
    save_json: bool,
) -> Result<AppSettings, String> {
    if BUSY.load(Ordering::SeqCst) {
        return Err("Settings cannot be changed while generation is running.".into());
    }
    let output_dir = validated_output_dir(&output_dir)?;
    std::fs::create_dir_all(&output_dir)
        .map_err(|e| format!("Cannot create output folder: {e}"))?;
    let mut current = settings.config.write().unwrap();
    if BUSY.load(Ordering::SeqCst) {
        return Err("Settings cannot be changed while generation is running.".into());
    }
    let previous = current.clone();
    let mut updated = previous.clone();
    updated.output_dir = output_dir.clone();
    updated.save_json = save_json;
    updated
        .save()
        .map_err(|e| format!("Cannot save settings: {e}"))?;
    if let Err(error) = app
        .asset_protocol_scope()
        .allow_directory(&output_dir, true)
    {
        let _ = previous.save();
        return Err(format!("Cannot expose output folder to the app: {error}"));
    }
    *current = updated;
    Ok(settings_snapshot(&current))
}

#[tauri::command]
fn prompt_presets_load(settings: State<'_, Settings>) -> Result<PromptPresetFile, String> {
    let config = cfg(&settings);
    prompt_presets::load(&config.runtime_dir).map_err(|error| error.to_string())
}

#[tauri::command]
fn prompt_presets_save(
    settings: State<'_, Settings>,
    document: PromptPresetFile,
) -> Result<PromptPresetFile, String> {
    let config = cfg(&settings);
    prompt_presets::save(&config.runtime_dir, &document).map_err(|error| error.to_string())?;
    Ok(document)
}

// ── Engine ──

fn engine_emitter(app: &AppHandle) -> impl Fn(EngineState) + Send + Sync {
    let app = app.clone();
    move |s: EngineState| {
        let _ = app.emit("engine", s);
    }
}

#[tauri::command]
async fn engine_start(
    app: AppHandle,
    engine: State<'_, Engine>,
    settings: State<'_, Settings>,
) -> Result<(), String> {
    let c = cfg(&settings);
    engine.start(&c, &engine_emitter(&app)).await
}

#[tauri::command]
fn engine_stop(app: AppHandle, engine: State<'_, Engine>) -> bool {
    let stopped = engine.stop();
    if stopped {
        let _ = app.emit(
            "engine",
            EngineState {
                state: "off",
                detail: "Engine stopped".into(),
                external: false,
            },
        );
    }
    stopped
}

// ── Generation ──

#[tauri::command]
async fn generate(
    app: AppHandle,
    settings: State<'_, Settings>,
    req: GenRequest,
    remaining_ms: Option<u64>,
) -> Result<GenResult, String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("A generation is already running.".into());
    }
    let _flag = Flag(&BUSY);
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    *settings
        .active_generation
        .lock()
        .map_err(|_| "Generation cancellation state is unavailable.".to_string())? =
        Some(cancel_tx);
    let _active = ActiveGenerationGuard(&settings);
    let c = cfg(&settings);
    if !engine::is_up(&c).await {
        return Err("The engine is off.".into());
    }
    let emit = move |p: qil_core::Progress| {
        let _ = app.emit("gen-progress", p);
    };
    let now = tokio::time::Instant::now();
    let request_deadline = now + Duration::from_secs(30 * 60);
    let deadline = remaining_ms
        .map(|remaining| now + Duration::from_millis(remaining).min(Duration::from_secs(30 * 60)))
        .map_or(request_deadline, |batch_deadline| {
            request_deadline.min(batch_deadline)
        });
    let options = generate::GenerateOptions {
        config: &c,
        output_dir: c.output_dir.clone(),
        control: qil_core::run_control::RunControl::new(cancel_rx, deadline),
        on_progress: &emit,
    };
    generate::run_with_control(req, options)
        .await
        .map_err(|error| match error {
            qil_core::run_control::RunError::Cancelled => "Cancelled".to_string(),
            qil_core::run_control::RunError::TimedOut => {
                format!("Generation timed out; see {}", c.engine_log().display())
            }
            qil_core::run_control::RunError::CancellationUnconfirmed => "Cancelled; the engine may still be finishing and the next request may queue behind it.".to_string(),
            qil_core::run_control::RunError::TimeoutUnconfirmed => format!(
                "Generation timed out; the engine may still be finishing. See {}",
                c.engine_log().display()
            ),
            qil_core::run_control::RunError::Request(message) => message,
        })
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MaskedEditArgs {
    source_path: String,
    /// Grayscale PNG, source-sized (EXIF-normalized), white = editable; base64.
    mask_png_base64: String,
    instruction: String,
    preset: String,
    seed: Option<u64>,
    feather_px: u32,
    /// Extra photos whose content goes into the painted area (`<image2>`, ...).
    #[serde(default)]
    references: Vec<String>,
}

/// Mask-guided edit of one image. Shares the generation lock, cancel channel and
/// progress event with `generate`; only the final composite is published.
#[tauri::command]
async fn generate_masked_edit(
    app: AppHandle,
    settings: State<'_, Settings>,
    args: MaskedEditArgs,
) -> Result<GenResult, String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("A generation is already running.".into());
    }
    let _flag = Flag(&BUSY);
    let mask_png = base64::engine::general_purpose::STANDARD
        .decode(args.mask_png_base64.as_bytes())
        .map_err(|e| format!("The mask is not valid base64: {e}"))?;
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    *settings
        .active_generation
        .lock()
        .map_err(|_| "Generation cancellation state is unavailable.".to_string())? =
        Some(cancel_tx);
    let _active = ActiveGenerationGuard(&settings);
    let c = cfg(&settings);
    if !engine::is_up(&c).await {
        return Err("The engine is off.".into());
    }
    let emit = move |p: qil_core::Progress| {
        let _ = app.emit("gen-progress", p);
    };
    let options = generate::GenerateOptions {
        config: &c,
        output_dir: c.output_dir.clone(),
        control: qil_core::run_control::RunControl::new(
            cancel_rx,
            tokio::time::Instant::now() + Duration::from_secs(30 * 60),
        ),
        on_progress: &emit,
    };
    let req = qil_core::masked_run::MaskedEditRequest {
        source_path: PathBuf::from(args.source_path),
        mask_png,
        instruction: args.instruction,
        preset: args.preset,
        seed: args.seed,
        feather_px: args.feather_px,
        references: args.references.into_iter().map(PathBuf::from).collect(),
    };
    qil_core::masked_run::run_masked_edit(req, options)
        .await
        .map_err(|error| qil_core::masked_run::error_text(error, &c))
}

#[tauri::command]
async fn cancel(settings: State<'_, Settings>) -> Result<(), String> {
    let active = settings
        .active_generation
        .lock()
        .map_err(|_| "Generation cancellation state is unavailable.".to_string())?
        .clone();
    if let Some(cancel) = active {
        cancel.send_replace(true);
    }
    Ok(())
}

#[tauri::command]
fn surprise_idea(theme: Option<String>) -> surprise::Idea {
    surprise::idea(theme.as_deref())
}

#[tauri::command]
async fn gpu_stats(settings: State<'_, Settings>) -> Result<Option<gpu::Gpu>, String> {
    let uuid = cfg(&settings).gpu_selection.uuid;
    Ok(
        tauri::async_runtime::spawn_blocking(move || gpu::detect_selected(uuid.as_deref()))
            .await
            .map_err(|e| e.to_string())?,
    )
}

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Decodes an image as it should be seen: phone photos are often stored sideways with an
/// EXIF orientation tag, which image::open ignores.
fn open_upright(path: &str) -> Result<image::DynamicImage, String> {
    qil_core::masked_run::open_upright(std::path::Path::new(path))
}

/// Writes a copy of a reference turned 90° clockwise and returns its path. The user's file is
/// never touched; the copy is what gets uploaded to the engine.
#[tauri::command]
async fn rotate_image(settings: State<'_, Settings>, path: String) -> Result<String, String> {
    let dir = cfg(&settings).runtime_dir.join("rotated");
    tauri::async_runtime::spawn_blocking(move || {
        let img = open_upright(&path)?.rotate90();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let stem = std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("image");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        // Keep transparency when there is any; photos go back to JPEG so uploads stay small.
        let out = if img.color().has_alpha() {
            let out = dir.join(format!("{stem}_{stamp}.png"));
            img.save(&out).map_err(|e| e.to_string())?;
            out
        } else {
            let out = dir.join(format!("{stem}_{stamp}.jpg"));
            let file =
                std::io::BufWriter::new(std::fs::File::create(&out).map_err(|e| e.to_string())?);
            image::codecs::jpeg::JpegEncoder::new_with_quality(file, 95)
                .encode_image(&img.to_rgb8())
                .map_err(|e| e.to_string())?;
            out
        };
        Ok(out.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// JPEG thumbnail as a data URL, so the UI can show references without file-system access.
#[tauri::command]
async fn thumbnail(path: String, size: Option<u32>) -> Result<String, String> {
    let size = size.unwrap_or(256);
    tauri::async_runtime::spawn_blocking(move || {
        let t = open_upright(&path)?.thumbnail(size, size).to_rgb8();
        let mut buf = std::io::Cursor::new(Vec::new());
        t.write_to(&mut buf, image::ImageFormat::Jpeg)
            .map_err(|e| e.to_string())?;
        Ok(format!(
            "data:image/jpeg;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(buf.into_inner())
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn pick_images(app: AppHandle) -> Vec<String> {
    app.dialog()
        .file()
        .set_title("Choose reference photos")
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp"])
        .blocking_pick_files()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| f.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[derive(Serialize)]
struct OutputItem {
    path: String,
    meta: Option<serde_json::Value>,
}

#[tauri::command]
fn list_outputs(settings: State<'_, Settings>, limit: Option<usize>) -> Vec<OutputItem> {
    let dir = cfg(&settings).output_dir;
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return vec![];
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .filter_map(|p| Some((p.metadata().ok()?.modified().ok()?, p)))
        .collect();
    files.sort_by_key(|file| std::cmp::Reverse(file.0));
    files
        .into_iter()
        .take(limit.unwrap_or(80))
        .map(|(_, p)| OutputItem {
            meta: std::fs::read(p.with_extension("json"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok()),
            path: p.to_string_lossy().into_owned(),
        })
        .collect()
}

#[tauri::command]
fn output_dir(settings: State<'_, Settings>) -> Result<String, String> {
    let dir = cfg(&settings).output_dir;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
fn reveal(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_path(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err("only https links".into());
    }
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Engine::default())
        .manage(Settings {
            config: RwLock::new(Config::load()),
            active_generation: Mutex::new(None),
            active_verification: Mutex::new(None),
        })
        .setup(|app| {
            // Let the UI show images from the output folder through asset://
            let out = app
                .state::<Settings>()
                .config
                .read()
                .unwrap()
                .output_dir
                .clone();
            let _ = std::fs::create_dir_all(&out);
            let _ = app.asset_protocol_scope().allow_directory(&out, true);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            setup_status,
            setup_configure,
            setup_run,
            pick_folder,
            app_settings,
            pick_output_folder,
            save_app_settings,
            prompt_presets_load,
            prompt_presets_save,
            engine_start,
            engine_stop,
            generate,
            generate_masked_edit,
            cancel,
            gpu_verification_start,
            gpu_verification_cancel,
            gpu_verification_status,
            surprise_idea,
            gpu_stats,
            app_version,
            thumbnail,
            rotate_image,
            pick_images,
            list_outputs,
            output_dir,
            reveal,
            open_path,
            open_url
        ])
        .build(tauri::generate_context!())
        .expect("could not start PicaLum QwenImage");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            handle.state::<Engine>().stop();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_version_matches_cargo_package_version() {
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn output_folder_validation_rejects_empty_and_relative_paths() {
        assert!(validated_output_dir("").is_err());
        assert!(validated_output_dir("relative/output").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn output_folder_validation_accepts_absolute_windows_paths() {
        assert_eq!(
            validated_output_dir(r"D:\Art\Qwen").unwrap(),
            PathBuf::from(r"D:\Art\Qwen")
        );
    }
}

#[tauri::command]
async fn gpu_verification_start(
    app: AppHandle,
    engine: State<'_, Engine>,
    settings: State<'_, Settings>,
) -> Result<qil_core::GpuVerification, String> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Err("Generation or verification is already running.".into());
    }
    let _flag = Flag(&BUSY);
    let c = cfg(&settings);
    let uuid = c
        .gpu_selection
        .uuid
        .clone()
        .ok_or("No selected GPU UUID is configured.")?;
    let run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis()
        .to_string();
    let output = verification::run_dir(&c.runtime_dir, &run_id);
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    *settings
        .active_verification
        .lock()
        .map_err(|_| "Verification state unavailable.".to_string())? = Some(cancel_tx);
    let started = tokio::time::Instant::now();
    let startup = tokio::time::timeout(
        Duration::from_secs(60),
        engine.start(&c, &engine_emitter(&app)),
    )
    .await;
    match startup {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            let _ = verification::cleanup_empty(&output);
            return Err(format!("Verification engine could not start: {error}"));
        }
        Err(_) => {
            engine.stop();
            let _ = verification::cleanup_empty(&output);
            return Err(
                "Verification engine startup timed out; verification did not begin.".into(),
            );
        }
    }
    let peak = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let sampling = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let sampler_peak = peak.clone();
    let sampler_run = sampling.clone();
    let sampler_uuid = uuid.clone();
    let sampler = std::thread::spawn(move || {
        while sampler_run.load(Ordering::Relaxed) {
            if let Some(g) = gpu::detect_selected(Some(&sampler_uuid)) {
                sampler_peak.fetch_max(g.used_mb, Ordering::Relaxed);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
    let emit = move |p: qil_core::Progress| {
        let _ = app.emit("gpu-verification-progress", p);
    };
    let remaining =
        Duration::from_secs(verification::MAX_SECONDS).saturating_sub(started.elapsed());
    let control =
        qil_core::run_control::RunControl::new(cancel_rx, tokio::time::Instant::now() + remaining);
    let options = generate::GenerateOptions {
        config: &c,
        output_dir: output.clone(),
        control,
        on_progress: &emit,
    };
    let result = generate::run_with_control(verification::request(), options).await;
    sampling.store(false, Ordering::Relaxed);
    let _ = sampler.join();
    if let Ok(mut active) = settings.active_verification.lock() {
        *active = None;
    }
    let cancelled = matches!(result, Err(qil_core::run_control::RunError::Cancelled));
    let timed_out = matches!(
        result,
        Err(qil_core::run_control::RunError::TimedOut
            | qil_core::run_control::RunError::TimeoutUnconfirmed)
    );
    let text = result.as_ref().err().map(|e| format!("{e:?}"));
    let outcome = verification::classify(text.as_deref(), cancelled, timed_out);
    let status = if outcome.class == verification::VerificationClass::Success {
        qil_core::config::VerificationStatus::Passed
    } else if outcome.class == verification::VerificationClass::Cancelled {
        qil_core::config::VerificationStatus::Skipped
    } else {
        qil_core::config::VerificationStatus::Failed
    };
    let record = verification::record(
        uuid,
        c.profile,
        status,
        Some(started.elapsed().as_secs_f64()),
        Some(peak.load(Ordering::Relaxed)),
        text,
    );
    let mut updated = c.clone();
    updated.gpu_verification = Some(record.clone());
    updated.save().map_err(|e| e.to_string())?;
    *settings
        .config
        .write()
        .map_err(|_| "Settings unavailable.".to_string())? = updated;
    verification::cleanup(&output, outcome.cleanup_safe).map_err(|e| e.to_string())?;
    if timed_out && !outcome.cleanup_safe {
        return Err("Verification timed out before cancellation was confirmed; temporary output was retained.".into());
    }
    Ok(record)
}

#[tauri::command]
fn gpu_verification_cancel(settings: State<'_, Settings>) -> Result<(), String> {
    if let Some(tx) = settings
        .active_verification
        .lock()
        .map_err(|_| "Verification state unavailable.".to_string())?
        .clone()
    {
        tx.send_replace(true);
    }
    Ok(())
}

#[tauri::command]
fn gpu_verification_status(settings: State<'_, Settings>) -> Option<qil_core::GpuVerification> {
    cfg(&settings).gpu_verification
}
