use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::Duration;

use eframe::egui;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::config::{Config, IntonationEntry, SharedConfig, VoiceEntry};
use crate::keyhook::{self, HotkeyCombo};
use crate::playqueue::PlayQueue;

const ICON_RGBA: &[u8] = include_bytes!("../assets/tray.rgba");
const ICON_SIZE: u32 = 32;

pub fn run(
    config: SharedConfig,
    hotkey_rx: Receiver<HotkeyCombo>,
    ready: Arc<AtomicBool>,
    playqueue: Arc<PlayQueue>,
) {
    qwen3_tts_burn::talker::set_attn_boost(config.lock().unwrap().expr_boost);
    let icon = egui::IconData {
        rgba: ICON_RGBA.to_vec(),
        width: ICON_SIZE,
        height: ICON_SIZE,
    };
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([460.0, 620.0])
            .with_min_inner_size([440.0, 520.0])
            .with_title("AltCords")
            .with_icon(icon),
        ..Default::default()
    };
    let mut app = App {
        config,
        hotkey_rx,
        undo: Vec::new(),
        snap_on_press: None,
        capturing_hotkey: false,
        editing_overlay: false,
        editing_queue: false,
        tray: None,
        tray_thread: false,
        gui_hwnd: None,
        status: String::new(),
        add_form: None,
        file_rx: None,
        folder_rx: None,
        ready,
        playqueue,
        capturing_stop: false,
    };
    let _ = eframe::run_native(
        "AltCords",
        native_options,
        Box::new(move |cc| {
            install_korean_font(&cc.egui_ctx);
            app.gui_hwnd = hwnd_of(cc);
            Ok(Box::new(app))
        }),
    );
}

fn hwnd_of(cc: &eframe::CreationContext<'_>) -> Option<isize> {
    match cc.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

/// egui's bundled fonts have no CJK glyphs, so Korean renders as tofu boxes.
/// Load Malgun Gothic (ships with Windows) as a fallback for both families.
fn install_korean_font(ctx: &egui::Context) {
    let path = r"C:\Windows\Fonts\malgun.ttf";
    let Ok(data) = std::fs::read(path) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("malgun".to_owned(), egui::FontData::from_owned(data));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("malgun".to_owned());
    }
    ctx.set_fonts(fonts);
}

#[derive(Clone, Copy)]
enum AddKind {
    Voice,
    Intonation,
}

struct AddForm {
    kind: AddKind,
    name: String,
    path: String,
    text: String,
}

struct App {
    config: SharedConfig,
    hotkey_rx: Receiver<HotkeyCombo>,
    undo: Vec<Config>,
    snap_on_press: Option<Config>,
    capturing_hotkey: bool,
    editing_overlay: bool,
    editing_queue: bool,
    tray: Option<tray_icon::TrayIcon>,
    tray_thread: bool,
    gui_hwnd: Option<isize>,
    status: String,
    add_form: Option<AddForm>,
    file_rx: Option<Receiver<Option<String>>>,
    folder_rx: Option<Receiver<Option<String>>>,
    ready: Arc<AtomicBool>,
    playqueue: Arc<PlayQueue>,
    capturing_stop: bool,
}

impl App {
    fn snapshot(&mut self) {
        let c = self.config.lock().unwrap().clone();
        self.undo.push(c);
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
    }

    /// Push the whole config to the live overlay (colors/alpha/size/position).
    fn apply_to_overlay(&self, c: &Config) {
        crate::overlay::set_appearance(c.overlay_bg, c.overlay_alpha);
        crate::overlay::set_text_color(c.overlay_fg);
        crate::overlay::set_size(c.overlay_w, c.overlay_h);
        crate::overlay::set_position(c.overlay_x_frac, c.overlay_bottom_offset);
    }

    fn apply_hotkey(&mut self, c: bool, a: bool, s: bool, vk: u32) {
        self.snapshot();
        let mut cfg = self.config.lock().unwrap();
        cfg.hotkey_ctrl = c;
        cfg.hotkey_alt = a;
        cfg.hotkey_shift = s;
        cfg.hotkey_vk = vk;
        cfg.save();
        self.status = crate::i18n::hotkey_input(&cfg.hotkey_label());
    }

