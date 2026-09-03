use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

pub const VK_CAPITAL: u32 = 0x14;

pub const VOICE_MINE: &str = "내 목소리";
pub const VOICE_STANDARD: &str = "표준 목소리";
pub const INTONATION_STANDARD: &str = "표준 억양";
pub const INTONATION_PLAIN: &str = "기본 억양";

fn ref_mine() -> String {
    format!(r"{}\_ref_clone.wav", crate::paths::refs_dir())
}
fn ref_standard() -> String {
    format!(r"{}\_ref2_cut.wav", crate::paths::refs_dir())
}

/// A selectable timbre. Two kinds:
/// - clone (`preset` = None): the voice is cloned from `audio_path` on the Base
///   model; the selected intonation applies.
/// - preset (`preset` = Some(speaker)): a CustomVoice preset speaker (e.g.
///   "sohee"); uses the CustomVoice model with its own prosody (intonation
///   selection is ignored), `audio_path` unused.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct VoiceEntry {
    pub name: String,
    #[serde(default)]
    pub audio_path: String,
    #[serde(default)]
    pub preset: Option<String>,
    #[serde(default)]
    pub builtin: bool,
}

impl VoiceEntry {
    pub fn is_preset(&self) -> bool {
        self.preset.is_some()
    }
}

/// A selectable intonation: the ICL prosody to condition on. `audio_path == None`
/// means no ICL (the model's own default prosody). When set, `text` is the
/// reference transcript; if `text` is None a sidecar `.txt` beside the audio is read.
#[derive(Serialize, Deserialize, Clone, PartialEq)]
pub struct IntonationEntry {
    pub name: String,
    #[serde(default)]
    pub audio_path: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub builtin: bool,
}

fn default_voices() -> Vec<VoiceEntry> {
    vec![
        VoiceEntry { name: VOICE_MINE.to_string(), audio_path: ref_mine(), preset: None, builtin: true },
        VoiceEntry { name: VOICE_STANDARD.to_string(), audio_path: ref_standard(), preset: None, builtin: true },
        // No CustomVoice preset here: the Vulkan backend cannot synthesize one,
        // so offering it in the picker only produces silence. Add it back with
        // the CustomVoice checkpoint.
    ]
}

fn default_intonations() -> Vec<IntonationEntry> {
    vec![
        IntonationEntry { name: INTONATION_PLAIN.to_string(), audio_path: None, text: None, builtin: true },
        IntonationEntry {
            name: INTONATION_STANDARD.to_string(),
            audio_path: Some(ref_standard()),
            text: None,
            builtin: true,
        },
    ]
}

