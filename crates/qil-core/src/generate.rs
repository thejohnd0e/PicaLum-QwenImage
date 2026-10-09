//! Generation: builds the ComfyUI graph, queues it, follows progress over the
//! WebSocket, embeds interoperable PNG metadata, and optionally writes a JSON sidecar.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::time::Instant;

use crate::config::{Config, DIT, VAE};
use crate::png_metadata::{embed_generation_metadata, GenerationMetadata};
use crate::run_control::{RunControl, RunError};

pub const MAX_REFS: usize = 10;

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GenRequest {
    pub mode: String, // "text" | "photos" | "surprise" | "bench"
    pub prompt: String,
    #[serde(default)]
    pub images: Vec<String>,
    pub preset: String, // "draft" | "standard" | "max"
    pub aspect: String, // "16:9", ...
    pub seed: Option<u64>,
    #[serde(default)]
    pub transparent: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GenResult {
    pub path: String,
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub seconds: f64,
    /// Seconds spent in the sampler only (excludes model loading and uploads).
    pub sample_seconds: f64,
    pub steps: u32,
    pub prompt: String,
    pub mode: String,
    pub preset: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Progress {
    pub stage: &'static str, // upload | load | encode | sample | decode | save
    pub value: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Preset {
    pub steps: u32,
    pub megapixels: f64,
    pub edit_res: u32,
}

pub fn preset(name: &str) -> Preset {
    match name {
        "draft" => Preset {
            steps: 20,
            megapixels: 1.048_576,
            edit_res: 1024,
        }, // 1024²
        "max" => Preset {
            steps: 40,
            megapixels: 4.0,
            edit_res: 1536,
        }, // official defaults
        _ => Preset {
            steps: 30,
            megapixels: 1.6,
            edit_res: 1280,
        },
    }
}

/// Output size. "max" uses the official 2K table; otherwise it scales to the megapixel budget.
pub fn dims(aspect: &str, megapixels: f64) -> (u32, u32) {
    if megapixels >= 4.0 {
        let official = match aspect {
            "4:3" => Some((2400, 1792)),
            "3:4" => Some((1792, 2400)),
            "3:2" => Some((2528, 1696)),
            "2:3" => Some((1696, 2528)),
            "16:9" => Some((2752, 1536)),
            "9:16" => Some((1536, 2752)),
            "1:1" => Some((2048, 2048)),
            _ => None,
        };
        if let Some(d) = official {
            return d;
        }
    }
    let (a, b) = aspect
        .split_once(':')
        .and_then(|(a, b)| Some((a.parse::<f64>().ok()?, b.parse::<f64>().ok()?)))
        .unwrap_or((1.0, 1.0));
    let r = a / b;
    let w = (megapixels * 1e6 * r).sqrt();
    let h = w / r;
    let snap = |v: f64| ((v / 32.0).round() as u32).max(1) * 32;
    (snap(w), snap(h))
}

/// The prompt wrapper Qwen documents for native RGBA output.
pub fn wrap_transparent(p: &str) -> String {
    format!(
        "This is an RGBA format image with transparency. {}. The image has an alpha channel and a transparent background.",
        p.trim().trim_end_matches('.')
    )
}

#[allow(clippy::too_many_arguments)]
pub fn build_graph(
    prompt: &str,
    refs: &[String],
    w: u32,
    h: u32,
    steps: u32,
    seed: u64,
    edit_res: u32,
    encoder: &str,
) -> Value {
    let mut g = json!({
        "unet": {"class_type": "UNETLoader", "inputs": {"unet_name": DIT.name, "weight_dtype": "default"}},
        "clip": {"class_type": "CLIPLoader", "inputs": {"clip_name": encoder, "type": "qwen_image", "device": "default"}},
        "vae": {"class_type": "VAELoader", "inputs": {"vae_name": VAE.name}},
        "enc": {"class_type": "TextEncodeQwenImage21", "inputs": {
            "clip": ["clip", 0], "prompt": prompt, "negative_prompt": "", "resolution": edit_res}},
        "decode": {"class_type": "VAEDecode", "inputs": {"samples": ["sampler", 0], "vae": ["vae", 0]}},
        "save": {"class_type": "PreviewImage", "inputs": {"images": ["decode", 0]}},
    });
    let (model, latent) = if refs.is_empty() {
        g["latent"] = json!({"class_type": "EmptyLatentImage", "inputs": {"width": w, "height": h, "batch_size": 1}});
        (json!(["unet", 0]), json!(["latent", 0]))
    } else {
        // References go to the vision encoder and in as latents; the canvas comes from the first one.
        g["enc"]["inputs"]["vae"] = json!(["vae", 0]);
        for (i, name) in refs.iter().enumerate() {
            let id = format!("load{}", i + 1);
            g[&id] = json!({"class_type": "LoadImage", "inputs": {"image": name}});
            g["enc"]["inputs"][format!("images.image_{}", i + 1)] = json!([id, 0]);
        }
        g["cache"] = json!({"class_type": "QwenImage21Cache", "inputs": {"model": ["unet", 0], "device": "auto", "dtype": "default"}});
        (json!(["cache", 0]), json!(["enc", 2]))
    };
    g["sampler"] = json!({"class_type": "KSampler", "inputs": {
        "model": model, "seed": seed, "steps": steps, "cfg": 1.0, "sampler_name": "euler", "scheduler": "simple",
        "positive": ["enc", 0], "negative": ["enc", 1], "latent_image": latent, "denoise": 1.0}});
    g
}

fn stage_for(node: &str) -> &'static str {
    match node {
        "enc" => "encode",
        "sampler" => "sample",
        "decode" => "decode",
        "save" => "save",
        _ => "load",
    }
}

async fn upload(
    cfg: &Config,
    client: &reqwest::Client,
    path: &Path,
    tag: &str,
    idx: usize,
) -> Result<String, String> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("png")
        .to_lowercase();
    // Unique name: two photos with the same name in different folders must not overwrite each other.
    let name = format!("qil_{tag}_{idx}.{ext}");
    let part = reqwest::multipart::Part::bytes(bytes).file_name(name);
    let form = reqwest::multipart::Form::new()
        .part("image", part)
        .text("overwrite", "true");
    let res: Value = client
        .post(format!("{}/upload/image", cfg.base_url()))
        .multipart(form)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    res["name"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("Upload rejected: {res}"))
}

fn node_error_text(v: &Value) -> String {
    // /prompt returns {error:{message}, node_errors:{id:{errors:[{message,details}]}}}
    let mut out = v["error"]["message"]
        .as_str()
        .unwrap_or("The engine rejected the request")
        .to_string();
    if let Some(nodes) = v["node_errors"].as_object() {
        for (id, n) in nodes {
            for e in n["errors"].as_array().into_iter().flatten() {
                out.push_str(&format!(
                    " · {id}: {} {}",
                    e["message"].as_str().unwrap_or(""),
                    e["details"].as_str().unwrap_or("")
                ));
            }
        }
    }
    out
}

fn output_image(entry: &Value) -> Result<&Value, String> {
    let outputs = entry["outputs"]
        .as_object()
        .ok_or_else(|| "The engine history had no usable image metadata".to_string())?;
    let mut saw_image_entry = false;
    for node in outputs.values() {
        let images = node["output"]["images"]
            .as_array()
            .or_else(|| node["images"].as_array());
        let Some(images) = images else { continue };
        for image in images {
            saw_image_entry = true;
            if image["filename"]
                .as_str()
                .is_some_and(|filename| !filename.is_empty())
            {
                return Ok(image);
            }
        }
    }
    if saw_image_entry {
        Err("The engine history contained an image entry with no filename".into())
    } else {
        Err("The engine history had no usable image metadata".into())
    }
}

fn local_preview_path(cfg: &Config, image: &Value) -> Option<PathBuf> {
    if image["type"].as_str()? != "temp" {
        return None;
    }
    let filename = image["filename"].as_str()?;
    let file_path = Path::new(filename);
    if filename.is_empty()
        || filename.contains(['/', '\\'])
        || file_path.is_absolute()
        || file_path.components().count() != 1
        || !matches!(
            file_path.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return None;
    }
    let subfolder = image["subfolder"].as_str().unwrap_or("");
    let relative = Path::new(subfolder);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return None;
    }
    Some(cfg.comfy_dir().join("temp").join(relative).join(filename))
}

