//! Generation: builds the ComfyUI graph, queues it, follows progress over the
//! WebSocket, embeds interoperable PNG metadata, and optionally writes a JSON sidecar.

use std::path::{Path, PathBuf};
use std::time::Instant;

use futures_util::StreamExt;
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::{Config, DIT, VAE};
use crate::png_metadata::{embed_generation_metadata, GenerationMetadata};

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

/// Runs one generation. `out` is a folder (a timestamped name is picked) or an exact `.png` path.
pub async fn run(
    cfg: &Config,
    req: GenRequest,
    out: PathBuf,
    on_progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<GenResult, String> {
    let t0 = Instant::now();
    let p = preset(&req.preset);
    let client = reqwest::Client::new();
    let base = cfg.base_url();
    let emit = |stage, value, max| on_progress(Progress { stage, value, max });

    let tag = format!("{:08x}", rand::thread_rng().gen::<u32>());
    let refs_in: Vec<&String> = req.images.iter().take(MAX_REFS).collect();
    let mut refs = Vec::with_capacity(refs_in.len());
    for (i, path) in refs_in.iter().enumerate() {
        emit("upload", i as u32, refs_in.len() as u32);
        refs.push(upload(cfg, &client, Path::new(path), &tag, i + 1).await?);
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
    let (ws, _) = tokio_tungstenite::connect_async(cfg.ws_url(&client_id))
        .await
        .map_err(|e| format!("Cannot reach the engine: {e}"))?;
    let (_, mut rx) = ws.split();

    let res = client
        .post(format!("{base}/prompt"))
        .json(&json!({"prompt": graph, "client_id": client_id}))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let ok = res.status().is_success();
    let body: Value = res.json().await.map_err(|e| e.to_string())?;
    if !ok {
        return Err(node_error_text(&body));
    }
    let prompt_id = body["prompt_id"]
        .as_str()
        .ok_or("The engine returned no prompt_id")?
        .to_string();
    emit("load", 0, 0);

    let mut sample_start: Option<Instant> = None;
    let mut sample_secs = 0.0;
    while let Some(msg) = rx.next().await {
        let Ok(msg) = msg else { break };
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
            "execution_interrupted" => return Err("Cancelled".into()),
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
                return Err(format!("{m}{hint}"));
            }
            _ => {}
        }
    }

    // The WebSocket can drop before the prompt finishes; /history is the source of truth,
    // and it only holds the entry once execution is over.
    let deadline = Instant::now() + std::time::Duration::from_secs(30 * 60);
    let entry = loop {
        let hist: Value = client
            .get(format!("{base}/history/{prompt_id}"))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        let Some(entry) = hist.get(&prompt_id) else {
            if Instant::now() > deadline {
                return Err(format!(
                    "The engine returned no history for prompt {prompt_id}"
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(750)).await;
            continue;
        };
        if entry["status"]["completed"].as_bool() == Some(true)
            || entry["status"]["status_str"].as_str() == Some("error")
        {
            break entry.clone();
        }
        if Instant::now() > deadline {
            return Err(format!(
                "The engine did not finish within 30 min; see {}",
                cfg.engine_log().display()
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(750)).await;
    };
    emit("save", 0, 0);
    let entry = &entry;
    if entry["status"]["status_str"].as_str() == Some("error") {
        return Err(format!(
            "The engine finished with an error; see {}",
            cfg.engine_log().display()
        ));
    }
    let img = output_image(entry)?;
    let filename = img["filename"]
        .as_str()
        .ok_or("The engine image output had no filename")?;
    let response = client
        .get(format!("{base}/view"))
        .query(&[
            ("filename", filename),
            ("subfolder", img["subfolder"].as_str().unwrap_or("")),
            ("type", img["type"].as_str().unwrap_or("output")),
        ])
        .send()
        .await
        .map_err(|e| format!("Failed to download image {filename}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Failed to download image {filename}: HTTP {}",
            response.status()
        ));
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read image {filename}: {e}"))?;

    let (width, height) = image::load_from_memory(&bytes)
        .map(|i| (i.width(), i.height()))
        .unwrap_or((w, h));
    let file = if out
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("png"))
    {
        out
    } else {
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        out.join(format!("{stamp}_{}_{seed}.png", req.mode))
    };
    if let Some(dir) = file.parent() {
        tokio::fs::create_dir_all(dir)
            .await
            .map_err(|e| e.to_string())?;
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
    tokio::fs::write(&file, png)
        .await
        .map_err(|e| e.to_string())?;

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

pub async fn interrupt(cfg: &Config) {
    let client = reqwest::Client::new();
    let base = cfg.base_url();
    let _ = client
        .post(format!("{base}/queue"))
        .json(&json!({"clear": true}))
        .send()
        .await;
    let _ = client.post(format!("{base}/interrupt")).send().await;
}

#[cfg(test)]
mod tests {
    use super::*;

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
