//! Vulkan (GPU-agnostic) TTS worker — the `vulkan` feature's backend, a
//! drop-in for the candle `tts.rs` (same `spawn_worker` signature). Holds a
//! resident qwen3-tts-burn Engine; post-processing (leading_trim + LPF) lives
//! inside the engine. Preset (CustomVoice) speakers are not supported by the
//! burn port yet — selecting one logs and is skipped; clone voices are the path.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use qwen3_tts_burn::lang::Language;
use qwen3_tts_burn::pipeline::ClonePrompt;
use qwen3_tts_burn::sampling::SamplerCfg;
use qwen3_tts_burn::VulkanEngine as Engine;

use crate::config::{IntonationEntry, SharedConfig, VoiceEntry};
use crate::playback::StreamPlayer;

/// Upper cap on frames (≈ the largest KV-cache bucket minus prefill); the actual
/// budget per utterance scales with text length in `speak`.
const MAX_FRAMES: usize = 1800;
/// Semantic-token sampling temperature. Higher = more prosodic variation (some
/// expression) at the cost of stability; 0.9 chosen by ear.
const TEMPERATURE: f64 = 0.9;

pub fn spawn_worker(
    base_dir: String,
    _customvoice_dir: String,
    _device_str: String,
    config: SharedConfig,
    ready: Arc<AtomicBool>,
    playqueue: Arc<crate::playqueue::PlayQueue>,
) -> Sender<String> {
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || worker_loop(base_dir, config, rx, ready, playqueue));
    tx
}

fn intonation_text(entry: &IntonationEntry, audio_path: &str) -> Option<String> {
    if let Some(t) = &entry.text {
        return Some(t.trim().to_string());
    }
    let txt = std::path::Path::new(audio_path).with_extension("txt");
    std::fs::read_to_string(&txt).ok().map(|s| s.trim().to_string())
}

fn build_prompt(
    eng: &Engine,
    cache: &mut HashMap<String, ClonePrompt>,
    voice: &VoiceEntry,
    intonation: &IntonationEntry,
) -> Result<ClonePrompt, String> {
    let key = format!("{}|{}", voice.name, intonation.name);
    if let Some(p) = cache.get(&key) {
        return Ok(ClonePrompt {
            language: p.language,
            speaker_embedding: p.speaker_embedding.clone(),
            ref_codes: p.ref_codes.clone(),
            ref_text_ids: p.ref_text_ids.clone(),
        });
    }
    let prompt = match &intonation.audio_path {
        Some(icl) => {
            let text = intonation_text(intonation, icl)
                .ok_or_else(|| format!("no transcript for intonation {}", intonation.name))?;
            eng.build_clone_prompt(&voice.audio_path, icl, &text, Language::Korean)?
        }
        None => eng.build_xvector_prompt(&voice.audio_path, Language::Korean)?,
    };
    cache.insert(
        key,
        ClonePrompt {
            language: prompt.language,
            speaker_embedding: prompt.speaker_embedding.clone(),
            ref_codes: prompt.ref_codes.clone(),
            ref_text_ids: prompt.ref_text_ids.clone(),
        },
    );
    Ok(prompt)
}

fn text_seed(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// Load the engine, warm the kernels, and prewarm the configured voice — logging
/// each phase. Shared by the live worker and the installer's `--warmup` pass so
/// both populate cubecl's on-disk kernel cache identically. Returns the engine
/// plus the seeded prompt cache, or None if loading failed.
fn load_and_ready(
    _base_dir: &str,
    config: &SharedConfig,
) -> Option<(Engine, HashMap<String, ClonePrompt>)> {
    // Resolve (and, on a fresh install, download) the model dir before loading.
    let model_path = config.lock().unwrap().model_path.clone();
    let base_dir = match crate::modelsrc::resolve_base_dir(&model_path) {
        Ok(d) => d,
        Err(e) => {
            crate::applog::log(&format!("tts(vulkan): model unavailable: {e}"));
            crate::modelsrc::set_load_failed(true);
            crate::modelsrc::set_status(crate::i18n::model_load_failed(&e.to_string()));
            return None;
        }
    };
    crate::applog::log(&format!("tts(vulkan): loading engine from {base_dir}…"));
    let t_load = std::time::Instant::now();
    let eng = match qwen3_tts_burn::load_vulkan(&base_dir) {
        Ok(e) => e,
        Err(e) => {
            crate::applog::log(&format!("tts(vulkan): FAILED to load engine: {e}"));
            // Without this the GUI keeps showing the loading spinner forever.
            crate::modelsrc::set_load_failed(true);
            crate::modelsrc::set_status(crate::i18n::model_load_failed(&e.to_string()));
            return None;
        }
    };
    crate::applog::log(&format!("tts(vulkan): weights loaded {:.1}s, warming up…", t_load.elapsed().as_secs_f64()));
    let t_warm = std::time::Instant::now();
    if let Err(e) = eng.warmup() {
        crate::applog::log(&format!("tts(vulkan): warmup failed: {e}"));
    }
    crate::applog::log(&format!("tts(vulkan): warmup done {:.1}s, compiling voice…", t_warm.elapsed().as_secs_f64()));
    let mut cache: HashMap<String, ClonePrompt> = HashMap::new();
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        prewarm(&eng, config, &mut cache)
    }));
    if r.is_err() {
        crate::applog::log("tts(vulkan): prewarm PANICKED — continuing; first message will compile");
    }
    crate::applog::log("tts(vulkan): engine ready");
    Some((eng, cache))
}