fn should_save_json(save_json: bool) -> bool {
    save_json
}

fn new_prompt_id() -> String {
    let mut bytes = [0_u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u64::from_be_bytes([
            0, 0, bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
        ]),
    )
}

fn unique_output_path(path: &Path) -> Result<PathBuf, String> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("image");
    let extension = path.extension().and_then(|value| value.to_str());
    let mut suffix = 1_u64;
    loop {
        let name = match extension {
            Some(extension) => format!("{stem} ({suffix}).{extension}"),
            None => format!("{stem} ({suffix})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return Ok(candidate);
        }
        suffix = suffix
            .checked_add(1)
            .ok_or_else(|| "Could not find a free output filename".to_string())?;
    }
}

fn queue_delete_payload(prompt_id: &str) -> Value {
    json!({"delete": [prompt_id]})
}

fn interrupt_payload(prompt_id: &str) -> Value {
    json!({"prompt_id": prompt_id})
}

pub struct GenerateOptions<'a> {
    pub config: &'a Config,
    pub output_dir: PathBuf,
    pub control: RunControl,
    pub on_progress: &'a (dyn Fn(Progress) + Send + Sync),
}

struct RunContext<'a> {
    config: &'a Config,
    output_dir: PathBuf,
    control: RunControl,
    on_progress: &'a (dyn Fn(Progress) + Send + Sync),
    client: reqwest::Client,
    prompt_id: String,
    prompt_submitted: bool,
}

