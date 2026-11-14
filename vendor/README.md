# Vendored crates

## miniquad 0.4.11 (patched)

An unchanged copy of [miniquad](https://github.com/not-fl3/miniquad) 0.4.11 (MIT / Apache-2.0,
see its licence files), except for one function, `WindowsDisplay::set_cursor_grab` in
`src/native/windows.rs`, marked `GRIDSHOT PATCH`.

**Why:** macroquad calls `set_cursor_grab` every frame. On Windows, the original code ran
`ClipCursor(NULL)` every frame while the cursor wasn't grabbed. The cursor clip is shared by
the whole system, so with Gridshot open in the background it kept cancelling the clip that a
game in front (for example Valorant) had set, and the mouse escaped from the game after a
few clicks.

**The patch:** only release a clip that this window set, and only clip while this window is
the foreground window.
