use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, OnceLock};

use crate::playqueue::PlayQueue;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, WH_KEYBOARD_LL,
    WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

use crate::config::SharedConfig;
use crate::hangul::HangulComposer;

const VK_CONTROL: u32 = 0x11;
const VK_LCONTROL: u32 = 0xA2;
const VK_RCONTROL: u32 = 0xA3;
const VK_SHIFT: u32 = 0x10;
const VK_LSHIFT: u32 = 0xA0;
const VK_RSHIFT: u32 = 0xA1;
const VK_MENU: u32 = 0x12; // Alt
const VK_LMENU: u32 = 0xA4;
const VK_RMENU: u32 = 0xA5;
const VK_CAPITAL: u32 = 0x14;
const VK_HANGUL: u32 = 0x15; // 한/영 key (IME language toggle on Korean keyboards)
const VK_BACK: u32 = 0x08;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_OEM_COMMA: u32 = 0xBC;
const VK_OEM_PERIOD: u32 = 0xBE;
const VK_OEM_2: u32 = 0xBF; // '/' key — '?' with shift
const VK_V: u32 = 0x56; // for Ctrl+V paste

/// Maps digit/punctuation keys to their typed character, honoring shift for
/// the standard US-layout shifted symbols. Sentence-final punctuation (.,?!)
/// matters for TTS prosody — without it every message reads as a flat,
/// unterminated clause.
fn vk_to_char(vk: u32, shift: bool) -> Option<char> {
    // Numpad (NumLock on): VK_NUMPAD0..9 = 0x60..0x69, then the operators.
    if (0x60..=0x69).contains(&vk) {
        return Some((b'0' + (vk - 0x60) as u8) as char);
    }
    match vk {
        0x6A => return Some('*'), // VK_MULTIPLY
        0x6B => return Some('+'), // VK_ADD
        0x6D => return Some('-'), // VK_SUBTRACT
        0x6E => return Some('.'), // VK_DECIMAL
        0x6F => return Some('/'), // VK_DIVIDE
        _ => {}
    }
    if (0x30..=0x39).contains(&vk) {
        let digit = vk as u8 as char;
        return Some(if shift {
            match digit {
                '1' => '!', '2' => '@', '3' => '#', '4' => '$', '5' => '%',
                '6' => '^', '7' => '&', '8' => '*', '9' => '(', '0' => ')',
                _ => unreachable!(),
            }
        } else {
            digit
        });
    }
    Some(match vk {
        VK_OEM_COMMA => ',',
        VK_OEM_PERIOD => '.',
        VK_OEM_2 => {
            if shift {
                '?'
            } else {
                '/'
            }
        }
        _ => return None,
    })
}

/// (ctrl, alt, shift, vk) captured for a hotkey. vk == 0 means capture was
/// cancelled (Escape).
pub type HotkeyCombo = (bool, bool, bool, u32);

struct HookState {
    capturing: bool,
    ctrl_down: bool,
    shift_down: bool,
    alt_down: bool,
    korean_mode: bool,
    composer: HangulComposer,
    tx: Sender<String>,
    config: SharedConfig,
    capture_hotkey: bool,
    hotkey_tx: Sender<HotkeyCombo>,
    playqueue: Arc<PlayQueue>,
}

/// Puts the hook into "capture the next key combo" mode (for the GUI's hotkey
/// picker). The captured combo is sent on the hotkey channel.
pub fn request_hotkey_capture() {
    if let Some(s) = STATE.get() {
        if let Ok(mut st) = s.lock() {
            st.capture_hotkey = true;
        }
    }
}

/// Clears the low-level hook's capture flag — used when the GUI captured the
/// hotkey itself (via its own focused key events) so the hook doesn't also
/// grab and swallow the next keystroke.
pub fn cancel_hotkey_capture() {
    if let Some(s) = STATE.get() {
        if let Ok(mut st) = s.lock() {
            st.capture_hotkey = false;
        }
    }
}

static STATE: OnceLock<Mutex<HookState>> = OnceLock::new();

fn is_ctrl_vk(vk: u32) -> bool {
    vk == VK_CONTROL || vk == VK_LCONTROL || vk == VK_RCONTROL
}

fn is_shift_vk(vk: u32) -> bool {
    vk == VK_SHIFT || vk == VK_LSHIFT || vk == VK_RSHIFT
}

fn is_alt_vk(vk: u32) -> bool {
    vk == VK_MENU || vk == VK_LMENU || vk == VK_RMENU
}

