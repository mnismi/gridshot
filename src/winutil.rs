//! Small Windows helpers: finding our window, focus, and the cursor clip.
//!
//! The cursor clip (`ClipCursor`) is one rectangle for the whole system. Games like Valorant
//! set it when they get focus. If Gridshot cleared it after losing focus it would cancel the
//! game's clip and the mouse could wander out of the game. miniquad also applies a grab change
//! one frame late, so Gridshot waits for its own release to happen before handing focus back,
//! and puts back a game's clip if its late release landed on top of it.

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
        fn GetSystemMetrics(index: i32) -> i32;
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

    fn current_clip() -> Rect {
        let mut r = Rect::default();
        unsafe { GetClipCursor(&mut r) };
        r
    }

    /// What the clip covers when nobody has set one: the whole virtual desktop.
    fn no_clip() -> Rect {
        const SM_XVIRTUALSCREEN: i32 = 76;
        const SM_YVIRTUALSCREEN: i32 = 77;
        const SM_CXVIRTUALSCREEN: i32 = 78;
        const SM_CYVIRTUALSCREEN: i32 = 79;
        unsafe {
            let (x, y) = (GetSystemMetrics(SM_XVIRTUALSCREEN), GetSystemMetrics(SM_YVIRTUALSCREEN));
            Rect {
                left: x,
                top: y,
                right: x + GetSystemMetrics(SM_CXVIRTUALSCREEN),
                bottom: y + GetSystemMetrics(SM_CYVIRTUALSCREEN),
            }
        }
    }

    /// A clip that some other app (e.g. a game that just took focus) has set, if any.
    pub fn other_app_clip() -> Option<Rect> {
        let c = current_clip();
        (c != no_clip() && Some(c) != own_client_rect()).then_some(c)
    }

    /// Put back another app's clip if ours was released on top of it. Only while that
    /// app is still in front and nobody has set a different clip since.
    pub fn restore_clip_if_cleared(r: Rect) {
        if !has_focus() && current_clip() == no_clip() {
            unsafe { ClipCursor(&r) };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn has_focus() -> bool {
        true
    }
    #[derive(Clone, Copy)]
    pub struct Rect;
    pub fn other_app_clip() -> Option<Rect> {
        None
    }
    pub fn restore_clip_if_cleared(_r: Rect) {}
}

pub use imp::*;
