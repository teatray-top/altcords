//! Base-model directory resolution + first-run HuggingFace download. The burn
//! engine loads from a directory, so downloaded files are assembled into the
//! same layout as the bundled/dev model dir; then loading is backend-agnostic.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const BASE_REPO: &str = "Qwen/Qwen3-TTS-12Hz-1.7B-Base";
const TOKENIZER_REPO: &str = "Qwen/Qwen3-TTS-Tokenizer-12Hz";
const BASE_FILES: &[&str] = &[
    "config.json",
    "generation_config.json",
    "merges.txt",
    "model.safetensors",
    "preprocessor_config.json",
    "tokenizer_config.json",
    "vocab.json",
];
const ST_FILES: &[&str] = &["config.json", "configuration.json", "model.safetensors", "preprocessor_config.json"];
/// Required files (relative to the dir) with a minimum size, so a partial/empty
/// folder is rejected up front with a clear message instead of failing deep in load.
const REQUIRED: &[(&str, u64)] = &[
    ("config.json", 0),
    ("vocab.json", 0),
    ("merges.txt", 0),
    ("model.safetensors", 500_000_000),
    ("speech_tokenizer/model.safetensors", 100_000_000),
];

static STATUS: Mutex<String> = Mutex::new(String::new());
static LOAD_FAILED: AtomicBool = AtomicBool::new(false);

/// Human-readable load/download status for the GUI loading screen.
pub fn status() -> String {
    STATUS.lock().map(|s| s.clone()).unwrap_or_default()
}
pub fn load_failed() -> bool {
    LOAD_FAILED.load(Ordering::Relaxed)
}

pub fn set_load_failed(v: bool) {
    LOAD_FAILED.store(v, Ordering::Relaxed);
}

pub fn set_status(s: String) {
    if let Ok(mut g) = STATUS.lock() {
        *g = s;
    }
}

/// Check a folder holds a usable model: every required file present and the big
/// weights a sane size. Returns a specific "what's missing" message on failure.
pub fn validate_model_dir(dir: &str) -> Result<(), String> {
    let d = Path::new(dir);
    if !d.is_dir() {
        return Err(format!("폴더가 없습니다: {dir}"));
    }
    for (rel, min) in REQUIRED {
        match std::fs::metadata(d.join(rel)) {
            Ok(m) if m.len() >= *min => {}
            Ok(m) => return Err(format!("{rel}: 파일이 너무 작습니다 ({}MB)", m.len() / 1_000_000)),
            Err(_) => return Err(format!("{rel}: 파일이 없습니다")),
        }
    }
    Ok(())
}

/// Resolve the base model directory. Precedence: explicit config path (fail loudly
/// if wrong) → bundled/dev dir → previously-downloaded user-data dir → HF download
/// into the user-data dir. `explicit` is `config.model_path` (may be empty).
pub fn resolve_base_dir(explicit: &str) -> Result<String, String> {
    if !explicit.trim().is_empty() {
        validate_model_dir(explicit)?;
        return Ok(explicit.to_string());
    }
    let bundled = crate::paths::base_model_dir();
    if validate_model_dir(&bundled).is_ok() {
        return Ok(bundled);
    }
    let data = crate::paths::data_model_dir();
    if validate_model_dir(&data).is_ok() {
        return Ok(data);
    }
    set_status(crate::i18n::downloading_model());
    crate::applog::log("modelsrc: no local model — downloading from HuggingFace");
    download_base(&data)?;
    validate_model_dir(&data)?;
    set_status(String::new());
    Ok(data)
}

/// Download the 1.7B base model + 12Hz speech tokenizer from HF and assemble them
/// into `dest` with the layout the engine expects (tokenizer files under
/// `dest/speech_tokenizer/`).
fn download_base(dest: &str) -> Result<(), String> {
    use hf_hub::api::sync::ApiBuilder;
    let api = ApiBuilder::new().with_progress(false).build().map_err(|e| format!("HF API: {e}"))?;
    let dest = Path::new(dest);
    let st = dest.join("speech_tokenizer");
    std::fs::create_dir_all(&st).map_err(|e| format!("폴더 생성 실패: {e}"))?;

    let base = api.model(BASE_REPO.to_string());
    for f in BASE_FILES {
        set_status(crate::i18n::downloading_file(&f));
        let cached = base.get(f).map_err(|e| format!("{BASE_REPO}/{f}: {e}"))?;
        std::fs::copy(&cached, dest.join(f)).map_err(|e| format!("{f} 복사 실패: {e}"))?;
    }
    let tok = api.model(TOKENIZER_REPO.to_string());
    for f in ST_FILES {
        set_status(crate::i18n::downloading_file(&format!("speech_tokenizer/{f}")));
        let cached = tok.get(f).map_err(|e| format!("{TOKENIZER_REPO}/{f}: {e}"))?;
        std::fs::copy(&cached, st.join(f)).map_err(|e| format!("speech_tokenizer/{f} 복사 실패: {e}"))?;
    }
    Ok(())
}