/// Headless cache-warming entry (installer `--warmup`): load + compile + cache,
/// then drop everything and return. On a fresh machine this pays the one-time
/// SPIR-V compile so the user's first real launch loads from the on-disk cache.
pub fn warmup_cache(base_dir: String, config: SharedConfig) {
    crate::applog::log("tts(vulkan): --warmup: populating kernel cache…");
    let _ = load_and_ready(&base_dir, &config);
    crate::applog::log("tts(vulkan): --warmup: done");
}

fn worker_loop(
    base_dir: String,
    config: SharedConfig,
    rx: Receiver<String>,
    ready: Arc<AtomicBool>,
    playqueue: Arc<crate::playqueue::PlayQueue>,
) {
    let Some((eng, mut cache)) = load_and_ready(&base_dir, &config) else {
        return;
    };
    // Engine + kernels + voice are all warm now — let the GUI reveal itself.
    ready.store(true, Ordering::SeqCst);

    while let Ok(text) = rx.recv() {
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        playqueue.begin(&text);
        crate::playqueue::refresh_overlay(&playqueue);
        let my_epoch = playqueue.epoch();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            handle_message(&eng, &config, &mut cache, &text, &playqueue, my_epoch)
        }));
        if r.is_err() {
            crate::applog::log("tts(vulkan): message handler PANICKED — worker still alive");
        }
        playqueue.finish();
        // Barge-in: a stop during this message clears the queue, so also drop
        // anything already sitting in the channel from before the stop.
        if playqueue.cancelled(my_epoch) {
            while rx.try_recv().is_ok() {}
        }
        crate::playqueue::refresh_overlay(&playqueue);
    }
}

fn handle_message(
    eng: &Engine,
    config: &SharedConfig,
    cache: &mut HashMap<String, ClonePrompt>,
    text: &str,
    playqueue: &crate::playqueue::PlayQueue,
    my_epoch: u64,
) {
    let (voice, intonation) = {
        let c = config.lock().unwrap();
        (
            c.find_voice(&c.voice).cloned(),
            c.find_intonation(&c.intonation).cloned(),
        )
    };
    let Some(voice) = voice else {
        crate::applog::log("tts(vulkan): no voice configured");
        return;
    };
    if voice.is_preset() {
        crate::applog::log("tts(vulkan): preset voices unsupported on this backend; skipping");
        return;
    }
    let Some(intonation) = intonation else {
        crate::applog::log("tts(vulkan): no intonation configured");
        return;
    };

    let prompt = match build_prompt(eng, cache, &voice, &intonation) {
        Ok(p) => p,
        Err(e) => {
            crate::applog::log(&format!("tts(vulkan): prompt build failed: {e}"));
            return;
        }
    };
    crate::applog::log(&format!(
        "tts(vulkan): speaking {text:?} [{}/{}]",
        voice.name, intonation.name
    ));
    let t0 = std::time::Instant::now();
    match speak(eng, text, &prompt, playqueue, my_epoch) {
        Ok(played) if played > 0.0 => crate::applog::log(&format!(
            "tts(vulkan): done {played:.2}s in {:.2}s",
            t0.elapsed().as_secs_f64()
        )),
        Ok(_) => crate::applog::log("tts(vulkan): empty synthesis"),
        Err(e) => crate::applog::log(&format!("tts(vulkan): synth/playback failed: {e}")),
    }
}