    fn apply_stop_hotkey(&mut self, c: bool, a: bool, s: bool, vk: u32) {
        self.snapshot();
        let mut cfg = self.config.lock().unwrap();
        cfg.stop_hotkey_ctrl = c;
        cfg.stop_hotkey_alt = a;
        cfg.stop_hotkey_shift = s;
        cfg.stop_hotkey_vk = vk;
        cfg.save();
        self.status = crate::i18n::hotkey_stop(&cfg.stop_hotkey_label());
    }

    /// The tray icon + its background event handler. Events are drained on a
    /// dedicated thread (not the egui update loop, which stops ticking while the
    /// window is hidden in the tray — that's why a tray "Exit" click was ignored).
    fn build_tray(&mut self) {
        use tray_icon::menu::{Menu, MenuItem};
        let menu = Menu::new();
        let show = MenuItem::new(crate::i18n::t("open_settings"), true, None);
        let quit = MenuItem::new(crate::i18n::t("quit"), true, None);
        let _ = menu.append(&show);
        let _ = menu.append(&quit);
        let show_id = show.id().clone();
        let quit_id = quit.id().clone();

        if let Ok(icon) = tray_icon::Icon::from_rgba(ICON_RGBA.to_vec(), ICON_SIZE, ICON_SIZE) {
            if let Ok(tray) = tray_icon::TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("AltCords")
                .with_icon(icon)
                .build()
            {
                self.tray = Some(tray);
                spawn_tray_thread(self.gui_hwnd, show_id, quit_id);
                self.tray_thread = true;
            }
        }
    }