/// Renders the composed text, first dropping any syllable the model cannot
/// pronounce (config.blocked_syllables) and explaining the removal in a toast —
/// silently deleting a character the user typed would just look like a bug.
fn update_overlay(state: &mut HookState, _text: &str) {
    let blocked = state.config.lock().unwrap().blocked_syllables.clone();
    let hit = state.composer.strip_blocked(&blocked);
    let text = state.composer.text();
    match hit {
        Some(c) => crate::overlay::toast(&text, &crate::i18n::syllable_blocked(c)),
        None => crate::overlay::set_text(&text),
    }
}

/// Reads the clipboard's Unicode text (CF_UNICODETEXT) for Ctrl+V paste. Returns
/// None if the clipboard has no text or can't be opened.
fn get_clipboard_text() -> Option<String> {
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }
        let text = (|| {
            let handle = GetClipboardData(13).ok()?; // 13 = CF_UNICODETEXT
            let hglobal = HGLOBAL(handle.0);
            let ptr = GlobalLock(hglobal) as *const u16;
            if ptr.is_null() {
                return None;
            }
            let mut len = 0usize;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(hglobal);
            Some(s)
        })();
        let _ = CloseClipboard();
        text
    }
}

/// Sends a synthetic CapsLock down+up. Windows flips the CapsLock toggle bit
/// (and LED) at a level below WH_KEYBOARD_LL — swallowing the real keypress
/// (returning nonzero from the hook) does not stop that, so the only way to
/// keep CapsLock's actual lock state unaffected by our hotkey is to send a
/// second, compensating press that flips it back. Marked LLKHF_INJECTED, so
/// hook_proc ignores it instead of reprocessing it as user input.
fn compensate_capslock_toggle() {
    unsafe {
        let mut down = INPUT::default();
        down.r#type = INPUT_KEYBOARD;
        down.Anonymous.ki = KEYBDINPUT {
            wVk: VIRTUAL_KEY(VK_CAPITAL as u16),
            wScan: 0,
            dwFlags: Default::default(),
            time: 0,
            dwExtraInfo: 0,
        };
        let mut up = down;
        up.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
        SendInput(&[down, up], std::mem::size_of::<INPUT>() as i32);
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code as u32 != HC_ACTION {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
    if kb.flags.0 & LLKHF_INJECTED.0 != 0 {
        return CallNextHookEx(None, code, wparam, lparam);
    }
    let msg = wparam.0 as u32;
    let is_down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    let is_up = msg == WM_KEYUP || msg == WM_SYSKEYUP;
    let vk = kb.vkCode;

    let Some(state_mutex) = STATE.get() else {
        return CallNextHookEx(None, code, wparam, lparam);
    };
    let Ok(mut state) = state_mutex.lock() else {
        return CallNextHookEx(None, code, wparam, lparam);
    };

    // Ctrl-up always passes through and is never swallowed, in either mode.
    // Ctrl-down (while not capturing) also passes through normally — we
    // can't know in advance whether CapsLock will follow, and swallowing
    // every ctrl-down would break ctrl shortcuts system-wide. Once capturing
    // starts, ctrl-down IS swallowed (below), so its matching ctrl-up must
    // still be forwarded here or the target app would see ctrl stuck "held".
    if is_ctrl_vk(vk) && is_up {
        state.ctrl_down = false;
        return CallNextHookEx(None, code, wparam, lparam);
    }

    if !state.capturing {
        if is_ctrl_vk(vk) {
            state.ctrl_down = is_down;
        } else if is_shift_vk(vk) {
            state.shift_down = is_down;
        } else if is_alt_vk(vk) {
            state.alt_down = is_down;
        } else if is_down && state.capture_hotkey {
            state.capture_hotkey = false;
            let combo = if vk == VK_ESCAPE {
                (false, false, false, 0) // cancelled
            } else {
                (state.ctrl_down, state.alt_down, state.shift_down, vk)
            };
            let _ = state.hotkey_tx.send(combo);
            return LRESULT(1); // swallow the captured key
        } else if is_down {
            let (enabled, hk_ctrl, hk_alt, hk_shift, hk_vk, s_ctrl, s_alt, s_shift, s_vk) = {
                let cfg = state.config.lock().unwrap();
                (
                    cfg.enabled, cfg.hotkey_ctrl, cfg.hotkey_alt, cfg.hotkey_shift, cfg.hotkey_vk,
                    cfg.stop_hotkey_ctrl, cfg.stop_hotkey_alt, cfg.stop_hotkey_shift, cfg.stop_hotkey_vk,
                )
            };
            // Stop/barge-in hotkey: cut current playback + clear the queue.
            let stop_match = state.ctrl_down == s_ctrl
                && state.alt_down == s_alt
                && state.shift_down == s_shift;
            if s_vk != 0 && vk == s_vk && stop_match {
                state.playqueue.stop();
                crate::playqueue::refresh_overlay(&state.playqueue);
                return LRESULT(1);
            }
            let mods_match = state.ctrl_down == hk_ctrl
                && state.alt_down == hk_alt
                && state.shift_down == hk_shift;
            if enabled && vk == hk_vk && mods_match {
                state.composer.reset();
                state.capturing = true;
                state.korean_mode = true;
                update_overlay(&mut state, "");
                crate::overlay::show();
                // CapsLock's toggle bit flips below the hook; undo it only when
                // CapsLock is actually the trigger key.
                if hk_vk == VK_CAPITAL {
                    compensate_capslock_toggle();
                }
                return LRESULT(1); // swallow the triggering keypress itself
            }
        }
        return CallNextHookEx(None, code, wparam, lparam);
    }

    // Capturing mode: every event is swallowed; only key-down is actionable.
    if is_down {
        if is_ctrl_vk(vk) {
            state.ctrl_down = true;
        } else if is_shift_vk(vk) {
            state.shift_down = true;
        } else if is_alt_vk(vk) {
            state.alt_down = true;
        } else if state.ctrl_down && vk == VK_V {
            if let Some(pasted) = get_clipboard_text() {
                let clean: String = pasted.chars().filter(|&c| c != '\r').collect();
                let text = state.composer.feed_str(&clean);
                update_overlay(&mut state, &text);
            }
        } else if vk == VK_HANGUL {
            state.korean_mode = !state.korean_mode;
            let text = state.composer.text();
            update_overlay(&mut state, &text);
        } else if vk == VK_RETURN {
            let text = state.composer.finalize();
            state.capturing = false;
            crate::overlay::hide(); // input text window; the queue has its own
            let _ = state.tx.send(text);
        } else if vk == VK_ESCAPE {
            state.composer.reset();
            state.capturing = false;
            crate::overlay::hide();
        } else if vk == VK_BACK {
            let text = state.composer.backspace();
            update_overlay(&mut state, &text);
        } else if vk == VK_SPACE {
            let text = state.composer.feed_key(' ');
            update_overlay(&mut state, &text);
        } else if let Some(ch) = vk_to_char(vk, state.shift_down) {
            let text = state.composer.feed_key(ch);
            update_overlay(&mut state, &text);
        } else if (0x41..=0x5A).contains(&vk) {
            let base = vk as u8 as char;
            let ch = if state.shift_down { base } else { base.to_ascii_lowercase() };
            let text = if state.korean_mode {
                state.composer.feed_key(ch)
            } else {
                state.composer.feed_literal(ch)
            };
            update_overlay(&mut state, &text);
        }
    } else if is_up {
        if is_shift_vk(vk) {
            state.shift_down = false;
        } else if is_alt_vk(vk) {
            state.alt_down = false;
        }
    }

    LRESULT(1) // swallow everything while capturing
}

/// RAII guard that unhooks the keyboard hook on drop.
pub struct HookGuard {
    #[allow(dead_code)]
    hook: HHOOK,
}

impl Drop for HookGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.hook);
        }
    }
}

