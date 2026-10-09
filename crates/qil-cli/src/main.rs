//! qil — run Qwen-Image-2.1 locally on an 8–12 GB NVIDIA GPU.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use qil_core::download::human_bytes;
use qil_core::{config, generate, gpu, setup, surprise, Config, Engine, GenRequest, Profile};
use serde_json::{json, Value};

#[derive(Parser)]
#[command(
    name = "qil",
    version,
    about = "Qwen-Image-2.1 on small GPUs: text-to-image, photo editing and benchmarks, fully in VRAM"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check GPU, driver, disk and what is installed
    Doctor,
    /// Install the runtime and download the weights (~17 GB, resumable)
    Setup {
        /// balanced (12 GB+) or compact (~10 GB, experimental). Default: picked from your VRAM
        #[arg(long)]
        profile: Option<String>,
        /// Where to put the models (another disk is fine, or a folder that already has them)
        #[arg(long)]
        models_dir: Option<PathBuf>,
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
    },
    /// Generate an image from text
    Generate {
        prompt: String,
        #[command(flatten)]
        opts: GenOpts,
        /// Transparent background (native RGBA)
        #[arg(long)]
        transparent: bool,
    },
    /// Edit or combine up to 10 photos with an instruction (the first one is the canvas)
    Edit {
        instruction: String,
        #[arg(short, long = "image", required = true, num_args = 1)]
        images: Vec<PathBuf>,
        #[command(flatten)]
        opts: GenOpts,
    },
    /// Let the app invent the idea
    Surprise {
        /// Optional theme, e.g. "capybaras"
        #[arg(long)]
        theme: Option<String>,
        #[command(flatten)]
        opts: GenOpts,
    },
    /// Run a benchmark suite (bench/suite.json) and record time and peak VRAM
    Bench {
        #[arg(long, default_value = "bench/suite.json")]
        suite: PathBuf,
        /// Output folder for images and results.json
        #[arg(long, default_value = "bench/results/qwen")]
        out: PathBuf,
        /// Folder with the input photos (default: <suite folder>/inputs)
        #[arg(long)]
        inputs: Option<PathBuf>,
        /// Only these test ids
        #[arg(long)]
        only: Vec<String>,
        /// Quality preset for every test (the published results use max)
        #[arg(long, default_value = "max")]
        preset: String,
        #[arg(long, default_value_t = 20260923)]
        seed: u64,
        /// Re-run tests that already have a result
        #[arg(long)]
        force: bool,
        #[arg(long)]
        keep_engine: bool,
    },
    /// Show or change settings
    Config {
        #[arg(long)]
        profile: Option<String>,
        #[arg(long)]
        models_dir: Option<PathBuf>,
        #[arg(long)]
        output_dir: Option<PathBuf>,
    },
}

#[derive(clap::Args)]
struct GenOpts {
    /// draft (1024², 20 steps) · standard (1.6 MP, 30) · max (native 2K, 40)
    #[arg(short, long, default_value = "standard")]
    preset: String,
    /// 1:1 4:3 3:4 3:2 2:3 16:9 9:16
    #[arg(short, long, default_value = "1:1")]
    aspect: String,
    #[arg(long)]
    seed: Option<u64>,
    /// Output .png file or folder (default: configured output folder)
    #[arg(short, long)]
    out: Option<PathBuf>,
    /// Leave the engine running afterwards (faster next call)
    #[arg(long)]
    keep_engine: bool,
}

fn main() {
    let cli = Cli::parse();
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    let code = match rt.block_on(run(cli)) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    };
    std::process::exit(code);
}

