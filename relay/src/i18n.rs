//! UI strings in Korean and English.
//!
//! The app was written in Korean throughout, which is right for its author but
//! makes it unusable for anyone else. Strings live here as a flat lookup rather
//! than a `.po`-style crate: there are under a hundred of them, they are all
//! short, and a `match` keeps every translation visible next to its original.
//!
//! Not included, deliberately: voice and intonation names stored in
//! `config.json`, the blocked-syllable defaults, and the pronunciation test
//! syllables. Those are user data or Korean-specific content, not interface.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Ko,
    En,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Ko => "ko",
            Lang::En => "en",
        }
    }

    pub fn from_code(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ko" | "korean" | "ko-kr" => Some(Lang::Ko),
            "en" | "english" | "en-us" => Some(Lang::En),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Lang::Ko => "한국어",
            Lang::En => "English",
        }
    }
}

// The GUI reads this on every frame from several places, so it is global rather
// than threaded through every widget call.
static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set(lang: Lang) {
    CURRENT.store(if lang == Lang::En { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn current() -> Lang {
    if CURRENT.load(Ordering::Relaxed) == 1 {
        Lang::En
    } else {
        Lang::Ko
    }
}

/// The language Windows is set to, used when nothing has been chosen yet.
#[cfg(windows)]
pub fn system_default() -> Lang {
    // 0x0412 is ko-KR; the primary language id is the low 10 bits.
    let id = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
    if (id & 0x3ff) == 0x12 {
        Lang::Ko
    } else {
        Lang::En
    }
}

#[cfg(not(windows))]
pub fn system_default() -> Lang {
    Lang::En
}

/// Look up `key` in the active language. Keys are the English text in
/// lower-snake form, so a missing arm is obvious at the call site.
pub fn t(key: &str) -> &'static str {
    let ko = current() == Lang::Ko;
    match key {
        // tray + window
        "open_settings" => if ko { "설정 열기" } else { "Open settings" },
        "quit" => if ko { "종료" } else { "Quit" },

        // loading
        "engine_loading" => if ko { "엔진 로딩 중" } else { "Loading engine" },
        "first_run_slow" => if ko { "첫 실행은 몇 분 걸립니다" } else { "The first run takes a few minutes" },
        "choose_model_folder" => if ko { "모델 폴더 선택" } else { "Choose model folder" },

        // main toggles
        "enabled" => if ko { "사용" } else { "Enabled" },
        "input" => if ko { "입력" } else { "Input" },
        "stop" => if ko { "정지" } else { "Stop" },
        "press_a_key" => if ko { "키 입력…" } else { "Press a key…" },
        "press_key_esc_cancel" => if ko { "키를 누르세요 (Esc 취소)" } else { "Press a key (Esc to cancel)" },
        "clear_stop_hotkey" => if ko { "정지 단축키 해제" } else { "Clear stop hotkey" },
        "none" => if ko { "없음" } else { "None" },
        "auto" => if ko { "자동" } else { "Auto" },

        // voices / intonation
        "voice" => if ko { "목소리" } else { "Voice" },
        "intonation" => if ko { "억양" } else { "Intonation" },
        "add_voice" => if ko { "목소리 추가" } else { "Add voice" },
        "add_intonation" => if ko { "억양 추가" } else { "Add intonation" },
        "delete" => if ko { "삭제" } else { "Delete" },
        "name" => if ko { "이름" } else { "Name" },
        "add" => if ko { "추가" } else { "Add" },
        "cancel" => if ko { "취소" } else { "Cancel" },
        "ok" => if ko { "확인" } else { "OK" },
        "choose_audio" => if ko { "오디오 선택" } else { "Choose audio" },
        "choose_folder" => if ko { "폴더 선택" } else { "Choose folder" },
        "fill_every_field" => if ko { "모든 항목을 채우세요" } else { "Fill in every field" },
        "uses_audio_timbre" => if ko { "오디오의 음색을 사용합니다" } else { "Takes its timbre from the audio" },
        "line_matching_audio" => if ko { "대사 (오디오와 동일하게)" } else { "Transcript (must match the audio)" },

        // model
        "model" => if ko { "모델" } else { "Model" },
        "model_auto_restart" => if ko { "모델 자동 선택 (재시작 후 적용)" } else { "Model chosen automatically (applies after restart)" },
        "model_set_restart" => if ko { "모델 폴더 설정 (재시작 후 적용)" } else { "Model folder set (applies after restart)" },

        // overlay
        "overlay" => if ko { "오버레이" } else { "Overlay" },
        "composer" => if ko { "입력창" } else { "Composer" },
        "queue" => if ko { "대기열" } else { "Queue" },
        "adjust_position" => if ko { "위치 조정" } else { "Adjust position" },
        "width" => if ko { "너비" } else { "Width" },
        "height" => if ko { "높이" } else { "Height" },
        "background" => if ko { "배경색" } else { "Background" },
        "text_color" => if ko { "글자색" } else { "Text colour" },
        "opacity" => if ko { "투명도" } else { "Opacity" },
        "input_preview" => if ko { "입력 미리보기" } else { "Input preview" },

        // audio
        "volume" => if ko { "볼륨" } else { "Volume" },
        "expressiveness" => if ko { "표현력" } else { "Expressiveness" },

        // blocked syllables
        "blocked_syllables" => if ko { "제외 글자" } else { "Blocked characters" },
        "blocked_hint" => if ko { "발음이 안 되는 글자를 붙여 쓰세요" } else { "Type characters the model cannot pronounce, without spaces" },
        "blocked_auto_removed" => if ko { "입력 시 자동으로 빠집니다" } else { "They are dropped as you type" },

        // misc actions
        "restore_defaults" => if ko { "기본값 복원" } else { "Restore defaults" },
        "defaults" => if ko { "기본값" } else { "Defaults" },
        "undo" => if ko { "실행취소" } else { "Undo" },
        "undone" => if ko { "되돌림" } else { "Undone" },
        "clear" => if ko { "해제" } else { "Clear" },
        "language" => if ko { "언어" } else { "Language" },

        // queue overlay
        "playing" => if ko { "▶  재생 중\n·  대기 중" } else { "▶  playing\n·  queued" },

        _ => "",
    }
}

/// Formatted strings, kept separate so the argument order is explicit in both
/// languages rather than implied by a positional placeholder.
pub fn hotkey_input(label: &str) -> String {
    match current() {
        Lang::Ko => format!("입력 단축키: {label}"),
        Lang::En => format!("Input hotkey: {label}"),
    }
}

pub fn hotkey_stop(label: &str) -> String {
    match current() {
        Lang::Ko => format!("정지 단축키: {label}"),
        Lang::En => format!("Stop hotkey: {label}"),
    }
}

pub fn model_folder_error(e: &str) -> String {
    match current() {
        Lang::Ko => format!("모델 폴더 오류: {e}"),
        Lang::En => format!("Model folder error: {e}"),
    }
}

pub fn voice_added(name: &str) -> String {
    match current() {
        Lang::Ko => format!("목소리 추가: {name}"),
        Lang::En => format!("Voice added: {name}"),
    }
}

pub fn voice_deleted(name: &str) -> String {
    match current() {
        Lang::Ko => format!("목소리 삭제: {name}"),
        Lang::En => format!("Voice deleted: {name}"),
    }
}

pub fn intonation_added(name: &str) -> String {
    match current() {
        Lang::Ko => format!("억양 추가: {name}"),
        Lang::En => format!("Intonation added: {name}"),
    }
}

pub fn intonation_deleted(name: &str) -> String {
    match current() {
        Lang::Ko => format!("억양 삭제: {name}"),
        Lang::En => format!("Intonation deleted: {name}"),
    }
}

/// Shown when the composer drops a character the model cannot say.
pub fn syllable_blocked(c: char) -> String {
    match current() {
        Lang::Ko => format!("'{c}' 발음 불가 — 제외됨"),
        Lang::En => format!("'{c}' cannot be pronounced — dropped"),
    }
}

/// Windows file-dialog filter: NUL-separated pairs, terminated by a double NUL.
pub fn audio_filter() -> &'static str {
    match current() {
        Lang::Ko => "오디오 파일\0*.wav;*.mp3;*.flac;*.m4a;*.ogg\0모든 파일\0*.*\0\0",
        Lang::En => "Audio files\0*.wav;*.mp3;*.flac;*.m4a;*.ogg\0All files\0*.*\0\0",
    }
}

pub fn reference_audio_title() -> &'static str {
    match current() {
        Lang::Ko => "참조 오디오 선택\0",
        Lang::En => "Choose reference audio\0",
    }
}

pub fn downloading_model() -> String {
    match current() {
        Lang::Ko => "모델 다운로드 중… (약 4.5GB, 처음 한 번만)".to_string(),
        Lang::En => "Downloading the model… (about 4.5 GB, once)".to_string(),
    }
}

pub fn downloading_file(f: &str) -> String {
    match current() {
        Lang::Ko => format!("다운로드: {f}"),
        Lang::En => format!("Downloading: {f}"),
    }
}

pub fn model_load_failed(e: &str) -> String {
    match current() {
        Lang::Ko => format!("모델을 불러올 수 없습니다: {e}"),
        Lang::En => format!("Could not load the model: {e}"),
    }
}

pub fn config_save_failed(e: &str) -> String {
    match current() {
        Lang::Ko => format!("설정을 저장하지 못했습니다: {e}"),
        Lang::En => format!("Could not save settings: {e}"),
    }
}
