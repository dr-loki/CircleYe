#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;

use std::cell::RefCell;
use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi as gdi;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse as km;
use windows_sys::Win32::UI::Magnification as mag;
use windows_sys::Win32::UI::Shell as sh;
use windows_sys::Win32::UI::WindowsAndMessaging as wm;

use config::Config;

const TIMER_ID: usize = 1;
const WM_TRAY: u32 = wm::WM_APP + 1;

const MENU_TOGGLE: usize = 1;
const MENU_ZOOM_IN: usize = 2;
const MENU_ZOOM_OUT: usize = 3;
const MENU_SIZE_300: usize = 10;
const MENU_SIZE_400: usize = 11;
const MENU_SIZE_500: usize = 12;
const MENU_SIZE_600: usize = 13;
const MENU_AUTOSTART: usize = 20;
const MENU_QUIT: usize = 30;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "CircleYe";

static TOGGLE_MSG: AtomicU32 = AtomicU32::new(0);
static OLD_MAG_PROC: AtomicIsize = AtomicIsize::new(0);

#[derive(Clone, Copy)]
enum Phase {
    Closed,
    Opening(Instant),
    Open,
    Closing(Instant),
}

struct App {
    hwnd: HWND,
    hmag: HWND,
    hook: wm::HHOOK,
    icon: wm::HICON,
    config: Config,
    phase: Phase,
    center: (f64, f64),
    last_d: i32,
    last_zoom: f64,
    last_tick: Instant,
    last_save: Instant,
}

#[derive(Default)]
struct HookState {
    shift_down: bool,
    other_key: bool,
    down_at: Option<Instant>,
    last_tap: Option<Instant>,
    taps: u32,
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static HOOK_STATE: RefCell<HookState> = RefCell::new(HookState::default());
}

fn with_app<R>(f: impl FnOnce(&mut App) -> R) -> Option<R> {
    APP.with(|cell| cell.borrow_mut().as_mut().map(f))
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn colorref(rgb: u32) -> u32 {
    let r = (rgb >> 16) & 0xFF;
    let g = (rgb >> 8) & 0xFF;
    let b = rgb & 0xFF;
    (b << 16) | (g << 8) | r
}

fn ease_out_back(t: f64) -> f64 {
    let c1 = 1.70158_f64;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

fn log_line(s: &str) {
    use std::io::Write;
    let dir = config::dir();
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("circleye.log"))
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{secs}] {s}");
    }
}

fn autostart_enabled() -> bool {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(RUN_KEY)
        .and_then(|k| k.get_value::<String, _>(RUN_VALUE))
        .is_ok()
}