/// Runs one generation. `out` is a folder (a timestamped name is picked) or an exact `.png` path.
pub async fn run(
    cfg: &Config,
    req: GenRequest,
    out: PathBuf,
    on_progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<GenResult, String> {
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let options = GenerateOptions {
        config: cfg,
        output_dir: out,
        control: RunControl::new(cancel_rx, Instant::now() + Duration::from_secs(30 * 60)),
        on_progress,
    };
    let result = run_with_control(req, options)
        .await
        .map_err(|error| run_error_text(error, cfg));
    drop(cancel_tx);
    result
}

pub async fn run_with_control(
    req: GenRequest,
    options: GenerateOptions<'_>,
) -> Result<GenResult, RunError> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| RunError::Request(error.to_string()))?;
    let mut context = RunContext {
        config: options.config,
        output_dir: options.output_dir,
        control: options.control,
        on_progress: options.on_progress,
        client,
        prompt_id: new_prompt_id(),
        prompt_submitted: false,
    };
    let result = run_inner(req, &mut context).await;
    if let Err(error) = &result {
        if context.prompt_submitted {
            let cleanup =
                tokio::time::timeout(Duration::from_secs(10), cancel_comfy_prompt(&context)).await;
            if !matches!(cleanup, Ok(Ok(()))) {
                return Err(match error {
                    RunError::Cancelled => RunError::CancellationUnconfirmed,
                    RunError::TimedOut => RunError::TimeoutUnconfirmed,
                    RunError::CancellationUnconfirmed => RunError::CancellationUnconfirmed,
                    RunError::TimeoutUnconfirmed => RunError::TimeoutUnconfirmed,
                    RunError::Request(message) => RunError::Request(format!(
                        "{message}; the engine may still be finishing and the next request may queue behind it."
                    )),
                });
            }
        }
    }
    result
}

fn run_error_text(error: RunError, config: &Config) -> String {
    match error {
        RunError::Cancelled => "Cancelled".to_string(),
        RunError::TimedOut => format!(
            "Generation timed out; see {}",
            config.engine_log().display()
        ),
        RunError::CancellationUnconfirmed => {
            "Cancelled; the engine may still be finishing and the next request may queue behind it."
                .to_string()
        }
        RunError::TimeoutUnconfirmed => format!(
            "Generation timed out; the engine may still be finishing. See {}",
            config.engine_log().display()
        ),
        RunError::Request(message) => message,
    }
}