async fn run(cli: Cli) -> Result<(), String> {
    let mut cfg = Config::load();
    match cli.cmd {
        Cmd::Doctor => doctor(&cfg),
        Cmd::Setup {
            profile,
            models_dir,
            runtime_dir,
        } => {
            if let Some(d) = runtime_dir {
                cfg.runtime_dir = d;
                cfg.models_dir = cfg.runtime_dir.join("models");
            }
            if let Some(d) = models_dir {
                cfg.models_dir = d;
            }
            cfg.profile = match profile {
                Some(p) => Profile::parse(&p).ok_or("profile must be balanced or compact")?,
                None => gpu::detect()
                    .map(|g| Profile::for_vram(g.total_mb))
                    .unwrap_or_default(),
            };
            run_setup(cfg).await
        }
        Cmd::Generate {
            prompt,
            opts,
            transparent,
        } => {
            let req = request("text", prompt, vec![], &opts, transparent);
            one_shot(&cfg, req, &opts).await
        }
        Cmd::Edit {
            instruction,
            images,
            opts,
        } => {
            if images.len() > generate::MAX_REFS {
                return Err(format!("at most {} images", generate::MAX_REFS));
            }
            let imgs = images
                .iter()
                .map(|p| {
                    p.canonicalize()
                        .map(|p| p.to_string_lossy().trim_start_matches(r"\\?\").to_string())
                        .map_err(|e| format!("{}: {e}", p.display()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            let req = request("photos", instruction, imgs, &opts, false);
            one_shot(&cfg, req, &opts).await
        }
        Cmd::Surprise { theme, opts } => {
            let idea = surprise::idea(theme.as_deref());
            println!("idea: {}\naspect: {}", idea.prompt, idea.aspect);
            let mut req = request("surprise", idea.prompt, vec![], &opts, false);
            req.aspect = idea.aspect;
            one_shot(&cfg, req, &opts).await
        }
        Cmd::Bench {
            suite,
            out,
            inputs,
            only,
            preset,
            seed,
            force,
            keep_engine,
        } => {
            let inputs =
                inputs.unwrap_or_else(|| suite.parent().unwrap_or(Path::new(".")).join("inputs"));
            bench(
                &cfg,
                &suite,
                &out,
                &inputs,
                &only,
                &preset,
                seed,
                force,
                keep_engine,
            )
            .await
        }
        Cmd::Config {
            profile,
            models_dir,
            output_dir,
        } => {
            let mut changed = false;
            if let Some(p) = profile {
                cfg.profile = Profile::parse(&p).ok_or("profile must be balanced or compact")?;
                changed = true;
            }
            if let Some(d) = models_dir {
                cfg.models_dir = d;
                changed = true;
            }
            if let Some(d) = output_dir {
                cfg.output_dir = d;
                changed = true;
            }
            if changed {
                cfg.save().map_err(|e| e.to_string())?;
                println!("saved {}", config::config_path().display());
            }
            println!("{}", serde_json::to_string_pretty(&cfg).unwrap());
            Ok(())
        }
    }
}

fn request(
    mode: &str,
    prompt: String,
    images: Vec<String>,
    o: &GenOpts,
    transparent: bool,
) -> GenRequest {
    GenRequest {
        mode: mode.into(),
        prompt,
        images,
        preset: o.preset.clone(),
        aspect: o.aspect.clone(),
        seed: o.seed,
        transparent,
    }
}

fn select_output(cfg: &Config, explicit: Option<PathBuf>) -> PathBuf {
    explicit.unwrap_or_else(|| cfg.output_dir.clone())
}

fn doctor(cfg: &Config) -> Result<(), String> {
    let st = setup::status(cfg);
    match &st.gpu {
        Some(g) => println!(
            "GPU        {} · {:.1} GB VRAM · driver {} ({})",
            g.short_name(),
            g.total_mb as f64 / 1024.0,
            g.driver,
            if g.driver_major() >= config::MIN_DRIVER_MAJOR {
                "ok"
            } else {
                "too old"
            }
        ),
        None => println!("GPU        no NVIDIA GPU found"),
    }
    println!(
        "profile    {} ({})",
        serde_json::to_value(cfg.profile).unwrap().as_str().unwrap(),
        cfg.profile.label()
    );
    if let Some(g) = &st.gpu {
        let rec = Profile::for_vram(g.total_mb);
        if rec != cfg.profile {
            println!("           recommended for your card: {}", rec.label());
        }
    }
    println!("runtime    {}", cfg.runtime_dir.display());
    println!("models     {}", cfg.models_dir.display());
    println!("output     {}", cfg.output_dir.display());
    if let Some(f) = st.free_bytes {
        println!("free disk  {}", human_bytes(f));
    }
    println!();
    for s in &st.steps {
        let extra = if s.done {
            String::new()
        } else if s.download_bytes > 0 {
            format!("  (download {})", human_bytes(s.download_bytes))
        } else {
            String::new()
        };
        println!("  [{}] {}{extra}", if s.done { "x" } else { " " }, s.label);
    }
    println!();
    for p in &st.problems {
        println!("! {p}");
    }
    if st.ready {
        println!("Ready. Try: qil generate \"a red fox asleep in the snow\" --preset draft");
    } else {
        println!(
            "Not installed yet: run `qil setup` (about {} to download).",
            human_bytes(st.download_bytes)
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configured_output_is_default_and_explicit_destination_wins() {
        let mut cfg = Config::default();
        cfg.output_dir = PathBuf::from(r"D:\Art\Qwen");
        assert_eq!(select_output(&cfg, None), cfg.output_dir);
        let explicit = PathBuf::from(r"E:\one.png");
        assert_eq!(select_output(&cfg, Some(explicit.clone())), explicit);
    }
}

async fn run_setup(cfg: Config) -> Result<(), String> {
    let st = setup::status(&cfg);
    println!("profile: {}", cfg.profile.label());
    println!(
        "runtime: {}\nmodels:  {}",
        cfg.runtime_dir.display(),
        cfg.models_dir.display()
    );
    println!("to download: {}", human_bytes(st.download_bytes));
    for p in &st.problems {
        println!("! {p}");
    }
    if st.problems.iter().any(|p| p.starts_with("Not enough disk")) {
        return Err("free some disk space or pass --models-dir on another drive".into());
    }
    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template(
            "  {bar:32} {bytes}/{total_bytes} · {bytes_per_sec} · eta {eta}",
        )
        .unwrap()
        .progress_chars("━╸ "),
    );
    bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
    let verbose = std::env::var("QIL_VERBOSE").is_ok();
    tokio::task::spawn_blocking(move || {
        setup::run(&cfg, &mut |ev| match ev {
            setup::Event::Step { id, state, detail } => {
                bar.set_draw_target(indicatif::ProgressDrawTarget::hidden());
                let mark = match state {
                    "running" => "…",
                    "done" => "✓",
                    "skipped" => "·",
                    _ => "✗",
                };
                println!("{mark} {id} {detail}");
            }
            setup::Event::Progress { done, total, .. } => {
                if bar.is_hidden() {
                    bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
                    bar.reset();
                }
                bar.set_length(total.max(done));
                bar.set_position(done);
            }
            setup::Event::Log { line } => {
                if verbose {
                    bar.suspend(|| println!("    {line}"));
                }
            }
        })
    })
    .await
    .map_err(|e| e.to_string())??;
    println!("✓ ready. Try: qil generate \"a red fox asleep in the snow\" --preset draft");
    Ok(())
}

async fn ensure_engine(cfg: &Config, engine: &Engine) -> Result<(), String> {
    let quiet = |_s: qil_core::EngineState| {};
    if !qil_core::engine::is_up(cfg).await {
        eprintln!("starting engine…");
    }
    engine.start(cfg, &quiet).await
}

fn progress_bar() -> ProgressBar {
    let bar = ProgressBar::new(0);
    bar.set_style(
        ProgressStyle::with_template("  {msg:<24} {bar:28} {pos}/{len} · {elapsed}")
            .unwrap()
            .progress_chars("━╸ "),
    );
    bar
}

async fn one_shot(cfg: &Config, req: GenRequest, opts: &GenOpts) -> Result<(), String> {
    let engine = Engine::default();
    ensure_engine(cfg, &engine).await?;
    let bar = progress_bar();
    let b = bar.clone();
    let on = move |p: qil_core::Progress| {
        b.set_message(stage_label(p.stage));
        if p.max > 0 {
            b.set_length(p.max as u64);
            b.set_position(p.value as u64);
        }
    };
    let out = select_output(cfg, opts.out.clone());
    let res = generate::run(cfg, req, out, &on).await;
    bar.finish_and_clear();
    let r = res?;
    println!(
        "{}  {}×{} · seed {} · {:.0} s ({:.1} s/step)",
        r.path,
        r.width,
        r.height,
        r.seed,
        r.seconds,
        r.sample_seconds / r.steps.max(1) as f64
    );
    if opts.keep_engine && engine.owns_process() {
        std::mem::forget(engine); // leave it running; the next call attaches to it
        eprintln!("engine left running on port {}", cfg.port);
    }
    Ok(())
}

fn stage_label(s: &str) -> &'static str {
    match s {
        "upload" => "uploading references",
        "load" => "loading models",
        "encode" => "reading prompt",
        "sample" => "sampling",
        "decode" => "decoding",
        _ => "saving",
    }
}

/// Polls nvidia-smi and keeps the peak used VRAM while `running` is set.
fn vram_sampler(running: Arc<AtomicBool>, peak: Arc<AtomicU32>) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        while running.load(Ordering::Relaxed) {
            if let Some(g) = gpu::detect() {
                peak.fetch_max(g.used_mb, Ordering::Relaxed);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    })
}

#[allow(clippy::too_many_arguments)]
async fn bench(
    cfg: &Config,
    suite: &Path,
    out: &Path,
    inputs: &Path,
    only: &[String],
    preset: &str,
    seed: u64,
    force: bool,
    keep: bool,
) -> Result<(), String> {
    let tests: Vec<Value> = serde_json::from_slice(
        &std::fs::read(suite).map_err(|e| format!("{}: {e}", suite.display()))?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let res_path = out.join("results.json");
    let mut results: serde_json::Map<String, Value> = std::fs::read(&res_path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();

    let engine = Engine::default();
    ensure_engine(cfg, &engine).await?;
    let g = gpu::detect();
    let baseline = g.as_ref().map(|g| g.used_mb).unwrap_or(0);
    println!(
        "bench: {} tests · preset {preset} · seed {seed} · {} · profile {}",
        tests.len(),
        g.as_ref()
            .map(|g| g.short_name())
            .unwrap_or_else(|| "no GPU".into()),
        cfg.profile.label()
    );

    let bar = progress_bar();
    let shared_bar = Arc::new(Mutex::new(bar.clone()));
    for t in &tests {
        let id = t["id"].as_str().unwrap_or("?").to_string();
        if (!only.is_empty() && !only.contains(&id))
            || (!force
                && only.is_empty()
                && results.get(&id).is_some_and(|r| r.get("seconds").is_some()))
        {
            continue;
        }
        let edit = t["mode"].as_str() == Some("edit");
        let images = if edit {
            t["images"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .map(|f| inputs.join(f).to_string_lossy().into_owned())
                .collect()
        } else {
            vec![]
        };
        let req = GenRequest {
            mode: "bench".into(),
            prompt: t["prompt"].as_str().unwrap_or_default().to_string(),
            images,
            preset: preset.into(),
            aspect: t["ar"].as_str().unwrap_or("1:1").into(),
            seed: Some(seed),
            transparent: false, // suite prompts already carry the RGBA wording where needed
        };
        let running = Arc::new(AtomicBool::new(true));
        let peak = Arc::new(AtomicU32::new(0));
        let sampler = vram_sampler(running.clone(), peak.clone());
        let b = shared_bar.clone();
        let label = id.clone();
        let on = move |p: qil_core::Progress| {
            let b = b.lock().unwrap();
            b.set_message(format!("{label} · {}", stage_label(p.stage)));
            if p.max > 0 {
                b.set_length(p.max as u64);
                b.set_position(p.value as u64);
            }
        };
        let r = generate::run(cfg, req, out.join(format!("{id}.png")), &on).await;
        running.store(false, Ordering::Relaxed);
        let _ = sampler.join();
        bar.suspend(|| match &r {
            Ok(r) => println!(
                "✓ {id:<26} {:>4.0} s · {:.2} s/step · peak VRAM {:.1} GB",
                r.seconds,
                r.sample_seconds / r.steps as f64,
                peak.load(Ordering::Relaxed) as f64 / 1024.0
            ),
            Err(e) => println!("✗ {id:<26} {e}"),
        });
        results.insert(
            id,
            match r {
                Ok(r) => json!({
                    "seconds": (r.seconds * 10.0).round() / 10.0,
                    "sampleSeconds": (r.sample_seconds * 10.0).round() / 10.0,
                    "secondsPerStep": ((r.sample_seconds / r.steps as f64) * 100.0).round() / 100.0,
                    "steps": r.steps, "width": r.width, "height": r.height, "seed": r.seed,
                    "peakVramMb": peak.load(Ordering::Relaxed), "baselineVramMb": baseline,
                    "preset": preset, "profile": cfg.profile, "gpu": g.as_ref().map(|g| g.short_name()),
                }),
                Err(e) => json!({"error": e}),
            },
        );
        std::fs::write(&res_path, serde_json::to_vec_pretty(&results).unwrap())
            .map_err(|e| e.to_string())?;
    }
    bar.finish_and_clear();
    println!("results: {}", res_path.display());
    if keep && engine.owns_process() {
        std::mem::forget(engine);
    }
    Ok(())
}