fn set_autostart(on: bool) {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    if on {
        if let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, winreg::enums::KEY_SET_VALUE) {
            if let Ok(exe) = std::env::current_exe() {
                let val = format!("\"{}\"", exe.display());
                let _ = key.set_value(RUN_VALUE, &val);
            }
        }
    } else if let Ok(key) = hkcu.open_subkey_with_flags(RUN_KEY, winreg::enums::KEY_SET_VALUE) {
        let _ = key.delete_value(RUN_VALUE);
    }
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let toggle_name = w("CircleYe_Toggle");
    let tm = unsafe { wm::RegisterWindowMessageW(toggle_name.as_ptr()) };
    TOGGLE_MSG.store(tm, Ordering::Relaxed);

    let mutex_name = w("Local\\CircleYe_SingleInstance");
    let _mutex = unsafe { CreateMutexW(null(), 1, mutex_name.as_ptr()) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        if tm != 0 {
            unsafe { wm::PostMessageW(wm::HWND_BROADCAST, tm, 0, 0) };
        }
        return;
    }

    let config = Config::load();
    let brush = unsafe { gdi::CreateSolidBrush(colorref(config.ring_color)) };
    let hinst = unsafe { GetModuleHandleW(null()) };

    let class_name = w("CircleYeHost");
    let wc = wm::WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(wndproc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: null_mut(),
        hCursor: unsafe { wm::LoadCursorW(null_mut(), wm::IDC_ARROW) },
        hbrBackground: brush,
        lpszMenuName: null(),
        lpszClassName: class_name.as_ptr(),
    };
    let atom = unsafe { wm::RegisterClassW(&wc) };
    if atom == 0 {
        log_line("RegisterClassW failed");
    }

    unsafe {
        mag::MagInitialize();
    }

    let host_ex = wm::WS_EX_TOPMOST
        | wm::WS_EX_TOOLWINDOW
        | wm::WS_EX_NOACTIVATE
        | wm::WS_EX_TRANSPARENT;
    let title = w("CircleYe");
    let hwnd = unsafe {
        wm::CreateWindowExW(
            host_ex,
            class_name.as_ptr(),
            title.as_ptr(),
            wm::WS_POPUP,
            0,
            0,
            400,
            400,
            null_mut(),
            null_mut(),
            hinst,
            null(),
        )
    };
    if hwnd.is_null() {
        log_line("CreateWindowExW(host) failed");
        return;
    }

    let mag_class = w("Magnifier");
    let empty = w("");
    let hmag = unsafe {
        wm::CreateWindowExW(
            wm::WS_EX_TRANSPARENT,
            mag_class.as_ptr(),
            empty.as_ptr(),
            wm::WS_CHILD | wm::WS_VISIBLE,
            0,
            0,
            100,
            100,
            hwnd,
            null_mut(),
            hinst,
            null(),
        )
    };
    if hmag.is_null() {
        log_line("CreateWindowExW(magnifier) failed");
    } else {
        unsafe {
            let old = wm::SetWindowLongPtrW(
                hmag,
                wm::GWLP_WNDPROC,
                mag_subclass as *const () as usize as isize,
            );
            OLD_MAG_PROC.store(old as isize, Ordering::Relaxed);
            let mut list = [hwnd, hmag];
            mag::MagSetWindowFilterList(hmag, mag::MW_FILTERMODE_EXCLUDE, 2, list.as_mut_ptr());
            apply_zoom(hmag, config.zoom);
        }
    }

    let icon = make_ring_icon(32, config.ring_color);

    let zoom = config.zoom;
    let diameter = config.lens_diameter;
    APP.with(|cell| {
        *cell.borrow_mut() = Some(App {
            hwnd,
            hmag,
            hook: null_mut(),
            icon,
            config,
            phase: Phase::Closed,
            center: (0.0, 0.0),
            last_d: 0,
            last_zoom: zoom,
            last_tick: Instant::now(),
            last_save: Instant::now(),
        });
    });

    let hook = unsafe {
        wm::SetWindowsHookExW(wm::WH_KEYBOARD_LL, Some(keyboard_proc), hinst, 0)
    };
    if hook.is_null() {
        log_line("SetWindowsHookExW failed");
    }
    with_app(|app| app.hook = hook);

    tray_add(hwnd, icon);
    log_line(&format!(
        "started v{} zoom={} diameter={}",
        env!("CARGO_PKG_VERSION"),
        zoom,
        diameter
    ));

    let mut msg: wm::MSG = unsafe { zeroed() };
    loop {
        let r = unsafe { wm::GetMessageW(&mut msg, null_mut(), 0, 0) };
        if r == 0 || r == -1 {
            break;
        }
        unsafe {
            wm::TranslateMessage(&msg);
            wm::DispatchMessageW(&msg);
        }
    }

    tray_delete(hwnd);
    unsafe {
        if !hook.is_null() {
            wm::UnhookWindowsHookEx(hook);
        }
        mag::MagUninitialize();
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == wm::WM_NCHITTEST {
        return wm::HTTRANSPARENT as LRESULT;
    }
    if msg == wm::WM_TIMER {
        if wparam == TIMER_ID {
            with_app(|app| tick(app));
            return 0;
        }
    }
    if msg == WM_TRAY {
        let ev = (lparam as u32) & 0xFFFF;
        if ev == wm::WM_LBUTTONUP || ev == wm::WM_RBUTTONUP {
            let snap = with_app(|app| menu_snapshot(app));
            if let Some(snap) = snap {
                let cmd = show_menu(&snap);
                if cmd != 0 {
                    with_app(|app| handle_menu(app, cmd));
                }
            }
        }
        return 0;
    }
    if msg == wm::WM_DESTROY {
        wm::PostQuitMessage(0);
        return 0;
    }
    let tm = TOGGLE_MSG.load(Ordering::Relaxed);
    if tm != 0 && msg == tm {
        with_app(|app| toggle_lens(app));
        return 0;
    }
    wm::DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn mag_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == wm::WM_NCHITTEST {
        return wm::HTTRANSPARENT as LRESULT;
    }
    let old = OLD_MAG_PROC.load(Ordering::Relaxed);
    if old != 0 {
        let f: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT =
            std::mem::transmute(old);
        return wm::CallWindowProcW(Some(f), hwnd, msg, wparam, lparam);
    }
    wm::DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return wm::CallNextHookEx(null_mut(), code, wparam, lparam);
    }
    let kb = &*(lparam as *const wm::KBDLLHOOKSTRUCT);
    let msg = wparam as u32;
    let is_up = msg == wm::WM_KEYUP || msg == wm::WM_SYSKEYUP;
    let vk = kb.vkCode;
    let is_shift =
        vk == km::VK_LSHIFT as u32 || vk == km::VK_RSHIFT as u32 || vk == km::VK_SHIFT as u32;
    let now = Instant::now();
    let mut swallow = false;

    if !is_shift {
        let active = with_app(|app| !matches!(app.phase, Phase::Closed)).unwrap_or(false);
        if active {
            if !is_up {
                if vk == km::VK_UP as u32 {
                    with_app(|app| change_zoom(app, app.config.zoom_step));
                    swallow = true;
                } else if vk == km::VK_DOWN as u32 {
                    with_app(|app| change_zoom(app, -app.config.zoom_step));
                    swallow = true;
                } else if vk == km::VK_ESCAPE as u32 {
                    with_app(|app| close_lens(app));
                    swallow = true;
                }
            } else if vk == km::VK_UP as u32
                || vk == km::VK_DOWN as u32
                || vk == km::VK_ESCAPE as u32
            {
                swallow = true;
            }
        }
    }

    if is_shift {
        HOOK_STATE.with(|st| {
            let mut s = st.borrow_mut();
            if !is_up {
                if !s.shift_down {
                    s.shift_down = true;
                    s.other_key = false;
                    s.down_at = Some(now);
                }
            } else if s.shift_down {
                s.shift_down = false;
                let dur_ok = s
                    .down_at
                    .map(|t| now.duration_since(t) < Duration::from_millis(400))
                    .unwrap_or(false);
                if !s.other_key && dur_ok {
                    let double = s
                        .last_tap
                        .map(|t| now.duration_since(t) <= Duration::from_millis(500))
                        .unwrap_or(false);
                    s.taps = if double { s.taps + 1 } else { 1 };
                    s.last_tap = Some(now);
                    if s.taps >= 2 {
                        s.taps = 0;
                        s.last_tap = None;
                        with_app(|app| toggle_lens(app));
                    }
                }
            }
        });
    } else if !is_up {
        HOOK_STATE.with(|st| {
            let mut s = st.borrow_mut();
            s.other_key = true;
            s.taps = 0;
            s.last_tap = None;
        });
    }

    if swallow {
        1
    } else {
        wm::CallNextHookEx(null_mut(), code, wparam, lparam)
    }
}