async fn cancel_comfy_prompt(context: &RunContext<'_>) -> Result<(), String> {
    let base = context.config.base_url();
    let delete = async {
        context
            .client
            .post(format!("{base}/queue"))
            .json(&queue_delete_payload(&context.prompt_id))
            .send()
            .await
            .map_err(|error| error.to_string())
            .and_then(|response| {
                if response.status().is_success() {
                    Ok(())
                } else {
                    Err(format!(
                        "queue cancellation returned HTTP {}",
                        response.status()
                    ))
                }
            })
    };
    let interrupt = async {
        context
            .client
            .post(format!("{base}/interrupt"))
            .json(&interrupt_payload(&context.prompt_id))
            .send()
            .await
            .map_err(|error| error.to_string())
            .and_then(|response| {
                if response.status().is_success() {
                    Ok(())
                } else {
                    Err(format!(
                        "prompt interruption returned HTTP {}",
                        response.status()
                    ))
                }
            })
    };
    let (queued, interrupted) = tokio::join!(delete, interrupt);
    match (queued, interrupted) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(queue_error), Ok(())) => Err(queue_error),
        (Ok(()), Err(interrupt_error)) => Err(interrupt_error),
        (Err(queue_error), Err(interrupt_error)) => {
            Err(format!("{queue_error}; {interrupt_error}"))
        }
    }
}

async fn remove_temp_file(path: &Path) -> Result<(), String> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