    fn launch_folder_picker(&mut self) {
        if self.folder_rx.is_some() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(pick_folder());
        });
        self.folder_rx = Some(rx);
    }

    /// A picked model folder lands here (works during the loading screen too, so a
    /// failed download can be recovered by pointing at a local model).
    fn poll_folder_picker(&mut self) {
        let done = self.folder_rx.as_ref().and_then(|rx| rx.try_recv().ok());
        let Some(res) = done else { return };
        self.folder_rx = None;
        if let Some(dir) = res {
            match crate::modelsrc::validate_model_dir(&dir) {
                Ok(()) => {
                    let mut cfg = self.config.lock().unwrap();
                    cfg.model_path = dir;
                    cfg.save();
                    self.status = crate::i18n::t("model_set_restart").to_string();
                }
                Err(e) => self.status = crate::i18n::model_folder_error(&e.to_string()),
            }
        }
    }

    fn poll_hotkey(&mut self) {
        while let Ok((c, a, s, vk)) = self.hotkey_rx.try_recv() {
            // Ignore anything arriving after the GUI already captured the combo.
            let stop = self.capturing_stop;
            if !self.capturing_hotkey && !stop {
                continue;
            }
            self.capturing_hotkey = false;
            self.capturing_stop = false;
            if vk != 0 {
                if stop {
                    self.apply_stop_hotkey(c, a, s, vk);
                } else {
                    self.apply_hotkey(c, a, s, vk);
                }
            } else {
                self.status = crate::i18n::t("cancel").to_string();
            }
        }
    }

    /// Capture the hotkey from the GUI's own focused key events. This is the
    /// reliable path while the settings window has focus; the low-level hook is
    /// kept as a fallback for keys egui can't represent (e.g. CapsLock) or when
    /// focus is elsewhere.
    fn capture_hotkey_from_egui(&mut self, ctx: &egui::Context) {
        if !self.capturing_hotkey && !self.capturing_stop {
            return;
        }
        let result = ctx.input(|i| {
            for ev in &i.events {
                if let egui::Event::Key { key, pressed: true, modifiers, .. } = ev {
                    if *key == egui::Key::Escape {
                        return Some(None);
                    }
                    if let Some(vk) = egui_key_to_vk(*key) {
                        return Some(Some((modifiers.ctrl, modifiers.alt, modifiers.shift, vk)));
                    }
                }
            }
            None
        });
        match result {
            Some(Some((c, a, s, vk))) => {
                let stop = self.capturing_stop;
                self.capturing_hotkey = false;
                self.capturing_stop = false;
                keyhook::cancel_hotkey_capture();
                if stop {
                    self.apply_stop_hotkey(c, a, s, vk);
                } else {
                    self.apply_hotkey(c, a, s, vk);
                }
            }
            Some(None) => {
                self.capturing_hotkey = false;
                self.capturing_stop = false;
                keyhook::cancel_hotkey_capture();
                self.status = crate::i18n::t("cancel").to_string();
            }
            None => {}
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.tray_thread {
            self.build_tray();
        }
        self.poll_folder_picker();

        // Loading gate: until the TTS engine has loaded + warmed its GPU kernels,
        // show a spinner instead of the settings. On a fresh machine the first
        // launch compiles the kernels (minutes); later launches load from cache.
        if !self.ready.load(Ordering::Relaxed) {
            // A close during loading still minimises to the tray, not quit.
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                hide_window(self.gui_hwnd);
            }
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(150.0);
                    ui.heading("AltCords");
                    ui.add_space(24.0);
                    ui.add(egui::Spinner::new().size(44.0));
                    ui.add_space(24.0);
                    ui.label(egui::RichText::new(crate::i18n::t("engine_loading")).size(16.0));
                    let st = crate::modelsrc::status();
                    if !st.is_empty() {
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new(&st).size(14.0));
                    }
                    if crate::modelsrc::load_failed() {
                        ui.add_space(12.0);
                        if ui.button(crate::i18n::t("choose_model_folder")).clicked() {
                            self.launch_folder_picker();
                        }
                        if !self.status.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(&self.status).size(13.0));
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(crate::i18n::t("first_run_slow")).weak());
                });
            });
            ctx.request_repaint_after(Duration::from_millis(150));
            return;
        }

        self.capture_hotkey_from_egui(ctx);
        self.poll_hotkey();

        // X on the window hides it to the tray instead of quitting. Hiding via
        // Win32 (not ViewportCommand::Visible) keeps eframe's own state coherent
        // and lets the tray thread bring it back with ShowWindow.
        if ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            hide_window(self.gui_hwnd);
        }

        // A picked file path from the (threaded) native dialog lands here.
        if let Some(rx) = &self.file_rx {
            if let Ok(res) = rx.try_recv() {
                if let (Some(path), Some(form)) = (res, self.add_form.as_mut()) {
                    form.path = path;
                }
                self.file_rx = None;
            }
        }


        // Undo granularity: one entry per pointer gesture. Capture the config as
        // it stood when the press began; on release, if it changed during the
        // gesture, that pre-state becomes an undo step.
        if ctx.input(|i| i.pointer.any_pressed()) {
            self.snap_on_press = Some(self.config.lock().unwrap().clone());
        }

        let mut cfg = self.config.lock().unwrap().clone();
        let mut changed = false;
        let mut do_undo = false;
        let mut do_default = false;
        let mut open_add_voice = false;
        let mut open_add_intonation = false;
        let mut delete_voice = false;
        let mut delete_intonation = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(4.0);
            ui.heading("AltCords");
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("language"));
                for lang in [crate::i18n::Lang::Ko, crate::i18n::Lang::En] {
                    let active = crate::i18n::current() == lang;
                    if ui.selectable_label(active, lang.label()).clicked() && !active {
                        crate::i18n::set(lang);
                        cfg.lang = lang.code().to_string();
                        changed = true;
                    }
                }
            });
            if ui.checkbox(&mut cfg.enabled, crate::i18n::t("enabled")).changed() {
                changed = true;
            }

            ui.separator();
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("input"));
                let label = if self.capturing_hotkey {
                    crate::i18n::t("press_a_key").to_string()
                } else {
                    cfg.hotkey_label()
                };
                let btn = egui::Button::new(label).min_size(egui::vec2(190.0, 0.0));
                if ui.add(btn).clicked() && !self.capturing_hotkey {
                    self.capturing_hotkey = true;
                    self.capturing_stop = false;
                    self.status = crate::i18n::t("press_key_esc_cancel").to_string();
                    keyhook::request_hotkey_capture();
                }
            });

            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("stop"));
                let label = if self.capturing_stop {
                    crate::i18n::t("press_a_key").to_string()
                } else {
                    cfg.stop_hotkey_label()
                };
                let btn = egui::Button::new(label).min_size(egui::vec2(190.0, 0.0));
                if ui.add(btn).clicked() && !self.capturing_stop {
                    self.capturing_stop = true;
                    self.capturing_hotkey = false;
                    self.status = crate::i18n::t("press_key_esc_cancel").to_string();
                    keyhook::request_hotkey_capture();
                }
                if cfg.stop_hotkey_vk != 0 && ui.button(crate::i18n::t("clear")).clicked() {
                    cfg.stop_hotkey_ctrl = false;
                    cfg.stop_hotkey_alt = false;
                    cfg.stop_hotkey_shift = false;
                    cfg.stop_hotkey_vk = 0;
                    changed = true;
                    self.status = crate::i18n::t("clear_stop_hotkey").to_string();
                }
            });

            ui.separator();
            ui.label(egui::RichText::new(crate::i18n::t("overlay")).strong());
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::i18n::t("adjust_position")).weak());
                if ui.checkbox(&mut self.editing_overlay, crate::i18n::t("composer")).changed() {
                    crate::overlay::set_edit_mode(self.editing_overlay);
                }
                if ui.checkbox(&mut self.editing_queue, crate::i18n::t("queue")).changed() {
                    crate::queue_overlay::set_edit_mode(self.editing_queue);
                }
            });

            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("background"));
                if ui.color_edit_button_srgb(&mut cfg.overlay_bg).changed() {
                    changed = true;
                    crate::overlay::set_appearance(cfg.overlay_bg, cfg.overlay_alpha);
                }
                ui.add_space(12.0);
                ui.label(crate::i18n::t("text_color"));
                if ui.color_edit_button_srgb(&mut cfg.overlay_fg).changed() {
                    changed = true;
                    crate::overlay::set_text_color(cfg.overlay_fg);
                }
            });
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("opacity"));
                let mut a = cfg.overlay_alpha as f32;
                if ui.add(egui::Slider::new(&mut a, 40.0..=255.0)).changed() {
                    cfg.overlay_alpha = a as u8;
                    changed = true;
                    crate::overlay::set_appearance(cfg.overlay_bg, cfg.overlay_alpha);
                }
            });
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("width"));
                let mut w = cfg.overlay_w as f32;
                if ui.add(egui::Slider::new(&mut w, 200.0..=1600.0)).changed() {
                    cfg.overlay_w = w as i32;
                    changed = true;
                    crate::overlay::set_size(cfg.overlay_w, cfg.overlay_h);
                }
            });
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("height"));
                let mut h = cfg.overlay_h as f32;
                if ui.add(egui::Slider::new(&mut h, 40.0..=300.0)).changed() {
                    cfg.overlay_h = h as i32;
                    changed = true;
                    crate::overlay::set_size(cfg.overlay_w, cfg.overlay_h);
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("expressiveness"));
                if ui
                    .add(egui::Slider::new(&mut cfg.expr_boost, 0.0..=1.0).fixed_decimals(2))
                    .changed()
                {
                    changed = true;
                    qwen3_tts_burn::talker::set_attn_boost(cfg.expr_boost);
                }
            });
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("volume"));
                if ui
                    .add(egui::Slider::new(&mut cfg.volume, 0.0..=4.0).fixed_decimals(2))
                    .changed()
                {
                    changed = true;
                    self.playqueue.set_volume(cfg.volume);
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("blocked_syllables"));
                if ui
                    .add(
                        egui::TextEdit::singleline(&mut cfg.blocked_syllables)
                            .desired_width(f32::INFINITY)
                            .hint_text(crate::i18n::t("blocked_hint")),
                    )
                    .changed()
                {
                    changed = true;
                }
            });
            ui.label(
                egui::RichText::new(crate::i18n::t("blocked_auto_removed"))
                    .weak()
                    .small(),
            );

            ui.separator();
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("model"));
                if ui.button(crate::i18n::t("choose_folder")).clicked() {
                    self.launch_folder_picker();
                }
                if !cfg.model_path.is_empty() && ui.button(crate::i18n::t("auto")).clicked() {
                    cfg.model_path.clear();
                    changed = true;
                    self.status = crate::i18n::t("model_auto_restart").to_string();
                }
                let label = if cfg.model_path.is_empty() {
                    crate::i18n::t("auto").to_string()
                } else {
                    cfg.model_path.clone()
                };
                ui.label(egui::RichText::new(label).weak().small());
            });

            ui.separator();
            let voice_items: Vec<(String, bool)> =
                cfg.voices.iter().map(|v| (v.name.clone(), v.builtin)).collect();
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("voice"));
                egui::ComboBox::from_id_salt("voice")
                    .selected_text(cfg.voice.clone())
                    .show_ui(ui, |ui| {
                        for (name, _) in &voice_items {
                            if ui.selectable_value(&mut cfg.voice, name.clone(), name).clicked() {
                                changed = true;
                            }
                        }
                    });
                if ui.button(crate::i18n::t("add")).clicked() {
                    open_add_voice = true;
                }
                let deletable = voice_items
                    .iter()
                    .find(|(n, _)| *n == cfg.voice)
                    .map(|(_, b)| !*b)
                    .unwrap_or(false);
                if ui.add_enabled(deletable, egui::Button::new(crate::i18n::t("delete"))).clicked() {
                    delete_voice = true;
                }
            });

            let into_items: Vec<(String, bool)> =
                cfg.intonations.iter().map(|v| (v.name.clone(), v.builtin)).collect();
            ui.horizontal(|ui| {
                ui.label(crate::i18n::t("intonation"));
                egui::ComboBox::from_id_salt("intonation")
                    .selected_text(cfg.intonation.clone())
                    .show_ui(ui, |ui| {
                        for (name, _) in &into_items {
                            if ui
                                .selectable_value(&mut cfg.intonation, name.clone(), name)
                                .clicked()
                            {
                                changed = true;
                            }
                        }
                    });
                if ui.button(crate::i18n::t("add")).clicked() {
                    open_add_intonation = true;
                }
                let deletable = into_items
                    .iter()
                    .find(|(n, _)| *n == cfg.intonation)
                    .map(|(_, b)| !*b)
                    .unwrap_or(false);
                if ui.add_enabled(deletable, egui::Button::new(crate::i18n::t("delete"))).clicked() {
                    delete_intonation = true;
                }
            });

            ui.separator();
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.undo.is_empty(), egui::Button::new(crate::i18n::t("undo"))).clicked() {
                    do_undo = true;
                }
                if ui.button(crate::i18n::t("defaults")).clicked() {
                    do_default = true;
                }
            });

            ui.add_space(6.0);
            if !self.status.is_empty() {
                ui.label(egui::RichText::new(&self.status).weak());
            }
        });

        // ── Add voice / intonation dialog ─────────────────────────────────────
        let mut do_add = false;
        let mut cancel_add = false;
        let mut pick_file = false;
        if let Some(form) = &mut self.add_form {
            let title = match form.kind {
                AddKind::Voice => crate::i18n::t("add_voice"),
                AddKind::Intonation => crate::i18n::t("add_intonation"),
            };
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(crate::i18n::t("name"));
                        ui.text_edit_singleline(&mut form.name);
                    });
                    ui.horizontal(|ui| {
                        if ui.button(crate::i18n::t("choose_audio")).clicked() {
                            pick_file = true;
                        }
                        let shown = if form.path.is_empty() { crate::i18n::t("none") } else { form.path.as_str() };
                        ui.label(egui::RichText::new(shown).weak());
                    });
                    if matches!(form.kind, AddKind::Intonation) {
                        ui.label(crate::i18n::t("line_matching_audio"));
                        ui.add(
                            egui::TextEdit::multiline(&mut form.text)
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                    } else {
                        ui.label(egui::RichText::new(crate::i18n::t("uses_audio_timbre")).weak());
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button(crate::i18n::t("ok")).clicked() {
                            do_add = true;
                        }
                        if ui.button(crate::i18n::t("cancel")).clicked() {
                            cancel_add = true;
                        }
                    });
                });
        }

        if open_add_voice {
            self.add_form = Some(AddForm {
                kind: AddKind::Voice,
                name: String::new(),
                path: String::new(),
                text: String::new(),
            });
        }
        if open_add_intonation {
            self.add_form = Some(AddForm {
                kind: AddKind::Intonation,
                name: String::new(),
                path: String::new(),
                text: String::new(),
            });
        }
        if pick_file && self.file_rx.is_none() {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(pick_audio_file());
            });
            self.file_rx = Some(rx);
        }
        if cancel_add {
            self.add_form = None;
        }
        if do_add {
            if let Some(form) = self.add_form.take() {
                let name = form.name.trim().to_string();
                let ok_common = !name.is_empty() && !form.path.is_empty();
                match form.kind {
                    AddKind::Voice if ok_common => {
                        cfg.voices.retain(|v| v.name != name);
                        cfg.voices.push(VoiceEntry { name: name.clone(), audio_path: form.path.clone(), preset: None, builtin: false });
                        cfg.voice = name.clone();
                        changed = true;
                        self.status = crate::i18n::voice_added(&name);
                    }
                    AddKind::Intonation if ok_common && !form.text.trim().is_empty() => {
                        cfg.intonations.retain(|v| v.name != name);
                        cfg.intonations.push(IntonationEntry {
                            name: name.clone(),
                            audio_path: Some(form.path.clone()),
                            text: Some(form.text.trim().to_string()),
                            builtin: false,
                        });
                        cfg.intonation = name.clone();
                        changed = true;
                        self.status = crate::i18n::intonation_added(&name);
                    }
                    _ => {
                        self.status = crate::i18n::t("fill_every_field").to_string();
                        self.add_form = Some(form);
                    }
                }
            }
        }
        if delete_voice {
            let sel = cfg.voice.clone();
            cfg.voices.retain(|v| !(v.name == sel && !v.builtin));
            if !cfg.voices.iter().any(|v| v.name == cfg.voice) {
                if let Some(first) = cfg.voices.first() {
                    cfg.voice = first.name.clone();
                }
            }
            changed = true;
            self.status = crate::i18n::voice_deleted(&sel);
        }
        if delete_intonation {
            let sel = cfg.intonation.clone();
            cfg.intonations.retain(|v| !(v.name == sel && !v.builtin));
            if !cfg.intonations.iter().any(|v| v.name == cfg.intonation) {
                if let Some(first) = cfg.intonations.first() {
                    cfg.intonation = first.name.clone();
                }
            }
            changed = true;
            self.status = crate::i18n::intonation_deleted(&sel);
        }

        if do_undo {
            self.snap_on_press = None;
            if let Some(prev) = self.undo.pop() {
                self.apply_to_overlay(&prev);
                prev.save();
                *self.config.lock().unwrap() = prev;
                self.status = crate::i18n::t("undone").to_string();
            }
        } else if do_default {
            self.snap_on_press = None;
            self.snapshot();
            let def = Config::default();
            self.apply_to_overlay(&def);
            def.save();
            *self.config.lock().unwrap() = def;
            self.status = crate::i18n::t("restore_defaults").to_string();
        } else if changed {
            let mut guard = self.config.lock().unwrap();
            *guard = cfg;
            guard.save();
        }

        // Commit an undo step when a gesture that mutated the config finishes.
        if !do_undo && !do_default && ctx.input(|i| i.pointer.any_released()) {
            if let Some(pre) = self.snap_on_press.take() {
                if pre != *self.config.lock().unwrap() {
                    self.undo.push(pre);
                    if self.undo.len() > 50 {
                        self.undo.remove(0);
                    }
                }
            }
        }

        // Keep the loop ticking so egui-native hotkey capture and channels stay
        // responsive while the window is focused.
        ctx.request_repaint_after(Duration::from_millis(120));
    }
}