fn toggle_lens(app: &mut App) {
    match app.phase {
        Phase::Closed => {
            let mut pt = POINT { x: 0, y: 0 };
            unsafe { wm::GetCursorPos(&mut pt) };
            app.center = (pt.x as f64, pt.y as f64);
            let now = Instant::now();
            app.phase = Phase::Opening(now);
            app.last_tick = now;
            app.last_d = 0;
            unsafe {
                wm::ShowWindow(app.hwnd, wm::SW_SHOWNOACTIVATE);
                wm::SetTimer(app.hwnd, TIMER_ID, 15, None);
            }
            tick(app);
        }
        Phase::Opening(_) | Phase::Open => {
            app.phase = Phase::Closing(Instant::now());
        }
        Phase::Closing(_) => {
            let now = Instant::now();
            app.phase = Phase::Opening(now);
            app.last_tick = now;
            app.last_d = 0;
            unsafe {
                wm::ShowWindow(app.hwnd, wm::SW_SHOWNOACTIVATE);
                wm::SetTimer(app.hwnd, TIMER_ID, 15, None);
            }
            tick(app);
        }
    }
}

fn close_lens(app: &mut App) {
    if !matches!(app.phase, Phase::Closed) {
        app.phase = Phase::Closing(Instant::now());
    }
}