pub fn install(
    tx: Sender<String>,
    config: SharedConfig,
    hotkey_tx: Sender<HotkeyCombo>,
    playqueue: Arc<PlayQueue>,
) -> anyhow::Result<HookGuard> {
    STATE
        .set(Mutex::new(HookState {
            capturing: false,
            ctrl_down: false,
            shift_down: false,
            alt_down: false,
            korean_mode: true,
            composer: HangulComposer::new(),
            tx,
            config,
            capture_hotkey: false,
            hotkey_tx,
            playqueue,
        }))
        .map_err(|_| anyhow::anyhow!("hook already installed"))?;

    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), None, 0) }?;
    Ok(HookGuard { hook })
}

pub fn run_message_loop() {
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numpad_maps_to_digits_and_operators() {
        for (vk, ch) in (0x60u32..=0x69).zip('0'..='9') {
            assert_eq!(vk_to_char(vk, false), Some(ch));
        }
        assert_eq!(vk_to_char(0x6A, false), Some('*'));
        assert_eq!(vk_to_char(0x6B, false), Some('+'));
        assert_eq!(vk_to_char(0x6D, false), Some('-'));
        assert_eq!(vk_to_char(0x6E, false), Some('.'));
        assert_eq!(vk_to_char(0x6F, false), Some('/'));
        // main-row digits still work; shift still gives symbols
        assert_eq!(vk_to_char(0x31, false), Some('1'));
        assert_eq!(vk_to_char(0x31, true), Some('!'));
    }
}