#[derive(Serialize, Deserialize, Clone, PartialEq)]
#[serde(default)]
pub struct Config {
    /// Master enable — when false the hotkey does nothing and no capture happens.
    pub enabled: bool,
    /// Hotkey modifiers + main key. Default: Ctrl + CapsLock.
    pub hotkey_ctrl: bool,
    pub hotkey_alt: bool,
    pub hotkey_shift: bool,
    pub hotkey_vk: u32,
    /// Stop/barge-in hotkey: cuts current playback and clears the queue.
    /// vk == 0 means unassigned.
    #[serde(default)]
    pub stop_hotkey_ctrl: bool,
    #[serde(default)]
    pub stop_hotkey_alt: bool,
    #[serde(default)]
    pub stop_hotkey_shift: bool,
    #[serde(default)]
    pub stop_hotkey_vk: u32,
    /// Overlay position: horizontal center as a fraction of screen width (0..1),
    /// and distance of the overlay's bottom edge above the screen bottom (px).
    pub overlay_x_frac: f32,
    pub overlay_bottom_offset: i32,
    /// Overlay size in pixels.
    pub overlay_w: i32,
    pub overlay_h: i32,
    /// Overlay background color (RGB) and opacity (0..255).
    pub overlay_bg: [u8; 3],
    pub overlay_alpha: u8,
    /// Overlay text color (RGB).
    #[serde(default = "default_fg")]
    pub overlay_fg: [u8; 3],
    /// Queue overlay: a separate window (now-playing + pending) with its own
    /// position/size, defaulting to the right side. Shares the input overlay's
    /// colors. Drag it in the GUI's "큐 위치 편집" mode.
    #[serde(default = "default_queue_x_frac")]
    pub queue_x_frac: f32,
    #[serde(default = "default_queue_bottom_offset")]
    pub queue_bottom_offset: i32,
    #[serde(default = "default_queue_w")]
    pub queue_w: i32,
    #[serde(default = "default_queue_h")]
    pub queue_h: i32,
    /// Selected voice + intonation (by name into `voices` / `intonations`).
    pub voice: String,
    pub intonation: String,
    /// Registry of selectable voices (timbres) and intonations (prosody).
    #[serde(default = "default_voices")]
    pub voices: Vec<VoiceEntry>,
    #[serde(default = "default_intonations")]
    pub intonations: Vec<IntonationEntry>,
    /// ICL-reference attention boost (expression); 0 = off.
    #[serde(default = "default_expr_boost")]
    pub expr_boost: f32,
    /// Output gain applied before the virtual cable (1.0 = unchanged).
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// Explicit model directory. When set + valid it takes precedence over the
    /// bundled/downloaded model; empty = auto (bundled → downloaded → HF fetch).
    #[serde(default)]
    pub model_path: String,
    /// Syllables the model cannot pronounce (verified by ear). They are dropped
    /// as they are typed, with a toast, instead of being spoken wrong. Edit this
    /// string to add or remove entries — no rebuild needed.
    #[serde(default = "default_blocked")]
    pub blocked_syllables: String,
    /// Substring of the output device to play into. Empty = pick the first
    /// virtual audio device that is present. Any loopback driver works — the
    /// receiving app just has to listen on its capture side.
    #[serde(default)]
    pub output_device: String,
    /// Interface language: "ko" or "en". Empty means "not chosen yet", in which
    /// case the app follows the Windows UI language on first run - and the
    /// installer writes the language picked there, so a user who selected
    /// English at install does not meet a Korean window.
    #[serde(default)]
    pub lang: String,
}

fn default_fg() -> [u8; 3] {
    [232, 244, 250]
}

/// Ear-verified: the model garbles these (glide syllables — 뿅→"꾱", 뾱→"뻥",
/// 촥→"츅"). Kept as data so the list can grow without a rebuild.
fn default_blocked() -> String {
    "뿅뾱촥뺙꼇".to_string()
}

fn default_queue_x_frac() -> f32 {
    0.88
}
fn default_queue_bottom_offset() -> i32 {
    420
}
fn default_queue_w() -> i32 {
    340
}
fn default_queue_h() -> i32 {
    56
}

fn default_volume() -> f32 {
    1.0
}