fn change_zoom(app: &mut App, delta: f64) {
    let c = &mut app.config;
    let nz = (c.zoom + delta).clamp(c.min_zoom, c.max_zoom);
    if (nz - c.zoom).abs() > 1e-9 {
        c.zoom = nz;
        unsafe { apply_zoom(app.hmag, c.zoom) };
        app.last_zoom = c.zoom;
        persist(app, false);
    }
}

fn set_size(app: &mut App, d: i32) {
    app.config.lens_diameter = d;
    app.last_d = 0;
    persist(app, true);
}

fn persist(app: &mut App, force: bool) {
    let now = Instant::now();
    if force || now.duration_since(app.last_save) > Duration::from_millis(750) {
        app.config.save();
        app.last_save = now;
    }
}

fn tick(app: &mut App) {
    let now = Instant::now();
    let dt = (now - app.last_tick).as_secs_f64().clamp(0.001, 0.05);
    app.last_tick = now;

    match app.phase {
        Phase::Closed => return,
        Phase::Opening(start) => {
            if (now - start).as_secs_f64() >= 0.45 {
                app.phase = Phase::Open;
            }
        }
        Phase::Closing(start) => {
            if (now - start).as_secs_f64() >= 0.20 {
                app.phase = Phase::Closed;
                app.last_d = 0;
                unsafe {
                    wm::ShowWindow(app.hwnd, wm::SW_HIDE);
                    wm::KillTimer(app.hwnd, TIMER_ID);
                }
                persist(app, true);
                return;
            }
        }
        Phase::Open => {}
    }

    let mut pt = POINT { x: 0, y: 0 };
    unsafe { wm::GetCursorPos(&mut pt) };
    let tau = app.config.follow_smoothing.max(0.001);
    let k = 1.0 - (-dt / tau).exp();
    app.center.0 += (pt.x as f64 - app.center.0) * k;
    app.center.1 += (pt.y as f64 - app.center.1) * k;

    let base = app.config.lens_diameter as f64;
    let d = match app.phase {
        Phase::Opening(start) => {
            let t = ((now - start).as_secs_f64() / 0.45).min(1.0);
            base * (0.55 + 0.45 * ease_out_back(t))
        }
        Phase::Open => base,
        Phase::Closing(start) => {
            let t = ((now - start).as_secs_f64() / 0.20).min(1.0);
            base * (1.0 - 0.45 * t * t * t)
        }
        Phase::Closed => base,
    };

    let d_i = d.round().max(2.0) as i32;
    let x = (app.center.0 - d / 2.0).round() as i32;
    let y = (app.center.1 - d / 2.0).round() as i32;
    let ring = app.config.ring_width.max(0);
    let inner = (d_i - 2 * ring).max(2);

    unsafe {
        if (app.config.zoom - app.last_zoom).abs() > 1e-9 {
            apply_zoom(app.hmag, app.config.zoom);
            app.last_zoom = app.config.zoom;
        }
        wm::SetWindowPos(
            app.hwnd,
            wm::HWND_TOPMOST,
            x,
            y,
            d_i,
            d_i,
            wm::SWP_NOACTIVATE | wm::SWP_SHOWWINDOW,
        );
        if d_i != app.last_d {
            let hr = gdi::CreateEllipticRgn(0, 0, d_i, d_i);
            gdi::SetWindowRgn(app.hwnd, hr, 1);
            let hr2 = gdi::CreateEllipticRgn(0, 0, inner, inner);
            gdi::SetWindowRgn(app.hmag, hr2, 1);
            wm::SetWindowPos(
                app.hmag,
                null_mut(),
                ring,
                ring,
                inner,
                inner,
                wm::SWP_NOACTIVATE | wm::SWP_NOZORDER,
            );
            app.last_d = d_i;
        }
        let half = inner as f64 / (2.0 * app.config.zoom);
        let src = RECT {
            left: (app.center.0 - half).floor() as i32,
            top: (app.center.1 - half).floor() as i32,
            right: (app.center.0 + half).ceil() as i32,
            bottom: (app.center.1 + half).ceil() as i32,
        };
        mag::MagSetWindowSource(app.hmag, src);
    }
}