/// egui's `Key` has no VK codes, so map the ones a user might bind. Keys egui
/// can't represent (CapsLock, etc.) fall through to the low-level hook path.
fn egui_key_to_vk(key: egui::Key) -> Option<u32> {
    use egui::Key::*;
    Some(match key {
        A => 0x41, B => 0x42, C => 0x43, D => 0x44, E => 0x45, F => 0x46, G => 0x47,
        H => 0x48, I => 0x49, J => 0x4A, K => 0x4B, L => 0x4C, M => 0x4D, N => 0x4E,
        O => 0x4F, P => 0x50, Q => 0x51, R => 0x52, S => 0x53, T => 0x54, U => 0x55,
        V => 0x56, W => 0x57, X => 0x58, Y => 0x59, Z => 0x5A,
        Num0 => 0x30, Num1 => 0x31, Num2 => 0x32, Num3 => 0x33, Num4 => 0x34,
        Num5 => 0x35, Num6 => 0x36, Num7 => 0x37, Num8 => 0x38, Num9 => 0x39,
        F1 => 0x70, F2 => 0x71, F3 => 0x72, F4 => 0x73, F5 => 0x74, F6 => 0x75,
        F7 => 0x76, F8 => 0x77, F9 => 0x78, F10 => 0x79, F11 => 0x7A, F12 => 0x7B,
        F13 => 0x7C, F14 => 0x7D, F15 => 0x7E, F16 => 0x7F, F17 => 0x80, F18 => 0x81,
        F19 => 0x82, F20 => 0x83,
        Space => 0x20, Tab => 0x09, Enter => 0x0D, Backspace => 0x08,
        Insert => 0x2D, Delete => 0x2E, Home => 0x24, End => 0x23,
        PageUp => 0x21, PageDown => 0x22,
        ArrowLeft => 0x25, ArrowUp => 0x26, ArrowRight => 0x27, ArrowDown => 0x28,
        Backtick => 0xC0, Minus => 0xBD, Equals => 0xBB, OpenBracket => 0xDB,
        CloseBracket => 0xDD, Backslash => 0xDC, Semicolon => 0xBA, Quote => 0xDE,
        Comma => 0xBC, Period => 0xBE, Slash => 0xBF,
        _ => return None,
    })
}

