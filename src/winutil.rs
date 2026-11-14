//! Small Windows helpers: finding our window, focus, and the cursor clip.
//!
//! The cursor clip (`ClipCursor`) is one rectangle for the whole system. Games like Valorant
//! set it when they get focus. If Gridshot cleared it after losing focus it would cancel the
//! game's clip and the mouse could wander out of the game, so Gridshot only clears a clip
//! that is still its own.

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;

    pub type Hwnd = *mut c_void;

    #[repr(C)]
    #[derive(Default, Clone, Copy, PartialEq, Debug)]
    pub struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowExW(parent: Hwnd, after: Hwnd, class: *const u16, title: *const u16) -> Hwnd;
        fn GetWindowThreadProcessId(hwnd: Hwnd, pid: *mut u32) -> u32;
        fn GetForegroundWindow() -> Hwnd;
        fn GetClipCursor(rect: *mut Rect) -> i32;
        fn ClipCursor(rect: *const Rect) -> i32;
        fn GetClientRect(hwnd: Hwnd, rect: *mut Rect) -> i32;
        fn ClientToScreen(hwnd: Hwnd, point: *mut [i32; 2]) -> i32;
    }

    fn pid_of(hwnd: Hwnd) -> u32 {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
        pid
    }

    /// The game window (miniquad's window class) belonging to this process.
    pub fn own_window() -> Hwnd {
        let class: Vec<u16> = "MINIQUADAPP\0".encode_utf16().collect();
        let me = std::process::id();
        let mut w: Hwnd = std::ptr::null_mut();
        loop {
            w = unsafe { FindWindowExW(std::ptr::null_mut(), w, class.as_ptr(), std::ptr::null()) };
            if w.is_null() || pid_of(w) == me {
                return w;
            }
        }
    }

    /// True while a window of this process is the one in front.
    pub fn has_focus() -> bool {
        let w = unsafe { GetForegroundWindow() };
        !w.is_null() && pid_of(w) == std::process::id()
    }

    fn own_client_rect() -> Option<Rect> {
        let w = own_window();
        if w.is_null() {
            return None;
        }
        let mut r = Rect::default();
        unsafe {
            GetClientRect(w, &mut r);
            let mut tl = [r.left, r.top];
            let mut br = [r.right, r.bottom];
            ClientToScreen(w, &mut tl);
            ClientToScreen(w, &mut br);
            Some(Rect { left: tl[0], top: tl[1], right: br[0], bottom: br[1] })
        }
    }

    /// Run `release` (which clears the clip), then put back the clip if it belonged to
    /// another app rather than to Gridshot.
    pub fn release_keeping_other_clip(release: impl FnOnce()) {
        let mut current = Rect::default();
        let had = unsafe { GetClipCursor(&mut current) } != 0;
        release();
        if had && Some(current) != own_client_rect() {
            unsafe { ClipCursor(&current) };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn has_focus() -> bool {
        true
    }
    pub fn release_keeping_other_clip(release: impl FnOnce()) {
        release()
    }
}

pub use imp::*;