/// warmup() only compiles the talker+code-predictor+decoder path (empty
/// prompt). The ECAPA speaker encoder, speech encoder and the ICL-conditioned
/// decode loop stay uncompiled until the first real clone prompt — a 30-60 s
/// cold SPIR-V compile that would otherwise land mid-utterance with no log, so
/// the app looks frozen and the user restarts. Do that compile here, during
/// load, using the configured voice (also seeding the prompt cache).
fn prewarm(eng: &Engine, config: &SharedConfig, cache: &mut HashMap<String, ClonePrompt>) {
    let (voice, intonation) = {
        let c = config.lock().unwrap();
        (
            c.find_voice(&c.voice).cloned(),
            c.find_intonation(&c.intonation).cloned(),
        )
    };
    let (Some(voice), Some(intonation)) = (voice, intonation) else {
        crate::applog::log("tts(vulkan): prewarm skipped (no voice/intonation)");
        return;
    };
    if voice.is_preset() {
        crate::applog::log("tts(vulkan): prewarm skipped (preset voice)");
        return;
    }
    let t = std::time::Instant::now();
    crate::applog::log(&format!("tts(vulkan): prewarm build_prompt [{}/{}]…", voice.name, intonation.name));
    match build_prompt(eng, cache, &voice, &intonation) {
        Ok(prompt) => {
            crate::applog::log(&format!(
                "tts(vulkan): prewarm prompt built {:.1}s ({} ref frames), test synth…",
                t.elapsed().as_secs_f64(),
                prompt.ref_codes.len()
            ));
            let ts = std::time::Instant::now();
            match eng.synthesize("네.", &prompt, SamplerCfg::app(), 24) {
                Ok(w) => crate::applog::log(&format!(
                    "tts(vulkan): warmed clone path (synth {:.1}s, {} samples)",
                    ts.elapsed().as_secs_f64(),
                    w.len()
                )),
                Err(e) => crate::applog::log(&format!("tts(vulkan): prewarm synth failed: {e}")),
            }
        }
        Err(e) => crate::applog::log(&format!("tts(vulkan): prewarm build_prompt failed: {e}")),
    }
}

/// Speak the whole utterance as ONE continuous generation — a single talker
/// KV-cache = one voice, no chunk boundaries (the user rejected chunking: the
/// register jumps + gaps at chunk joins were worse than a flatter single read).
/// Streamed into the CABLE player. The frame budget scales with the text length
/// so a short message uses a small (fast) KV-cache bucket and a long passage gets
/// a big enough one to run to the end instead of overflowing mid-way. Trailing
/// sentence punctuation is dropped ("?"/"." over-generate a silent tail).
fn speak(
    eng: &Engine,
    text: &str,
    prompt: &ClonePrompt,
    playqueue: &crate::playqueue::PlayQueue,
    my_epoch: u64,
) -> anyhow::Result<f64> {
    // Pronunciation normalization happens HERE (not at input) so the queue overlay
    // keeps the raw typed text while the model gets the spoken form (닭→닥).
    let spoken = crate::hangul::for_speech(text);
    if spoken != text {
        crate::applog::log(&format!("tts: g2p {} -> {}", crate::applog::redact(text), crate::applog::redact(&spoken)));
    }
    let synth = spoken
        .trim()
        .trim_end_matches(|c: char| matches!(c, '.' | '?' | '!' | '。' | '？' | '！' | '…' | ' '));
    if synth.is_empty() {
        return Ok(0.0);
    }
    // Append a comma so the model fully articulates the last phoneme then goes silent.
    let damped = format!("{synth},");
    let max_frames = (damped.chars().count() * 3 + 80).clamp(160, MAX_FRAMES);
    let mut player = StreamPlayer::new(playqueue.epoch_handle(), my_epoch, playqueue.volume_handle())?;
    let t0 = std::time::Instant::now();
    let cfg = SamplerCfg { seed: text_seed(synth), temperature: TEMPERATURE, ..SamplerCfg::app() };

    // Stream chunks as they generate so long utterances start without a full wait.
    const LEADIN_MS: usize = 120; // silence pre-roll
    let pad = vec![0.0f32; 24000 * LEADIN_MS / 1000];
    let mut total = 0usize;
    let mut logged = false;
    let post = qwen3_tts_burn::engine::PostProcess {
        damp_ending: false,
        ..qwen3_tts_burn::engine::PostProcess::app_default()
    };
    eng.synthesize_streaming(&damped, prompt, cfg, max_frames, true, post, |c| {
        if c.is_empty() || playqueue.cancelled(my_epoch) {
            return;
        }
        if !logged {
            logged = true;
            let _ = player.push(&pad, 24000);
            crate::applog::log(&format!(
                "tts(vulkan): first audio {:.0}ms",
                t0.elapsed().as_secs_f64() * 1000.0
            ));
        }
        total += c.len();
        let _ = player.push(c, 24000);
    })
    .map_err(|e| anyhow::anyhow!(e))?;
    if total == 0 {
        return Ok(0.0);
    }
    // On barge-in the callback goes silent without draining, so don't wait (the
    // player drops here, stopping the stream and discarding the queue).
    if !playqueue.cancelled(my_epoch) {
        player.wait_until_drained()?;
    }
    Ok(total as f64 / 24000.0)
}
