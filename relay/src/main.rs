#![windows_subsystem = "windows"]

mod applog;
mod config;
mod gui;
mod hangul;
mod i18n;
mod keyhook;
mod modelsrc;
mod overlay;
mod paths;
mod playback;
mod playqueue;
mod queue_overlay;
mod tts_vulkan;

// Single TTS backend: burn/Vulkan (GPU-agnostic). tts_vulkan::spawn_worker holds
// a resident qwen3-tts-burn Engine and takes Sender<String> submissions.

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;

use config::SharedConfig;
use keyhook::HotkeyCombo;
use playqueue::PlayQueue;

fn main() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        applog::log(&format!("PANIC: {info}"));
        prev(info);
    }));

    let mut cfg = config::Config::load();
    playback::set_output_device(&cfg.output_device);

    // Language, in order of authority: what the installer passed on the very
    // first launch, then what the config already holds, then whatever Windows
    // itself is set to. Someone who picked English in the installer should not
    // be met by a Korean window.
    if cfg.lang.is_empty() {
        let from_installer = std::env::args()
            .skip_while(|a| a != "--lang")
            .nth(1)
            .and_then(|v| i18n::Lang::from_code(&v));
        cfg.lang = from_installer
            .unwrap_or_else(i18n::system_default)
            .code()
            .to_string();
    }
    i18n::set(i18n::Lang::from_code(&cfg.lang).unwrap_or(i18n::Lang::Ko));

    cfg.save(); // materialize defaults on first run
    let shared: SharedConfig = config::shared(cfg);

    // Installer post-install step: compile & cache the Vulkan kernels once (~1 min
    // on a fresh machine) so the user's first real launch loads them from disk in
    // seconds instead of recompiling. Headless — no GUI, no keyboard hook.
    if std::env::args().any(|a| a == "--warmup") {
        tts_vulkan::warmup_cache(paths::base_model_dir(), shared);
        return;
    }

    let (hotkey_tx, hotkey_rx) = mpsc::channel::<HotkeyCombo>();

    // Cleared until the TTS engine has loaded, warmed its kernels and prewarmed
    // the voice; the GUI shows a loading screen until it flips true.
    let ready = Arc::new(AtomicBool::new(false));

    // Shared playback state (queue + barge-in epoch + output volume), read by the
    // worker, the stop hotkey, the overlay, and the GUI.
    let playqueue = PlayQueue::new(shared.lock().unwrap().volume);

    // The keyboard hook + overlay live on one thread that pumps its own Win32
    // message queue; the egui GUI owns the main thread.
    let core_cfg = shared.clone();
    let core_ready = ready.clone();
    let core_pq = playqueue.clone();
    std::thread::spawn(move || run_relay_core(core_cfg, hotkey_tx, core_ready, core_pq));

    gui::run(shared, hotkey_rx, ready, playqueue);
}

fn run_relay_core(
    cfg: SharedConfig,
    hotkey_tx: Sender<HotkeyCombo>,
    ready: Arc<AtomicBool>,
    playqueue: Arc<PlayQueue>,
) {
    overlay::create(cfg.clone()).expect("failed to create overlay window");
    queue_overlay::create(cfg.clone()).expect("failed to create queue overlay window");

    // The resident Vulkan engine loads + warms on its own thread; `ready` flips
    // true once synthesis can start, and the GUI shows a loading screen until then.
    let tts_tx = tts_vulkan::spawn_worker(
        paths::base_model_dir(),
        paths::customvoice_model_dir(),
        "auto".to_string(),
        cfg.clone(),
        ready.clone(),
        playqueue.clone(),
    );

    // Self-test hook: VR_SELFTEST=1 sends a test line straight to TTS (bypassing
    // the keyboard hook, which ignores injected keys) to verify the full
    // synth+playback path in the built binary without physical keypresses.
    if std::env::var("VR_SELFTEST").is_ok() {
        let t = tts_tx.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(6));
            let lines = ["워밍업.", "가", "네", "안녕", "안녕하세요, 반갑습니다."];
            for line in lines {
                applog::log(&format!("selftest: send {}", applog::redact(line)));
                let _ = t.send(line.to_string());
                std::thread::sleep(std::time::Duration::from_secs(8));
            }
        });
    }

    let (tx, rx) = mpsc::channel();
    let _guard =
        keyhook::install(tx, cfg, hotkey_tx, playqueue.clone()).expect("failed to install keyboard hook");

    applog::log("core: relay started (overlay+hook+tts ready)");
    std::thread::spawn(move || {
        for text in rx {
            if !text.trim().is_empty() {
                applog::log(&format!("submitted: {}", applog::redact(&text)));
                playqueue.enqueue(text.clone());
                playqueue::refresh_overlay(&playqueue);
                let _ = tts_tx.send(text);
            }
        }
    });

    keyhook::run_message_loop();
}