fn default_expr_boost() -> f32 {
    0.5
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_device: String::new(),
            enabled: true,
            hotkey_ctrl: true,
            hotkey_alt: false,
            hotkey_shift: false,
            hotkey_vk: VK_CAPITAL,
            stop_hotkey_ctrl: false,
            stop_hotkey_alt: false,
            stop_hotkey_shift: false,
            stop_hotkey_vk: 0,
            overlay_x_frac: 0.5,
            overlay_bottom_offset: 140,
            overlay_w: 700,
            overlay_h: 70,
            overlay_bg: [20, 26, 32],
            overlay_alpha: 235,
            overlay_fg: default_fg(),
            queue_x_frac: default_queue_x_frac(),
            queue_bottom_offset: default_queue_bottom_offset(),
            queue_w: default_queue_w(),
            queue_h: default_queue_h(),
            voice: VOICE_MINE.to_string(),
            intonation: INTONATION_STANDARD.to_string(),
            voices: default_voices(),
            intonations: default_intonations(),
            expr_boost: default_expr_boost(),
            volume: default_volume(),
            model_path: String::new(),
            blocked_syllables: default_blocked(),
            // Empty means "decide on first run": the installer's choice, then
            // the Windows UI language.
            lang: String::new(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let read = |p: &str| -> Option<Config> {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        };
        let mut cfg: Config = read(crate::paths::config_path())
            .or_else(|| {
                // Settings written by an older build, beside the executable.
                let legacy = crate::paths::legacy_config_path();
                let c = read(&legacy);
                if c.is_some() {
                    crate::applog::log(&format!("config: carried over from {legacy}"));
                }
                c
            })
            .unwrap_or_default();
        cfg.merge_builtins();
        cfg.drop_unsupported();
        cfg
    }

    /// Drop voices this backend cannot speak. A CustomVoice preset needs the
    /// CustomVoice checkpoint; kept in the picker it just plays nothing, so an
    /// older config that still lists one loses it here. `find_voice` falls back
    /// to the first entry, so a selection pointing at it still resolves.
    fn drop_unsupported(&mut self) {
        let before = self.voices.len();
        self.voices.retain(|v| !v.is_preset());
        if self.voices.len() != before {
            crate::applog::log("config: dropped CustomVoice preset voices (unsupported backend)");
        }
    }

    /// Append any built-in voices/intonations missing from a loaded config (by
    /// name), so older config files gain newly-added built-ins without losing
    /// the user's custom entries.
    fn merge_builtins(&mut self) {
        for v in default_voices() {
            if !self.voices.iter().any(|e| e.name == v.name) {
                self.voices.push(v);
            }
        }
        for i in default_intonations() {
            if !self.intonations.iter().any(|e| e.name == i.name) {
                self.intonations.push(i);
            }
        }
    }

    /// Write the config, reporting a failure rather than dropping it: a silent
    /// failure looks like the settings saved and then vanished on restart.
    pub fn save(&self) {
        if let Err(e) = self.try_save() {
            crate::applog::log(&format!(
                "config: could not save to {}: {e}",
                crate::paths::config_path()
            ));
            crate::modelsrc::set_status(crate::i18n::config_save_failed(&e));
        }
    }

    fn try_save(&self) -> Result<(), String> {
        let path = std::path::Path::new(crate::paths::config_path());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let s = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, s).map_err(|e| e.to_string())
    }

    pub fn find_voice(&self, name: &str) -> Option<&VoiceEntry> {
        self.voices.iter().find(|v| v.name == name).or_else(|| self.voices.first())
    }

    pub fn find_intonation(&self, name: &str) -> Option<&IntonationEntry> {
        self.intonations.iter().find(|v| v.name == name).or_else(|| self.intonations.first())
    }

    /// Human-readable hotkey string, e.g. "Ctrl + CapsLock".
    pub fn hotkey_label(&self) -> String {
        let mut parts = Vec::new();
        if self.hotkey_ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.hotkey_alt {
            parts.push("Alt".to_string());
        }
        if self.hotkey_shift {
            parts.push("Shift".to_string());
        }
        parts.push(vk_name(self.hotkey_vk));
        parts.join(" + ")
    }

    /// Human-readable stop-hotkey string, or "없음" when unassigned (vk == 0).
    pub fn stop_hotkey_label(&self) -> String {
        if self.stop_hotkey_vk == 0 {
            return "없음".to_string();
        }
        let mut parts = Vec::new();
        if self.stop_hotkey_ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.stop_hotkey_alt {
            parts.push("Alt".to_string());
        }
        if self.stop_hotkey_shift {
            parts.push("Shift".to_string());
        }
        parts.push(vk_name(self.stop_hotkey_vk));
        parts.join(" + ")
    }
}

pub fn vk_name(vk: u32) -> String {
    match vk {
        0x14 => "CapsLock".to_string(),
        0x09 => "Tab".to_string(),
        0x20 => "Space".to_string(),
        0x0D => "Enter".to_string(),
        0xC0 => "`".to_string(),
        0x70..=0x7B => format!("F{}", vk - 0x6F), // F1..F12
        0x30..=0x39 => ((vk as u8) as char).to_string(),
        0x41..=0x5A => ((vk as u8) as char).to_string(),
        other => format!("0x{other:02X}"),
    }
}

pub type SharedConfig = Arc<Mutex<Config>>;

pub fn shared(config: Config) -> SharedConfig {
    Arc::new(Mutex::new(config))
}
