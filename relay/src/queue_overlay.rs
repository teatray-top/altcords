use std::sync::{Mutex, OnceLock};

use windows::core::w;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC,
    InvalidateRect, ReleaseDC, SelectObject, SetBkMode, SetTextColor, DT_CALCRECT, DT_LEFT,
    DT_WORDBREAK, FW_NORMAL, HFONT, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetSystemMetrics, GetWindowRect, LoadCursorW, PostMessageW,
    RegisterClassExW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, HTBOTTOM, HTBOTTOMLEFT,
    HTBOTTOMRIGHT, HTCAPTION, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT, IDC_ARROW, LWA_ALPHA,
    SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, WM_APP,
    WM_EXITSIZEMOVE, WM_NCHITTEST, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

use crate::config::SharedConfig;

const RESIZE_MARGIN: i32 = 8;
const MARGIN_X: i32 = 16;
const MARGIN_Y: i32 = 10;

const WM_QUEUE_SHOW: u32 = WM_APP + 21;
const WM_QUEUE_HIDE: u32 = WM_APP + 22;
const WM_QUEUE_RELAYOUT: u32 = WM_APP + 23; // re-wrap to text height + reposition + repaint
const WM_QUEUE_EDITMODE: u32 = WM_APP + 24; // wparam=0/1

struct QueueState {
    hwnd: HWND,
    font: HFONT,
    text: String,
    bg_colorref: u32,
    fg_colorref: u32,
    edit_mode: bool,
    config: SharedConfig,
}

unsafe impl Send for QueueState {}

static STATE: OnceLock<Mutex<QueueState>> = OnceLock::new();

fn rgb_to_colorref(rgb: [u8; 3]) -> u32 {
    (rgb[2] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[0] as u32
}

fn with_state<R>(f: impl FnOnce(&mut QueueState) -> R) -> Option<R> {
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
                DrawTextW(hdc, &mut wide, &mut r, DT_LEFT | DT_WORDBREAK);
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
            let (left, right) = (sx < rc.left + m, sx >= rc.right - m);
            let (top, bottom) = (sy < rc.top + m, sy >= rc.bottom - m);
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
                    c.queue_x_frac = x_frac.clamp(0.0, 1.0);
                    c.queue_bottom_offset = bottom_offset.max(0);
                    c.queue_w = w.max(80);
                    c.queue_h = h.max(30);
                    c.save();
                }
            });
            LRESULT(0)
        }
        WM_QUEUE_SHOW => {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            LRESULT(0)
        }
        WM_QUEUE_HIDE => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            LRESULT(0)
        }
        WM_QUEUE_RELAYOUT => {
            relayout(hwnd);
            LRESULT(0)
        }
        WM_QUEUE_EDITMODE => {
            let on = wparam.0 != 0;
            with_state(|s| s.edit_mode = on);
            if on {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = InvalidateRect(hwnd, None, true);
            } else {
                // Hide unless something is currently queued.
                let empty = with_state(|s| s.text.is_empty()).unwrap_or(true);
                if empty {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Size to the wrapped text height at the configured width, then bottom-anchor at
/// (queue_x_frac, queue_bottom_offset) growing upward — same as the input overlay.
unsafe fn relayout(hwnd: HWND) {
    let Some((font, text, w, base_h, bottom_offset, x_frac)) = STATE
        .get()
        .and_then(|m| m.lock().ok())
        .map(|s| {
            let c = s.config.lock().unwrap();
            (s.font, s.text.clone(), c.queue_w, c.queue_h, c.queue_bottom_offset, c.queue_x_frac)
        })
    else {
        return;
    };

    let screen_w = GetSystemMetrics(SM_CXSCREEN);
    let screen_h = GetSystemMetrics(SM_CYSCREEN);

    let hdc = GetDC(hwnd);
    let prev = SelectObject(hdc, font);
    let mut r = RECT { left: 0, top: 0, right: (w - 2 * MARGIN_X).max(50), bottom: 0 };
    let mut wide: Vec<u16> = if text.is_empty() { vec![b' ' as u16] } else { text.encode_utf16().collect() };
    DrawTextW(hdc, &mut wide, &mut r, DT_CALCRECT | DT_WORDBREAK | DT_LEFT);
    SelectObject(hdc, prev);
    ReleaseDC(hwnd, hdc);

    let text_h = (r.bottom - r.top).max(0);
    let new_h = (text_h + 2 * MARGIN_Y).max(base_h);
    let x = (screen_w as f32 * x_frac) as i32 - w / 2;
    let bottom = screen_h - bottom_offset;
    let mut top = bottom - new_h;
    if top < 0 {
        top = 0;
    }
    let _ = SetWindowPos(hwnd, None, x, top, w, new_h, SWP_NOZORDER | SWP_NOACTIVATE);
    let _ = InvalidateRect(hwnd, None, true);
}

fn post(msg: u32, w: usize, l: isize) {
    let Some(hwnd) = STATE.get().and_then(|m| m.lock().ok()).map(|s| s.hwnd) else {
        return;
    };
    unsafe {
        let _ = PostMessageW(hwnd, msg, WPARAM(w), LPARAM(l));
    }
}

pub fn create(config: SharedConfig) -> anyhow::Result<()> {
    let (x_frac, bottom_offset, w, h, bg, alpha, fg) = {
        let c = config.lock().unwrap();
        (c.queue_x_frac, c.queue_bottom_offset, c.queue_w, c.queue_h, c.overlay_bg, c.overlay_alpha, c.overlay_fg)
    };
    unsafe {
        let hmodule = GetModuleHandleW(None)?;
        let hinstance: windows::Win32::Foundation::HINSTANCE = hmodule.into();
        let class_name = w!("AltCordsQueueClass");
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

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w as f32 * x_frac) as i32 - w / 2;
        let y = screen_h - h - bottom_offset;
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
            class_name,
            w!("AltCords Queue"),
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
            22, 0, 0, 0, FW_NORMAL.0 as i32, 0, 0, 0,
            windows::Win32::Graphics::Gdi::DEFAULT_CHARSET.0 as u32,
            windows::Win32::Graphics::Gdi::OUT_DEFAULT_PRECIS.0 as u32,
            windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS.0 as u32,
            windows::Win32::Graphics::Gdi::CLEARTYPE_QUALITY.0 as u32,
            windows::Win32::Graphics::Gdi::FF_DONTCARE.0 as u32,
            w!("Malgun Gothic"),
        );

        STATE
            .set(Mutex::new(QueueState {
                hwnd,
                font,
                text: String::new(),
                bg_colorref: rgb_to_colorref(bg),
                fg_colorref: rgb_to_colorref(fg),
                edit_mode: false,
                config,
            }))
            .map_err(|_| anyhow::anyhow!("queue overlay already created"))?;
    }
    Ok(())
}

pub fn set_edit_mode(on: bool) {
    if on {
        set_text(crate::i18n::t("playing"));
    }
    post(WM_QUEUE_EDITMODE, on as usize, 0);
}

pub fn show() {
    post(WM_QUEUE_SHOW, 0, 0);
}

pub fn hide() {
    post(WM_QUEUE_HIDE, 0, 0);
}

pub fn set_text(text: &str) {
    let Some(state_mutex) = STATE.get() else { return };
    let hwnd = {
        let Ok(mut state) = state_mutex.lock() else { return };
        state.text = text.to_string();
        state.hwnd
    };
    unsafe {
        let _ = PostMessageW(hwnd, WM_QUEUE_RELAYOUT, WPARAM(0), LPARAM(0));
    }
}
