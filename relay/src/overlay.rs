use std::sync::{Mutex, OnceLock};

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC,
    InvalidateRect, ReleaseDC, SelectObject, SetBkMode, SetTextColor, DT_CALCRECT, DT_CENTER,
    DT_WORDBREAK, FW_NORMAL, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetSystemMetrics, GetWindowRect, LoadCursorW, PostMessageW,
    RegisterClassExW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, HTBOTTOM, HTBOTTOMLEFT,
    HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, LWA_ALPHA,
    SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE,
    SW_SHOWNOACTIVATE, WM_APP, WM_EXITSIZEMOVE, WM_NCHITTEST, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::config::SharedConfig;

const RESIZE_MARGIN: i32 = 8;
const MARGIN_X: i32 = 24; // horizontal text inset (wrap width = client_w - 2*MARGIN_X)
const MARGIN_Y: i32 = 14; // vertical text inset / padding above+below the text

const WM_OVERLAY_SHOW: u32 = WM_APP + 1;
const WM_OVERLAY_HIDE: u32 = WM_APP + 2;
const WM_OVERLAY_REPAINT: u32 = WM_APP + 3;
const WM_OVERLAY_REPOS: u32 = WM_APP + 4; // wparam=x, lparam=y
const WM_OVERLAY_RESIZE: u32 = WM_APP + 5; // wparam=w, lparam=h
const WM_OVERLAY_APPEARANCE: u32 = WM_APP + 6; // wparam=colorref(BGR), lparam=alpha
const WM_OVERLAY_EDITMODE: u32 = WM_APP + 7; // wparam=0/1
const WM_OVERLAY_TEXTCOLOR: u32 = WM_APP + 8; // wparam=colorref(BGR)
const WM_OVERLAY_RELAYOUT: u32 = WM_APP + 9; // re-wrap: size to text height, reposition, repaint

struct OverlayState {
    hwnd: HWND,
    font: HFONT,
    text: String,
    bg_colorref: u32,
    fg_colorref: u32,
    edit_mode: bool,
    config: SharedConfig,
}

unsafe impl Send for OverlayState {}

static STATE: OnceLock<Mutex<OverlayState>> = OnceLock::new();