unsafe fn apply_zoom(hmag: HWND, zoom: f64) {
    let mut m: mag::MAGTRANSFORM = zeroed();
    m.v[0] = zoom as f32;
    m.v[4] = zoom as f32;
    m.v[8] = 1.0;
    mag::MagSetWindowTransform(hmag, &mut m);
}

struct MenuSnap {
    hwnd: HWND,
    open: bool,
    zoom: f64,
    diameter: i32,
    autostart: bool,
}

fn menu_snapshot(app: &App) -> MenuSnap {
    MenuSnap {
        hwnd: app.hwnd,
        open: matches!(app.phase, Phase::Open | Phase::Opening(_)),
        zoom: app.config.zoom,
        diameter: app.config.lens_diameter,
        autostart: autostart_enabled(),
    }
}

fn append(h: wm::HMENU, flags: u32, id: usize, text: &str) {
    let t = w(text);
    unsafe {
        wm::AppendMenuW(h, flags, id, t.as_ptr());
    }
}

fn show_menu(s: &MenuSnap) -> usize {
    unsafe {
        let h = wm::CreatePopupMenu();
        if h.is_null() {
            return 0;
        }
        append(
            h,
            wm::MF_STRING,
            MENU_TOGGLE,
            if s.open { "Close lens" } else { "Open lens" },
        );
        append(h, wm::MF_SEPARATOR, 0, "");
        append(
            h,
            wm::MF_STRING,
            MENU_ZOOM_IN,
            &format!("Zoom in  ({:.2}x)", s.zoom),
        );
        append(h, wm::MF_STRING, MENU_ZOOM_OUT, "Zoom out");

        let sub = wm::CreatePopupMenu();
        for (id, size) in [
            (MENU_SIZE_300, 300),
            (MENU_SIZE_400, 400),
            (MENU_SIZE_500, 500),
            (MENU_SIZE_600, 600),
        ] {
            let mut flags = wm::MF_STRING;
            if s.diameter == size {
                flags |= wm::MF_CHECKED;
            }
            append(sub, flags, id, &format!("{size} px"));
        }
        let size_label = w("Lens size");
        wm::AppendMenuW(h, wm::MF_POPUP, sub as usize, size_label.as_ptr());

        let mut af = wm::MF_STRING;
        if s.autostart {
            af |= wm::MF_CHECKED;
        }
        append(h, af, MENU_AUTOSTART, "Start with Windows");
        append(h, wm::MF_SEPARATOR, 0, "");
        append(h, wm::MF_STRING, MENU_QUIT, "Quit CircleYe");

        let mut pt = POINT { x: 0, y: 0 };
        wm::GetCursorPos(&mut pt);
        wm::SetForegroundWindow(s.hwnd);
        let cmd = wm::TrackPopupMenu(
            h,
            wm::TPM_RETURNCMD | wm::TPM_RIGHTBUTTON | wm::TPM_NONOTIFY,
            pt.x,
            pt.y,
            0,
            s.hwnd,
            null(),
        );
        wm::PostMessageW(s.hwnd, wm::WM_NULL, 0, 0);
        wm::DestroyMenu(h);
        cmd as usize
    }
}

fn handle_menu(app: &mut App, cmd: usize) {
    match cmd {
        MENU_TOGGLE => toggle_lens(app),
        MENU_ZOOM_IN => change_zoom(app, app.config.zoom_step),
        MENU_ZOOM_OUT => change_zoom(app, -app.config.zoom_step),
        MENU_SIZE_300 => set_size(app, 300),
        MENU_SIZE_400 => set_size(app, 400),
        MENU_SIZE_500 => set_size(app, 500),
        MENU_SIZE_600 => set_size(app, 600),
        MENU_AUTOSTART => set_autostart(!autostart_enabled()),
        MENU_QUIT => quit(app),
        _ => {}
    }
}