fn spawn_tray_thread(
    hwnd: Option<isize>,
    show_id: tray_icon::menu::MenuId,
    quit_id: tray_icon::menu::MenuId,
) {
    std::thread::spawn(move || {
        use tray_icon::menu::MenuEvent;
        use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
        loop {
            while let Ok(ev) = MenuEvent::receiver().try_recv() {
                if ev.id == quit_id {
                    std::process::exit(0);
                } else if ev.id == show_id {
                    show_window(hwnd);
                }
            }
            while let Ok(ev) = TrayIconEvent::receiver().try_recv() {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = ev
                {
                    show_window(hwnd);
                }
            }
            std::thread::sleep(Duration::from_millis(80));
        }
    });
}

#[cfg(windows)]
fn show_window(hwnd: Option<isize>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_SHOW};
    if let Some(h) = hwnd {
        unsafe {
            let hwnd = HWND(h as *mut core::ffi::c_void);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

#[cfg(windows)]
fn hide_window(hwnd: Option<isize>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    if let Some(h) = hwnd {
        unsafe {
            let _ = ShowWindow(HWND(h as *mut core::ffi::c_void), SW_HIDE);
        }
    }
}

#[cfg(not(windows))]
fn show_window(_hwnd: Option<isize>) {}
#[cfg(not(windows))]
fn hide_window(_hwnd: Option<isize>) {}

/// Native "open file" dialog for picking a reference clip. Runs comdlg32's own
/// modal loop; call it off the UI thread so egui/winit keeps ticking.
#[cfg(windows)]
fn pick_audio_file() -> Option<String> {
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::UI::Controls::Dialogs::{
        GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_PATHMUSTEXIST, OPENFILENAMEW,
    };
    let mut file_buf = vec![0u16; 1024];
    let filter: Vec<u16> = crate::i18n::audio_filter()
        .encode_utf16()
        .collect();
    let title: Vec<u16> = crate::i18n::reference_audio_title().encode_utf16().collect();
    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: PWSTR(file_buf.as_mut_ptr()),
        nMaxFile: file_buf.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST,
        ..Default::default()
    };
    let ok = unsafe { GetOpenFileNameW(&mut ofn) };
    if ok.as_bool() {
        let len = file_buf.iter().position(|&c| c == 0).unwrap_or(file_buf.len());
        Some(String::from_utf16_lossy(&file_buf[..len]))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn pick_audio_file() -> Option<String> {
    None
}

#[cfg(windows)]
fn pick_folder() -> Option<String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let picked = (|| -> Option<String> {
            let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).ok()?;
            let opts = dialog.GetOptions().ok()?;
            dialog.SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM).ok()?;
            dialog.Show(HWND::default()).ok()?;
            let item = dialog.GetResult().ok()?;
            let pw = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
            let s = pw.to_string().ok();
            CoTaskMemFree(Some(pw.0 as *const core::ffi::c_void));
            s
        })();
        CoUninitialize();
        picked
    }
}
#[cfg(not(windows))]
fn pick_folder() -> Option<String> {
    None
}