fn rgb_to_colorref(rgb: [u8; 3]) -> u32 {
    (rgb[2] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[0] as u32
}

fn with_state<R>(f: impl FnOnce(&mut OverlayState) -> R) -> Option<R> {
    STATE.get().and_then(|m| m.lock().ok()).map(|mut s| f(&mut s))
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        windows::Win32::UI::WindowsAndMessaging::WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rect = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);

            let (bg, fg, font, text) = STATE
                .get()
                .and_then(|m| m.lock().ok())
                .map(|s| (s.bg_colorref, s.fg_colorref, s.font, s.text.clone()))
                .unwrap_or((0x00201A14, 0x00E8F4FA, HFONT::default(), String::new()));

            let bg_brush = CreateSolidBrush(COLORREF(bg));
            FillRect(hdc, &rect, bg_brush);
            let _ = DeleteObject(bg_brush);

            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, COLORREF(fg));
            let prev_font = SelectObject(hdc, font);
            let mut wide: Vec<u16> = text.encode_utf16().collect();
            if !wide.is_empty() {
                let mut r = rect;
                r.left += MARGIN_X;
                r.right -= MARGIN_X;
                r.top += MARGIN_Y;
                r.bottom -= MARGIN_Y;
                DrawTextW(hdc, &mut wide, &mut r, DT_CENTER | DT_WORDBREAK);
            }
            SelectObject(hdc, prev_font);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_NCHITTEST => {
            let edit = STATE.get().and_then(|m| m.lock().ok()).map(|s| s.edit_mode).unwrap_or(false);
            if !edit {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            }
            let sx = (lparam.0 & 0xFFFF) as u16 as i16 as i32;
            let sy = ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
            let mut rc = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rc);
            let m = RESIZE_MARGIN;
            let left = sx < rc.left + m;
            let right = sx >= rc.right - m;
            let top = sy < rc.top + m;
            let bottom = sy >= rc.bottom - m;
            let ht = match (left, right, top, bottom) {
                (true, _, true, _) => HTTOPLEFT,
                (_, true, true, _) => HTTOPRIGHT,
                (true, _, _, true) => HTBOTTOMLEFT,
                (_, true, _, true) => HTBOTTOMRIGHT,
                (true, ..) => HTLEFT,
                (_, true, ..) => HTRIGHT,
                (_, _, true, _) => HTTOP,
                (_, _, _, true) => HTBOTTOM,
                _ => HTCAPTION,
            };
            LRESULT(ht as isize)
        }
        WM_EXITSIZEMOVE => {
            // Drag/resize finished — persist the new geometry to config.
            let mut rc = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rc);
            let w = rc.right - rc.left;
            let h = rc.bottom - rc.top;
            let screen_w = GetSystemMetrics(SM_CXSCREEN);
            let screen_h = GetSystemMetrics(SM_CYSCREEN);
            let x_frac = (rc.left + w / 2) as f32 / screen_w as f32;
            let bottom_offset = screen_h - rc.bottom;
            with_state(|s| {
                if let Ok(mut c) = s.config.lock() {
                    c.overlay_x_frac = x_frac.clamp(0.0, 1.0);
                    c.overlay_bottom_offset = bottom_offset.max(0);
                    c.overlay_w = w.max(80);
                    c.overlay_h = h.max(30);
                    c.save();
                }
            });
            LRESULT(0)
        }
        WM_OVERLAY_SHOW => {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            LRESULT(0)
        }
        WM_OVERLAY_HIDE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_OVERLAY_REPAINT => {
            let _ = InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_OVERLAY_RELAYOUT => {
            relayout(hwnd);
            LRESULT(0)
        }
        WM_OVERLAY_REPOS => {
            let x = wparam.0 as i32;
            let y = lparam.0 as i32;
            let _ = SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            LRESULT(0)
        }
        WM_OVERLAY_RESIZE => {
            let w = wparam.0 as i32;
            let h = lparam.0 as i32;
            let _ = SetWindowPos(hwnd, None, 0, 0, w, h, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
            let _ = InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_OVERLAY_APPEARANCE => {
            let colorref = wparam.0 as u32;
            let alpha = (lparam.0 as u32 & 0xFF) as u8;
            if let Some(state_mutex) = STATE.get() {
                if let Ok(mut s) = state_mutex.lock() {
                    s.bg_colorref = colorref;
                }
            }
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
            let _ = InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_OVERLAY_TEXTCOLOR => {
            let colorref = wparam.0 as u32;
            if let Some(state_mutex) = STATE.get() {
                if let Ok(mut s) = state_mutex.lock() {
                    s.fg_colorref = colorref;
                }
            }
            let _ = InvalidateRect(hwnd, None, true);
            LRESULT(0)
        }
        WM_OVERLAY_EDITMODE => {
            let on = wparam.0 != 0;
            if let Some(state_mutex) = STATE.get() {
                if let Ok(mut s) = state_mutex.lock() {
                    s.edit_mode = on;
                }
            }
            if on {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = InvalidateRect(hwnd, None, true);
            } else {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Re-wrap the overlay to fit the current text: measure the word-wrapped height
/// at the fixed width, then size the window to it. The box is bottom-anchored —
/// its bottom edge stays where the single-line box sits and it grows upward as
/// the text gets taller (clamped to the top of the screen).
unsafe fn relayout(hwnd: HWND) {
    let Some((font, text, w, base_h, bottom_offset, x_frac)) = STATE
        .get()
        .and_then(|m| m.lock().ok())
        .map(|s| {
            let c = s.config.lock().unwrap();
            (s.font, s.text.clone(), c.overlay_w, c.overlay_h, c.overlay_bottom_offset, c.overlay_x_frac)
        })
    else {
        return;
    };

    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);

    let hdc = GetDC(hwnd);
    let prev = SelectObject(hdc, font);
    let mut r = RECT { left: 0, top: 0, right: (w - 2 * MARGIN_X).max(50), bottom: 0 };
    let mut wide: Vec<u16> = if text.is_empty() {
        vec![b' ' as u16]
    } else {
        text.encode_utf16().collect()
    };
    DrawTextW(hdc, &mut wide, &mut r, DT_CALCRECT | DT_WORDBREAK | DT_CENTER);
    SelectObject(hdc, prev);
    ReleaseDC(hwnd, hdc);

    let text_h = (r.bottom - r.top).max(0);
    let new_h = (text_h + 2 * MARGIN_Y).max(base_h);

    // Bottom-anchored: keep the box's bottom edge where the single-line box sits
    // (screen_h - bottom_offset) and grow upward as the text gets taller.
    let x = (screen_w as f32 * x_frac) as i32 - w / 2;
    let bottom = screen_h - bottom_offset;
    let mut top = bottom - new_h;
    if top < 0 {
        top = 0; // never run off the top of the screen
    }
    let _ = SetWindowPos(hwnd, None, x, top, w, new_h, SWP_NOZORDER | SWP_NOACTIVATE);
    let _ = InvalidateRect(hwnd, None, true);
}

fn position_for(x_frac: f32, bottom_offset: i32, w: i32, h: i32) -> (i32, i32) {
    unsafe {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w as f32 * x_frac) as i32 - w / 2;
        let y = screen_h - h - bottom_offset;
        (x, y)
    }
}

fn post(msg: u32, w: usize, l: isize) {
    let Some(hwnd) = STATE.get().and_then(|m| m.lock().ok()).map(|s| s.hwnd) else {
        return;
    };
    unsafe {
        let _ = PostMessageW(hwnd, msg, WPARAM(w), LPARAM(l));
    }
}

pub fn set_position(x_frac: f32, bottom_offset: i32) {
    let wh = with_state(|s| {
        let c = s.config.lock().unwrap();
        (c.overlay_w, c.overlay_h)
    });
    let (w, h) = wh.unwrap_or((700, 70));
    let (x, y) = position_for(x_frac, bottom_offset, w, h);
    post(WM_OVERLAY_REPOS, x as usize, y as isize);
}

pub fn set_size(w: i32, h: i32) {
    post(WM_OVERLAY_RESIZE, w as usize, h as isize);
}

pub fn set_appearance(bg: [u8; 3], alpha: u8) {
    post(WM_OVERLAY_APPEARANCE, rgb_to_colorref(bg) as usize, alpha as isize);
}

pub fn set_text_color(fg: [u8; 3]) {
    post(WM_OVERLAY_TEXTCOLOR, rgb_to_colorref(fg) as usize, 0);
}

pub fn set_edit_mode(on: bool) {
    if on {
        set_text(crate::i18n::t("input_preview"));
    }
    post(WM_OVERLAY_EDITMODE, on as usize, 0);
}

pub fn create(config: SharedConfig) -> anyhow::Result<()> {
    let (x_frac, bottom_offset, w, h, bg, alpha, fg) = {
        let c = config.lock().unwrap();
        (c.overlay_x_frac, c.overlay_bottom_offset, c.overlay_w, c.overlay_h, c.overlay_bg, c.overlay_alpha, c.overlay_fg)
    };
    unsafe {
        let hmodule = GetModuleHandleW(None)?;
        let hinstance: windows::Win32::Foundation::HINSTANCE = hmodule.into();
        let class_name = w!("AltCordsOverlayClass");
        // Without a class cursor Windows shows the "app starting" (arrow+spinner)
        // cursor over the window; use the plain arrow instead.
        let hcursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            lpszClassName: class_name,
            hCursor: hcursor,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        let (x, y) = position_for(x_frac, bottom_offset, w, h);
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name,
            w!("AltCords"),
            WS_POPUP,
            x,
            y,
            w,
            h,
            None,
            None,
            hinstance,
            None,
        )?;

        SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA)?;

        let font = CreateFontW(
            28, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
            windows::Win32::Graphics::Gdi::DEFAULT_CHARSET.0 as u32,
            windows::Win32::Graphics::Gdi::OUT_DEFAULT_PRECIS.0 as u32,
            windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS.0 as u32,
            windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY.0 as u32,
            windows::Win32::Graphics::Gdi::FF_DONTCARE.0 as u32,
            w!("Malgun Gothic"),
        );

        STATE
            .set(Mutex::new(OverlayState {
                hwnd,
                font,
                text: String::new(),
                bg_colorref: rgb_to_colorref(bg),
                fg_colorref: rgb_to_colorref(fg),
                edit_mode: false,
                config,
            }))
            .map_err(|_| anyhow::anyhow!("overlay already created"))?;
    }
    Ok(())
}

pub fn show() {
    post(WM_OVERLAY_SHOW, 0, 0);
}

pub fn hide() {
    post(WM_OVERLAY_HIDE, 0, 0);
}

pub fn set_text(text: &str) {
    let Some(state_mutex) = STATE.get() else { return };
    let hwnd = {
        let Ok(mut state) = state_mutex.lock() else { return };
        state.text = text.to_string();
        state.hwnd
    };
    unsafe {
        let _ = PostMessageW(hwnd, WM_OVERLAY_RELAYOUT, WPARAM(0), LPARAM(0));
    }
}

/// Show `notice` under `text` for a moment, then fall back to `text` alone —
/// used to explain a syllable that was dropped as it was typed. Keying the
/// restore on the text means a later keystroke simply wins.
pub fn toast(text: &str, notice: &str) {
    set_text(&format!("{text}\n{notice}"));
    let restore = text.to_string();
    let with_notice = format!("{text}\n{notice}");
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(1600));
        let Some(m) = STATE.get() else { return };
        let stale = m.lock().map(|s| s.text == with_notice).unwrap_or(false);
        if stale {
            set_text(&restore);
        }
    });
}
