use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Environment override for the whole layout: assets are read from here and
/// state is written here. Everything else is derived from the executable.
const ROOT_ENV: &str = "ALTCORDS_ROOT";

fn env_root() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        std::env::var_os(ROOT_ENV)
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
    })
    .as_deref()
}

fn exe_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."))
    })
}

/// Where assets are read from: `ALTCORDS_ROOT`, else the executable's directory
/// (the distributable layout, where `models\` and `refs\` sit beside the exe).
fn root() -> &'static Path {
    env_root().unwrap_or_else(|| exe_dir())
}

/// Where state is written: `ALTCORDS_ROOT` if set, else `%LOCALAPPDATA%\AltCords`,
/// else the executable's directory. The exe may live somewhere read-only such as
/// Program Files, so state does not default to sitting beside it.
fn state_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(r) = env_root() {
            return r.to_path_buf();
        }
        let dir = match std::env::var_os("LOCALAPPDATA") {
            Some(v) if !v.is_empty() => PathBuf::from(v).join("AltCords"),
            _ => return exe_dir().to_path_buf(),
        };
        if std::fs::create_dir_all(&dir).is_err() {
            return exe_dir().to_path_buf();
        }
        dir
    })
}

fn s(p: PathBuf) -> String {
    p.to_string_lossy().into_owned()
}

pub fn base_model_dir() -> String {
    s(root().join("models").join("base"))
}

/// User-writable location the base model is downloaded to when it isn't bundled
/// beside the exe (which may be read-only).
pub fn data_model_dir() -> String {
    s(state_dir().join("models").join("base"))
}

pub fn customvoice_model_dir() -> String {
    s(root().join("models").join("customvoice"))
}

pub fn refs_dir() -> String {
    s(root().join("refs"))
}

pub fn config_path() -> &'static str {
    static P: OnceLock<String> = OnceLock::new();
    P.get_or_init(|| s(state_dir().join("config.json")))
}

/// Where the config used to live, beside the executable. Read once at startup
/// if the current location has none, so an existing install keeps its settings.
pub fn legacy_config_path() -> String {
    s(exe_dir().join("config.json"))
}

pub fn log_path() -> &'static str {
    static P: OnceLock<String> = OnceLock::new();
    P.get_or_init(|| s(state_dir().join("relay_log.txt")))
}
