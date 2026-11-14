//! A system-wide hotkey (default Ctrl+Shift+G) that brings Gridshot to the front from any
//! app or game, and sends it back again. It uses the Windows `RegisterHotKey` API, so Windows
//! tells Gridshot when the keys are pressed; nothing is injected into other programs.
//!
//! Pressed while another app is in front: Gridshot is restored and brought forward, and that
//! app is remembered. Pressed while Gridshot is in front: the main loop first lets go of the
//! mouse (pausing any round), then the remembered app comes back to the front and Gridshot is
//! minimised. Letting go first matters: the cursor clip is system-wide, and releasing it after
//! the game is back would undo the game's own clip.

pub const DEFAULT: &str = "ctrl+shift+g";

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Off,
    /// Registered; the label is how it is shown, e.g. "Ctrl+Shift+G".
    Ready(String),
    /// Bad setting, or another app already owns that key combination.
    Failed(String),
}

const MOD_ALT: u32 = 0x1;
const MOD_CONTROL: u32 = 0x2;
const MOD_SHIFT: u32 = 0x4;
const MOD_WIN: u32 = 0x8;

/// Parse `ctrl+shift+g` style text into (modifiers, virtual key code, display label).
pub fn parse(spec: &str) -> Option<(u32, u32, String)> {
    let mut mods = 0;
    let mut key: Option<(u32, String)> = None;
    let mut label = Vec::new();
    for part in spec.split('+').map(|p| p.trim().to_ascii_lowercase()) {
        match part.as_str() {
            "ctrl" | "control" => {
                mods |= MOD_CONTROL;
                label.push("Ctrl".to_owned());
            }
            "alt" => {
                mods |= MOD_ALT;
                label.push("Alt".to_owned());
            }
            "shift" => {
                mods |= MOD_SHIFT;
                label.push("Shift".to_owned());
            }
            "win" => {
                mods |= MOD_WIN;
                label.push("Win".to_owned());
            }
            k => {
                if key.is_some() {
                    return None; // two normal keys
                }
                key = Some(key_code(k)?);
            }
        }
    }
    let (vk, name) = key?;
    label.push(name);
    Some((mods, vk, label.join("+")))
}

fn key_code(k: &str) -> Option<(u32, String)> {
    let b = k.as_bytes();
    if b.len() == 1 && (b[0].is_ascii_alphanumeric()) {
        let c = b[0].to_ascii_uppercase();
        return Some((c as u32, (c as char).to_string()));
    }
    if let Some(n) = k.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Some((0x70 + n - 1, format!("F{n}")));
        }
    }
    let (vk, name) = match k {
        "`" | "backtick" | "tilde" => (0xC0, "`"),
        "space" => (0x20, "Space"),
        "insert" => (0x2D, "Insert"),
        "delete" => (0x2E, "Delete"),
        "home" => (0x24, "Home"),
        "end" => (0x23, "End"),
        "pageup" => (0x21, "PageUp"),
        "pagedown" => (0x22, "PageDown"),
        "pause" => (0x13, "Pause"),
        "scrolllock" => (0x91, "ScrollLock"),
        _ => return None,
    };
    Some((vk, name.to_owned()))
}

/// Register the hotkey on its own thread and start listening.
pub fn start(spec: &str) -> Status {
    let spec = spec.trim();
    if spec.eq_ignore_ascii_case("off") || spec.is_empty() {
        return Status::Off;
    }
    let Some((mods, vk, label)) = parse(spec) else {
        return Status::Failed(format!("hotkey \"{spec}\" in settings.txt is not valid"));
    };
    imp::start(mods, vk, label)
}

#[cfg(windows)]
mod imp {
    use super::Status;
    use std::ffi::c_void;
    use crate::winutil::own_window;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::mpsc;

    type Hwnd = *mut c_void;

    #[repr(C)]
    struct Msg {
        hwnd: Hwnd,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: [i32; 2],
        private: u32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn RegisterHotKey(hwnd: Hwnd, id: i32, mods: u32, vk: u32) -> i32;
        fn GetMessageW(msg: *mut Msg, hwnd: Hwnd, min: u32, max: u32) -> i32;
        fn GetForegroundWindow() -> Hwnd;
        fn SetForegroundWindow(hwnd: Hwnd) -> i32;
        fn BringWindowToTop(hwnd: Hwnd) -> i32;
        fn ShowWindow(hwnd: Hwnd, cmd: i32) -> i32;
        fn IsIconic(hwnd: Hwnd) -> i32;
        fn IsWindow(hwnd: Hwnd) -> i32;
    }

