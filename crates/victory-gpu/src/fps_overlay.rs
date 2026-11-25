//! A frame-rate counter over the top-left corner of our window.
//!
//! A borderless layered top-most window on a thread of its own (not owned
//! by the presented window, whose thread may be the title's: an owned
//! window would join the two threads' input), clicks pass through it,
//! black is its transparent colour; it shows only while the presented
//! window is in front. Every 250 ms it reads the frames presented so far
//! (`crate::queue::presented`), follows our window's client corner and
//! draws the rate over the last second. Nothing touches the title's
//! picture or the GPU. `VICTORY_FPS_OVERLAY=0` leaves it out;
//! `VICTORY_FPS_OVERLAY_SIZE=<pixels>` sets the character height (default 54).
//!
//! Source: rorodev6/victory, commit 4ddc0c1 (`crates/victory-gpu/src/fps_overlay.rs`),
//! GPL-3.0-or-later; ported with a configurable size.

use std::sync::Mutex;
use std::time::Instant;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{PCWSTR, w};

const TIMER: usize = 1;
/// Character height in pixels when `VICTORY_FPS_OVERLAY_SIZE` is unset or unreadable.
const DEFAULT_FONT: i32 = 54;
/// Bounds of `VICTORY_FPS_OVERLAY_SIZE`.
const MIN_FONT: i32 = 16;
const MAX_FONT: i32 = 240;

/// Character height from `VICTORY_FPS_OVERLAY_SIZE`'s value. Pure, for testing.
fn parse_font(value: Option<&str>) -> i32 {
    value
        .and_then(|v| v.trim().parse::<i32>().ok())
        .map_or(DEFAULT_FONT, |n| n.clamp(MIN_FONT, MAX_FONT))
}

fn font() -> i32 {
    static FONT: std::sync::OnceLock<i32> = std::sync::OnceLock::new();
    *FONT.get_or_init(|| parse_font(std::env::var("VICTORY_FPS_OVERLAY_SIZE").ok().as_deref()))
}

/// Window size for a character height: room for "999.9 fps" in a bold monospace font.
fn window_size(font: i32) -> (i32, i32) {
    (font * 520 / DEFAULT_FONT, font * 68 / DEFAULT_FONT)
}
/// The colour drawn as transparent.
const KEY: COLORREF = COLORREF(0);

/// What the overlay knows: our window, the samples of the last second,
/// and the text shown.
struct State {
    owner: HWND,
    samples: Vec<(Instant, u64)>,
    text: Vec<u16>,
}

// SAFETY: the window handle is only used on the window thread.
unsafe impl Send for State {}

static STATE: Mutex<Option<State>> = Mutex::new(None);

fn wanted() -> bool {
    std::env::var_os("VICTORY_FPS_OVERLAY").is_none_or(|v| v != "0")
}

/// Puts the counter over `owner`, once per process, on a thread of its own.
pub(crate) fn start(owner: HWND) {
    static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !wanted() || STARTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let owner = owner.0 as usize;
    let _ = std::thread::Builder::new()
        .name("victory-fps".into())
        .spawn(move || run(HWND(owner as *mut _)));
}