fn quit(app: &mut App) {
    app.config.save();
    unsafe {
        if !app.hook.is_null() {
            wm::UnhookWindowsHookEx(app.hook);
        }
        tray_delete(app.hwnd);
        wm::DestroyWindow(app.hwnd);
        if !app.icon.is_null() {
            wm::DestroyIcon(app.icon);
        }
        mag::MagUninitialize();
    }
}

fn copy_utf16(dst: &mut [u16], s: &str) {
    let enc: Vec<u16> = s.encode_utf16().collect();
    let n = enc.len().min(dst.len().saturating_sub(1));
    dst[..n].copy_from_slice(&enc[..n]);
    dst[n] = 0;
}

fn tray_add(hwnd: HWND, icon: wm::HICON) {
    unsafe {
        let mut nid: sh::NOTIFYICONDATAW = zeroed();
        nid.cbSize = size_of::<sh::NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = sh::NIF_MESSAGE | sh::NIF_ICON | sh::NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = icon;
        copy_utf16(&mut nid.szTip, "CircleYe  -  Shift twice to open / close");
        sh::Shell_NotifyIconW(sh::NIM_ADD, &nid);
    }
}

fn tray_delete(hwnd: HWND) {
    unsafe {
        let mut nid: sh::NOTIFYICONDATAW = zeroed();
        nid.cbSize = size_of::<sh::NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        sh::Shell_NotifyIconW(sh::NIM_DELETE, &nid);
    }
}

fn make_ring_icon(size: i32, rgb: u32) -> wm::HICON {
    unsafe {
        let s = size.max(8);
        let mut bmi: gdi::BITMAPV5HEADER = zeroed();
        bmi.bV5Size = size_of::<gdi::BITMAPV5HEADER>() as u32;
        bmi.bV5Width = s;
        bmi.bV5Height = -s;
        bmi.bV5Planes = 1;
        bmi.bV5BitCount = 32;
        bmi.bV5Compression = gdi::BI_BITFIELDS;
        bmi.bV5RedMask = 0x00FF0000;
        bmi.bV5GreenMask = 0x0000FF00;
        bmi.bV5BlueMask = 0x000000FF;
        bmi.bV5AlphaMask = 0xFF000000;

        let mut bits: *mut c_void = null_mut();
        let hdc = gdi::GetDC(null_mut());
        let hbm = gdi::CreateDIBSection(
            hdc,
            &bmi as *const _ as *const gdi::BITMAPINFO,
            gdi::DIB_RGB_COLORS,
            &mut bits,
            null_mut(),
            0,
        );
        gdi::ReleaseDC(null_mut(), hdc);
        if hbm.is_null() || bits.is_null() {
            return null_mut();
        }

        let px = bits as *mut u32;
        let cx = (s as f64 - 1.0) / 2.0;
        let cy = (s as f64 - 1.0) / 2.0;
        let outer = s as f64 / 2.0 - 1.0;
        let inner = outer - (s as f64) * 0.13;
        let r = (rgb >> 16) & 0xFF;
        let g = (rgb >> 8) & 0xFF;
        let b = rgb & 0xFF;

        let mut mask = vec![0u8; ((s * s + 7) / 8) as usize];
        for y in 0..s {
            for x in 0..s {
                let dx = x as f64 - cx;
                let dy = y as f64 - cy;
                let dist = (dx * dx + dy * dy).sqrt();
                let idx = (y * s + x) as usize;
                if dist <= outer && dist >= inner {
                    *px.add(idx) = 0xFF000000 | (r << 16) | (g << 8) | b;
                } else {
                    *px.add(idx) = 0;
                    let bit = (y * s + x) as usize;
                    mask[bit / 8] |= 1 << (7 - (bit % 8));
                }
            }
        }

        let hbm_mask = gdi::CreateBitmap(s, s, 1, 1, mask.as_ptr() as *const c_void);
        let ii = wm::ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm_mask,
            hbmColor: hbm,
        };
        let icon = wm::CreateIconIndirect(&ii);
        let _ = gdi::DeleteObject(hbm as gdi::HGDIOBJ);
        let _ = gdi::DeleteObject(hbm_mask as gdi::HGDIOBJ);
        icon
    }
}