    const MOD_NOREPEAT: u32 = 0x4000;
    const WM_HOTKEY: u32 = 0x0312;
    const SW_MINIMIZE: i32 = 6;
    const SW_SHOWMINNOACTIVE: i32 = 7;
    const SW_RESTORE: i32 = 9;

    /// The app that was in front when Gridshot was called up.
    static PREVIOUS: AtomicIsize = AtomicIsize::new(0);
    /// Set by the hotkey thread, handled by the main loop (which owns the mouse grab).
    static HIDE_REQUESTED: AtomicBool = AtomicBool::new(false);

    pub fn take_hide_request() -> bool {
        HIDE_REQUESTED.swap(false, Ordering::Relaxed)
    }

    /// Give the front back to the app Gridshot was called from and minimise.
    /// Call from the main loop after releasing the mouse grab.
    pub fn hide_now() {
        let me = own_window();
        if me.is_null() {
            return;
        }
        let prev = PREVIOUS.swap(0, Ordering::Relaxed) as Hwnd;
        unsafe {
            // Gridshot is in front, so it is allowed to hand the foreground to another window.
            if !prev.is_null() && prev != me && IsWindow(prev) != 0 && SetForegroundWindow(prev) != 0 {
                ShowWindow(me, SW_SHOWMINNOACTIVE);
            } else {
                ShowWindow(me, SW_MINIMIZE);
            }
        }
    }

    pub fn start(mods: u32, vk: u32, label: String) -> Status {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || unsafe {
            // A null window posts WM_HOTKEY to this thread's own queue.
            let ok = RegisterHotKey(std::ptr::null_mut(), 1, mods | MOD_NOREPEAT, vk) != 0;
            let _ = tx.send(ok);
            if !ok {
                return;
            }
            let mut msg: Msg = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY {
                    toggle();
                }
            }
        });
        match rx.recv() {
            Ok(true) => Status::Ready(label),
            _ => Status::Failed(format!("{label} is already used by another app")),
        }
    }

    unsafe fn toggle() {
        let me = own_window();
        if me.is_null() {
            return;
        }
        let front = GetForegroundWindow();
        if front == me && IsIconic(me) == 0 {
            HIDE_REQUESTED.store(true, Ordering::Relaxed);
        } else {
            if front != me {
                PREVIOUS.store(front as isize, Ordering::Relaxed);
            }
            if IsIconic(me) != 0 {
                ShowWindow(me, SW_RESTORE);
            }
            // Receiving the hotkey gives this process the right to take the foreground.
            SetForegroundWindow(me);
            BringWindowToTop(me);
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Status;
    pub fn start(_mods: u32, _vk: u32, _label: String) -> Status {
        Status::Off
    }
    pub fn take_hide_request() -> bool {
        false
    }
    pub fn hide_now() {}
}

pub use imp::{hide_now, take_hide_request};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_hotkeys() {
        assert_eq!(parse("ctrl+shift+g"), Some((MOD_CONTROL | MOD_SHIFT, 'G' as u32, "Ctrl+Shift+G".into())));
        assert_eq!(parse("Alt + F9"), Some((MOD_ALT, 0x78, "Alt+F9".into())));
        assert_eq!(parse("f8"), Some((0, 0x77, "F8".into())));
        assert_eq!(parse("ctrl+`"), Some((MOD_CONTROL, 0xC0, "Ctrl+`".into())));
        assert_eq!(parse("win+alt+5"), Some((MOD_WIN | MOD_ALT, '5' as u32, "Win+Alt+5".into())));
    }

    #[test]
    fn rejects_bad_hotkeys() {
        assert_eq!(parse("ctrl+shift"), None, "no normal key");
        assert_eq!(parse("ctrl+g+h"), None, "two normal keys");
        assert_eq!(parse("ctrl+banana"), None);
        assert_eq!(parse("f25"), None);
    }

    #[test]
    fn off_and_invalid_settings() {
        assert_eq!(start("off"), Status::Off);
        assert!(matches!(start("ctrl+nope"), Status::Failed(_)));
    }
}