async fn run_inner(req: GenRequest, context: &mut RunContext<'_>) -> Result<GenResult, RunError> {
    let cfg = context.config;
    let out = &context.output_dir;
    let client = &context.client;
    let prompt_id = &context.prompt_id;
    let on_progress = context.on_progress;
    let control = &mut context.control;
    let t0 = Instant::now();
    let p = preset(&req.preset);
    let base = cfg.base_url();
    let emit = |stage, value, max| on_progress(Progress { stage, value, max });

    let tag = format!("{:08x}", rand::thread_rng().gen::<u32>());
    let refs_in: Vec<&String> = req.images.iter().take(MAX_REFS).collect();
    let mut refs = Vec::with_capacity(refs_in.len());
    for (i, path) in refs_in.iter().enumerate() {
        emit("upload", i as u32, refs_in.len() as u32);
        let uploaded = control
            .race(upload(cfg, client, Path::new(path), &tag, i + 1))
            .await?;
        refs.push(uploaded);
    }

    let prompt = if req.transparent && refs.is_empty() {
        wrap_transparent(&req.prompt)
    } else {
        req.prompt.clone()
    };
    let (w, h) = dims(&req.aspect, p.megapixels);
    let seed = req
        .seed
        .unwrap_or_else(|| rand::thread_rng().gen_range(0..(1u64 << 48)));
    let graph = build_graph(
        &prompt,
        &refs,
        w,
        h,
        p.steps,
        seed,
        p.edit_res,
        cfg.profile.encoder().name,
    );

    // Open the WebSocket before queueing so the first events are not lost.
    let client_id = format!("qil-{tag}");
    let ws_url = cfg.ws_url(&client_id);
    let (ws, _) = control
        .race(async {
            tokio::time::timeout(
                Duration::from_secs(20),
                tokio_tungstenite::connect_async(ws_url),
            )
            .await
            .map_err(|_| "WebSocket connection timed out".to_string())?
            .map_err(|error| format!("Cannot reach the engine: {error}"))
        })
        .await?;
    let (_, mut rx) = ws.split();

    context.prompt_submitted = true;
    let prompt_url = format!("{base}/prompt");
    let prompt_body = json!({"prompt": graph, "client_id": client_id, "prompt_id": prompt_id});
    let res = control
        .race(async {
            client
                .post(prompt_url)
                .json(&prompt_body)
                .send()
                .await
                .map_err(|error| error.to_string())
        })
        .await?;
    let ok = res.status().is_success();
    let body: Value = control
        .race(async { res.json().await.map_err(|error| error.to_string()) })
        .await?;
    if !ok {
        return Err(node_error_text(&body).into());
    }
    emit("load", 0, 0);

    let mut sample_start: Option<Instant> = None;
    let mut sample_secs = 0.0;
    loop {
        let received = control
            .race(async {
                Ok(tokio::time::timeout(Duration::from_secs(30), rx.next())
                    .await
                    .ok())
            })
            .await?;
        let Some(Some(Ok(msg))) = received else { break };
        if !msg.is_text() {
            continue; // binary previews
        }
        let Ok(v) = serde_json::from_str::<Value>(msg.to_text().unwrap_or("")) else {
            continue;
        };
        let data = &v["data"];
        if data["prompt_id"].as_str().is_some_and(|id| id != prompt_id) {
            continue;
        }
        match v["type"].as_str().unwrap_or("") {
            "executing" => match data["node"].as_str() {
                Some(node) => {
                    if let Some(s) = sample_start.take() {
                        sample_secs = s.elapsed().as_secs_f64();
                    }
                    if node == "sampler" {
                        sample_start = Some(Instant::now());
                    }
                    emit(stage_for(node), 0, 0)
                }
                None => break, // finished
            },
            "progress" => {
                let (val, max) = (
                    data["value"].as_u64().unwrap_or(0) as u32,
                    data["max"].as_u64().unwrap_or(0) as u32,
                );
                emit("sample", val, max);
            }
            "execution_success" => break,
            "execution_interrupted" => return Err(RunError::Cancelled),
            "execution_error" => {
                let m = data["exception_message"]
                    .as_str()
                    .unwrap_or("Engine error")
                    .trim()
                    .to_string();
                let hint = if m.contains("out of memory") || m.contains("OutOfMemory") {
                    " — out of VRAM: close other apps using the GPU, lower the quality preset, or switch to the compact profile."
                } else {
                    ""
                };
                return Err(RunError::Request(format!("{m}{hint}")));
            }
            _ => {}
        }
    }

    // The WebSocket can drop before the prompt finishes; /history is the source of truth,
    // and it only holds the entry once execution is over.
    let entry = loop {
        let history_url = format!("{base}/history/{prompt_id}");
        let hist: Value = control
            .race(async {
                client
                    .get(history_url)
                    .send()
                    .await
                    .map_err(|error| error.to_string())?
                    .json()
                    .await
                    .map_err(|error| error.to_string())
            })
            .await?;
        let Some(entry) = hist.get(prompt_id) else {
            control
                .race(async {
                    tokio::time::sleep(Duration::from_millis(750)).await;
                    Ok(())
                })
                .await?;
            continue;
        };
        if entry["status"]["completed"].as_bool() == Some(true)
            || entry["status"]["status_str"].as_str() == Some("error")
        {
            break entry.clone();
        }
        control
            .race(async {
                tokio::time::sleep(Duration::from_millis(750)).await;
                Ok(())
            })
            .await?;
    };
    emit("save", 0, 0);
    let entry = &entry;
    if entry["status"]["status_str"].as_str() == Some("error") {
        return Err(format!(
            "The engine finished with an error; see {}",
            cfg.engine_log().display()
        )
        .into());
    }
    let img = output_image(entry)?;
    let filename = img["filename"]
        .as_str()
        .ok_or("The engine image output had no filename")?;
    let subfolder = img["subfolder"].as_str().unwrap_or("").to_string();
    let image_type = img["type"].as_str().unwrap_or("output").to_string();
    let view_url = format!("{base}/view");
    let response = control
        .race(async {
            client
                .get(view_url)
                .query(&[
                    ("filename", filename),
                    ("subfolder", &subfolder),
                    ("type", &image_type),
                ])
                .send()
                .await
                .map_err(|error| format!("Failed to download image {filename}: {error}"))
        })
        .await?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download image {filename}: HTTP {}",
            response.status()
        )
        .into());
    }
    let bytes = control
        .race(async {
            response
                .bytes()
                .await
                .map_err(|error| format!("Failed to read image {filename}: {error}"))
        })
        .await?;

    let (width, height) = image::load_from_memory(&bytes)
        .map(|i| (i.width(), i.height()))
        .unwrap_or((w, h));
    let proposed_file = if out
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        out.to_path_buf()
    } else {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        out.join(format!("{stamp}_{}_{seed}.png", req.mode))
    };
    let file = unique_output_path(&proposed_file)?;
    if let Some(dir) = file.parent() {
        let directory = dir.to_path_buf();
        control
            .race(async {
                tokio::fs::create_dir_all(directory)
                    .await
                    .map_err(|error| error.to_string())
            })
            .await?;
    }
    let png = embed_generation_metadata(
        &bytes,
        &GenerationMetadata {
            prompt: &req.prompt,
            seed,
            steps: p.steps,
            width,
            height,
            preset: &req.preset,
        },
    )?;
    let final_name = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "image.png".to_string());
    let temporary_file = file.with_file_name(format!(".{final_name}.{tag}.part"));
    if let Err(write_error) = tokio::fs::write(&temporary_file, png).await {
        return Err(match remove_temp_file(&temporary_file).await {
            Ok(()) => RunError::Request(write_error.to_string()),
            Err(cleanup_error) => RunError::Request(format!(
                "{write_error}; could not remove temporary image: {cleanup_error}"
            )),
        });
    }
    if let Err(error) = control.race(async { Ok(()) }).await {
        return match remove_temp_file(&temporary_file).await {
            Ok(()) => Err(error),
            Err(cleanup_error) => Err(RunError::Request(format!(
                "{}; could not remove temporary image: {cleanup_error}",
                run_error_text(error, cfg)
            ))),
        };
    }
    if let Err(rename_error) = tokio::fs::rename(&temporary_file, &file).await {
        return Err(match remove_temp_file(&temporary_file).await {
            Ok(()) => RunError::Request(format!("Could not publish generated image: {rename_error}")),
            Err(cleanup_error) => RunError::Request(format!(
                "Could not publish generated image: {rename_error}; could not remove temporary image: {cleanup_error}"
            )),
        });
    }

    let result = GenResult {
        path: file.to_string_lossy().into_owned(),
        seed,
        width,
        height,
        seconds: t0.elapsed().as_secs_f64(),
        sample_seconds: sample_secs,
        steps: p.steps,
        prompt: req.prompt.clone(),
        mode: req.mode.clone(),
        preset: req.preset.clone(),
    };
    let round1 = |x: f64| (x * 10.0).round() / 10.0;
    let meta = json!({
        "prompt": req.prompt, "mode": req.mode, "preset": req.preset, "aspect": req.aspect,
        "seed": seed, "steps": p.steps, "width": width, "height": height, "transparent": req.transparent,
        "references": req.images, "seconds": round1(result.seconds), "sampleSeconds": round1(sample_secs),
        "model": "Qwen-Image-2.1", "dit": DIT.name, "encoder": cfg.profile.encoder().name,
        "app": concat!("PicaLum QwenImage ", env!("CARGO_PKG_VERSION")),
        "created": chrono::Local::now().to_rfc3339(),
    });
    if should_save_json(cfg.save_json) {
        let _ = tokio::fs::write(
            file.with_extension("json"),
            serde_json::to_vec_pretty(&meta).unwrap(),
        )
        .await;
    }
    if let Some(preview) = local_preview_path(cfg, img) {
        let _ = tokio::fs::remove_file(preview).await;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_prompt_id_is_a_valid_uuid_v4() {
        let id = new_prompt_id();
        let parts: Vec<&str> = id.split('-').collect();

        assert_eq!(parts.len(), 5);
        assert_eq!(parts.iter().map(|part| part.len()).sum::<usize>(), 32);
        assert_eq!(parts[2].chars().next(), Some('4'));
        assert!(matches!(
            parts[3].chars().next(),
            Some('8' | '9' | 'a' | 'b')
        ));
    }

    #[test]
    fn output_path_adds_numeric_suffix_on_collision() {
        let root =
            std::env::temp_dir().join(format!("qil-output-{:x}", rand::thread_rng().gen::<u64>()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("20261009_photos_17.png");
        std::fs::write(&path, b"first").unwrap();

        let next = unique_output_path(&path).unwrap();

        assert_eq!(next.file_name().unwrap(), "20261009_photos_17 (1).png");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn targeted_cancel_payload_contains_only_the_requested_prompt_id() {
        let id = "48f2a1de-f6b9-4d13-831e-c6a46d3263a1";

        let delete = queue_delete_payload(id);
        let interrupt = interrupt_payload(id);

        assert_eq!(delete, json!({"delete": [id]}));
        assert_eq!(interrupt, json!({"prompt_id": id}));
    }

    #[test]
    fn dims_snap_to_32_and_respect_aspect() {
        assert_eq!(dims("1:1", 1.048_576), (1024, 1024));
        let (w, h) = dims("16:9", 1.6);
        assert!(w % 32 == 0 && h % 32 == 0 && w > h);
        assert_eq!(dims("16:9", 4.0), (2752, 1536));
    }

    #[test]
    fn edit_graph_wires_all_refs() {
        let refs: Vec<String> = (1..=3).map(|i| format!("r{i}.png")).collect();
        let g = build_graph("x", &refs, 0, 0, 20, 1, 1024, "enc.safetensors");
        assert_eq!(g["enc"]["inputs"]["images.image_3"], json!(["load3", 0]));
        assert_eq!(g["sampler"]["inputs"]["latent_image"], json!(["enc", 2]));
        assert_eq!(g["clip"]["inputs"]["clip_name"], json!("enc.safetensors"));
        assert!(g.get("latent").is_none());
    }

    #[test]
    fn graph_uses_temporary_preview_node() {
        let graph = build_graph("x", &[], 1024, 1024, 20, 1, 1024, "enc.safetensors");
        assert_eq!(graph["save"]["class_type"], "PreviewImage");
        assert!(graph["save"]["inputs"].get("filename_prefix").is_none());
    }

    #[test]
    fn json_policy_is_respected() {
        assert!(should_save_json(true));
        assert!(!should_save_json(false));
    }

    #[test]
    fn preview_path_accepts_only_safe_temp_results() {
        let cfg = Config::default();
        let valid = json!({"type":"temp", "subfolder":"qil", "filename":"preview.png"});
        assert_eq!(
            local_preview_path(&cfg, &valid),
            Some(cfg.comfy_dir().join("temp/qil/preview.png"))
        );
        for invalid in [
            json!({"type":"output", "filename":"preview.png"}),
            json!({"type":"temp", "filename":r"C:\outside.png"}),
            json!({"type":"temp", "filename":"C:outside.png"}),
            json!({"type":"temp", "subfolder":"../outside", "filename":"preview.png"}),
            json!({"type":"temp", "filename":"sub/preview.png"}),
            json!({"type":"temp", "filename":r"sub\preview.png"}),
        ] {
            assert_eq!(local_preview_path(&cfg, &invalid), None);
        }
    }

    #[test]
    fn transparent_wrapper() {
        assert!(wrap_transparent("a sticker.")
            .contains("RGBA format image with transparency. a sticker. The image"));
    }

    #[test]
    fn output_image_reads_nested_save_image_output() {
        let entry = json!({
            "outputs": {
                "save": {
                    "meta": {"node_id": "save"},
                    "output": {
                        "images": [{"filename": "ComfyUI_00001_.png", "subfolder": "", "type": "output"}]
                    }
                }
            }
        });

        let image = output_image(&entry).expect("nested SaveImage output");
        assert_eq!(image["filename"], "ComfyUI_00001_.png");
    }

    #[test]
    fn output_image_reads_legacy_direct_output() {
        let entry = json!({
            "outputs": {
                "save": {
                    "images": [{"filename": "legacy.png"}]
                }
            }
        });

        assert_eq!(output_image(&entry).unwrap()["filename"], "legacy.png");
    }

    #[test]
    fn output_image_rejects_missing_outputs() {
        let error = output_image(&json!({})).unwrap_err();
        assert!(error.contains("no usable image metadata"));
    }

    #[test]
    fn output_image_rejects_empty_images() {
        let error =
            output_image(&json!({"outputs": {"save": {"output": {"images": []}}}})).unwrap_err();
        assert!(error.contains("no usable image metadata"));
    }

    #[test]
    fn output_image_rejects_missing_or_empty_filename() {
        for image in [json!({}), json!({"filename": ""})] {
            let entry = json!({"outputs": {"save": {"output": {"images": [image]}}}});
            let error = output_image(&entry).unwrap_err();
            assert!(error.contains("no filename"));
        }
    }
}