fn run(owner: HWND) {
    const CLASS: PCWSTR = w!("VIctoryFps");
    // SAFETY: registers our class once and creates a window of it, owned
    // by ours, on this thread.
    unsafe {
        let Ok(instance) = GetModuleHandleW(None) else {
            return;
        };
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc_),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            ..Default::default()
        };
        RegisterClassExW(&class);
        *STATE.lock().unwrap_or_else(|e| e.into_inner()) = Some(State {
            owner,
            samples: Vec::new(),
            text: "-- fps".encode_utf16().collect(),
        });
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST,
            CLASS,
            w!("VIctory fps"),
            WS_POPUP,
            0,
            0,
            window_size(font()).0,
            window_size(font()).1,
            None,
            None,
            Some(instance.into()),
            None,
        ) else {
            tracing::warn!("the frame-rate counter's window was not made");
            return;
        };
        let _ = SetLayeredWindowAttributes(hwnd, KEY, 255, LWA_COLORKEY);
        follow(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(Some(hwnd), TIMER, 250, None);
        tracing::info!("frame-rate counter shown at the top left (VICTORY_FPS_OVERLAY=0: left out)");
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// Moves the overlay to the presented window's client corner, hidden while
/// that window is minimised, gone or not in front.
fn follow(hwnd: HWND) {
    let owner = match STATE.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
        Some(s) => s.owner,
        None => return,
    };
    // SAFETY: plain window queries and a move of our own window.
    unsafe {
        if !IsWindow(Some(owner)).as_bool() {
            let _ = DestroyWindow(hwnd);
            PostQuitMessage(0);
            return;
        }
        if IsIconic(owner).as_bool() || GetForegroundWindow() != owner {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return;
        }
        let mut corner = POINT { x: 8, y: 6 };
        let _ = ClientToScreen(owner, &mut corner);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            corner.x,
            corner.y,
            window_size(font()).0,
            window_size(font()).1,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

/// Takes a sample and makes the text: frames over the last second or so.
fn sample() {
    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    let Some(state) = guard.as_mut() else {
        return;
    };
    let now = Instant::now();
    state.samples.push((now, crate::queue::presented()));
    state.samples.retain(|(t, _)| now.duration_since(*t).as_secs_f64() <= 1.25);
    let (Some(&(t0, f0)), Some(&(t1, f1))) = (state.samples.first(), state.samples.last()) else {
        return;
    };
    let seconds = t1.duration_since(t0).as_secs_f64();
    if seconds < 0.2 {
        return;
    }
    let fps = (f1 - f0) as f64 / seconds;
    let text = if fps > 0.0 {
        format!("{fps:.1} fps  {:.0} ms", 1000.0 / fps)
    } else {
        "0.0 fps".to_string()
    };
    state.text = text.encode_utf16().collect();
}

fn paint(hwnd: HWND) {
    let text = STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(|s| s.text.clone())
        .unwrap_or_default();
    // SAFETY: painting our own window between BeginPaint and EndPaint; the
    // font made here is released before returning.
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut ps);
        let (width, height) = window_size(font());
        let rect = RECT { left: 0, top: 0, right: width, bottom: height };
        FillRect(dc, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0));
        let font = CreateFontW(
            font(),
            0,
            0,
            0,
            FW_BOLD.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            NONANTIALIASED_QUALITY,
            0,
            w!("Consolas"),
        );
        let old = SelectObject(dc, font.into());
        SetBkMode(dc, TRANSPARENT);
        // A dark outline (not the transparent black), then the text.
        SetTextColor(dc, COLORREF(0x0001_0101));
        for (dx, dy) in [(-3, 0), (3, 0), (0, -3), (0, 3), (-3, -3), (3, 3), (-3, 3), (3, -3)] {
            let _ = TextOutW(dc, 6 + dx, 5 + dy, &text);
        }
        // Yellow (COLORREF is 0x00BBGGRR).
        SetTextColor(dc, COLORREF(0x0000_FFFF));
        let _ = TextOutW(dc, 6, 5, &text);
        SelectObject(dc, old);
        let _ = DeleteObject(font.into());
        let _ = EndPaint(hwnd, &ps);
    }
}

extern "system" fn proc_(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_TIMER => {
            sample();
            follow(hwnd);
            // SAFETY: asks our own window to be painted again.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint(hwnd);
            LRESULT(0)
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        // SAFETY: the default handling for our window.
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_defaults_and_is_bounded() {
        assert_eq!(parse_font(None), DEFAULT_FONT);
        assert_eq!(parse_font(Some("abc")), DEFAULT_FONT);
        assert_eq!(parse_font(Some(" 90 ")), 90);
        assert_eq!(parse_font(Some("4")), MIN_FONT);
        assert_eq!(parse_font(Some("9000")), MAX_FONT);
    }

    #[test]
    fn the_window_grows_with_the_font() {
        assert_eq!(window_size(DEFAULT_FONT), (520, 68));
        assert_eq!(window_size(DEFAULT_FONT * 2), (1040, 136));
    }
}
