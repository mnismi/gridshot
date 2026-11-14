#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Gridshot: a 3x3 grid of target spots, three targets live at once.
//! Shoot one and a new one appears in a random free spot.
//! Mouse look uses raw mouse counts with Valorant's yaw (0.07 degrees per count).

mod demo;
mod game;
mod scores;
mod settings;
mod sound;
mod usage;

use game::{Aim, Grid, Shot};
use macroquad::audio::{play_sound, PlaySoundParams, Sound};
use macroquad::miniquad::{Comparison, PipelineParams};
use macroquad::prelude::*;
use scores::{Score, Scores};
use settings::Settings;

/// Valorant's 103 degree horizontal FOV at 16:9, as a vertical FOV (about 70.53 degrees).
const VFOV_DEG: f32 = 70.532_6;

const MISS_RED: Color = Color::new(1.0, 0.36, 0.4, 1.0);
const ORANGE: Color = Color::new(1.0, 0.54, 0.24, 1.0);
const DIM: Color = Color::new(0.63, 0.66, 0.72, 1.0);
const TARGET_COLOR: [u8; 4] = [255, 138, 61, 255];
const ROOM_X: f32 = 16.0;
const FLOOR_Y: f32 = -5.0;
const CEIL_Y: f32 = 9.0;
const BACK_Z: f32 = -14.0;
const FRONT_Z: f32 = 6.0;

enum Action {
    Start,
    Menu,
}

#[derive(PartialEq)]
enum State {
    Menu,
    Playing,
    /// Round on hold because the game lost focus; `at` is when it stopped.
    Paused { at: f64 },
    /// `rank` is this round's place in the high scores, if it made the list.
    Results { rank: Option<usize> },
}

fn window_conf() -> Conf {
    let s = Settings::load();
    let mut conf = Conf {
        window_title: "Gridshot".to_owned(),
        window_width: 1280,
        window_height: 720,
        high_dpi: true,
        fullscreen: s.fullscreen,
        sample_count: 4,
        window_resizable: true,
        ..Default::default()
    };
    conf.platform.swap_interval = Some(if s.vsync { 1 } else { 0 });
    conf
}

/// Collects mouse events in the order they happened inside a frame,
/// so a click is tested against the aim at the moment of the click.
#[derive(Default)]
struct Events {
    list: Vec<Ev>,
}

enum Ev {
    Move(f32, f32),
    Click,
}

impl macroquad::miniquad::EventHandler for Events {
    fn update(&mut self) {}
    fn draw(&mut self) {}
    fn mouse_motion_event(&mut self, x: f32, y: f32) {
        self.list.push(Ev::Move(x, y));
    }
    fn mouse_button_down_event(&mut self, btn: MouseButton, _x: f32, _y: f32) {
        if btn == MouseButton::Left {
            self.list.push(Ev::Click);
        }
    }
}

/// A hit ring on a target, or a red cross where a missed shot landed.
struct HitFx {
    pos: Vec3,
    born: f64,
    miss: bool,
}

const HIT_FX_SECS: f64 = 0.35;
const MISS_FX_SECS: f64 = 1.2;

#[macroquad::main(window_conf)]
async fn main() {
    let mut settings = Settings::load();
    let mut scores = Scores::load();
    let mut fullscreen = settings.fullscreen;
    let sphere_mat = load_material(
        ShaderSource::Glsl { vertex: SPHERE_VS, fragment: SPHERE_FS },
        MaterialParams {
            pipeline_params: PipelineParams {
                depth_test: Comparison::LessOrEqual,
                depth_write: true,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .expect("sphere shader");
    let unit_sphere = sphere_mesh(18, 28);
    let hit_sound: Option<Sound> = macroquad::audio::load_sound_from_bytes(&sound::hit_wav()).await.ok();

    let sub = macroquad::input::utils::register_input_subscriber();
    let mut events = Events::default();
    let mut last_raw: Option<Vec2> = None;

    let mut state = State::Menu;
    let mut grid = Grid::new(seed());
    let mut aim = Aim::centre();
    let mut sens_text = settings.sens_text();
    let mut round_start = 0.0_f64;
    let mut hits = 0u32;
    let mut misses = 0u32;
    let mut fx: Vec<HitFx> = Vec::new();
    // Menu buttons only work once the mouse has moved on screen (so the cursor spot is
    // real) and after a short wait when a round ends, so leftover shooting clicks do nothing.
    let mut pointer_live = false;
    let mut buttons_from = 0.0_f64;
    let mut fps = FpsMeter::default();
    let mut demo = demo::Demo::from_env();

    loop {
        let now = get_time();
        events.list.clear();
        macroquad::input::utils::repeat_all_miniquad_input(&mut events, sub);

        if let Some(d) = demo.as_mut() {
            let world = demo::World {
                state: &mut state,
                scores: &mut scores,
                hits: &mut hits,
                misses: &mut misses,
                fx: &mut fx,
                aim: &mut aim,
                round_start: &mut round_start,
                pointer_live: &mut pointer_live,
                grid: &mut grid,
                sound: hit_sound.as_ref(),
            };
            if !d.stage(now, world) {
                break;
            }
        } else {
            if is_key_pressed(KeyCode::F11)
                || (is_key_pressed(KeyCode::Enter) && (is_key_down(KeyCode::LeftAlt) || is_key_down(KeyCode::RightAlt)))
            {
                fullscreen = !fullscreen;
                set_fullscreen(fullscreen);
                settings.fullscreen = fullscreen;
                settings.save();
                if state == State::Playing {
                    set_cursor_grab(true);
                }
            }

            match state {
                State::Menu | State::Results { .. } => {
                    if matches!(state, State::Menu) {
                        edit_sens(&mut sens_text, &mut settings);
                    }
                    let esc = is_key_pressed(KeyCode::Escape);
                    if esc && matches!(state, State::Menu) {
                        commit_sens(&mut sens_text, &mut settings);
                        break;
                    }
                    if esc {
                        state = State::Menu;
                    }
                    if events.list.iter().any(|e| matches!(e, Ev::Move(..))) {
                        pointer_live = true;
                    }
                }
                State::Paused { at } => {
                    if is_key_pressed(KeyCode::Escape) {
                        pointer_live = false;
                        buttons_from = now + 0.3;
                        state = State::Menu;
                    } else if has_focus() && (is_mouse_button_pressed(MouseButton::Left) || is_key_pressed(KeyCode::Space)) {
                        round_start += now - at; // the paused time does not count
                        grab(true, &mut last_raw);
                        state = State::Playing;
                    }
                }
                State::Playing if !has_focus() => {
                    // Windows key, alt-tab, a popup...: give the mouse back and hold the round.
                    grab(false, &mut last_raw);
                    state = State::Paused { at: now };
                }
                State::Playing => {
                    for ev in events.list.drain(..) {
                        match ev {
                            Ev::Move(x, y) => {
                                let p = vec2(x, y);
                                if let Some(last) = last_raw {
                                    let d = p - last;
                                    aim.turn(d.x, d.y, settings.sensitivity);
                                }
                                last_raw = Some(p);
                            }
                            Ev::Click => {
                                // Re-clip in case Windows dropped the clip (alt-tab etc).
                                set_cursor_grab(true);
                                match grid.shoot(Vec3::ZERO, aim.dir()) {
                                    Shot::Hit(pos) => {
                                        hits += 1;
                                        fx.push(HitFx { pos, born: now, miss: false });
                                        if let Some(s) = &hit_sound {
                                            play_sound(s, PlaySoundParams { looped: false, volume: settings.volume });
                                        }
                                    }
                                    Shot::Miss => {
                                        misses += 1;
                                        fx.push(HitFx { pos: wall_point(aim.dir()), born: now, miss: true });
                                    }
                                }
                            }
                        }
                    }
                    if is_key_pressed(KeyCode::Escape) {
                        grab(false, &mut last_raw);
                        pointer_live = false;
                        buttons_from = now + 0.3;
                        state = State::Menu;
                    } else if now - round_start >= settings.duration as f64 {
                        grab(false, &mut last_raw);
                        pointer_live = false;
                        buttons_from = now + 1.0;
                        let rank = scores.add(Score {
                            hits,
                            misses,
                            sens: settings.sensitivity,
                            duration: settings.duration,
                            date: scores::today(),
                        });
                        scores.save();
                        state = State::Results { rank };
                    }
                }
            }
        }
        fx.retain(|f| now - f.born < if f.miss { MISS_FX_SECS } else { HIT_FX_SECS });

        // ---- 3D scene ----
        clear_background(Color::from_rgba(12, 14, 20, 255));
        let cam = Camera3D {
            position: Vec3::ZERO,
            target: aim.dir(),
            up: Vec3::Y,
            fovy: VFOV_DEG.to_radians(),
            ..Default::default()
        };
        set_camera(&cam);
        draw_room();
        {
            gl_use_material(&sphere_mat);
            for c in grid.live_centers() {
                draw_mesh(&placed(&unit_sphere, c, game::RADIUS, TARGET_COLOR));
            }
            gl_use_default_material();
        }

        // ---- 2D overlay ----
        set_default_camera();
        let (w, h) = (screen_width(), screen_height());
        let view = cam.matrix();
        for f in &fx {
            let Some(p) = to_screen(view, f.pos, w, h) else { continue };
            if f.miss {
                let a = (1.0 - (now - f.born) / MISS_FX_SECS).clamp(0.0, 1.0) as f32;
                let (r, red) = (7.0, Color::new(1.0, 0.25, 0.3, a));
                draw_line(p.x - r, p.y - r, p.x + r, p.y + r, 2.5, red);
                draw_line(p.x - r, p.y + r, p.x + r, p.y - r, 2.5, red);
            } else {
                let t = ((now - f.born) / HIT_FX_SECS) as f32;
                let a = (1.0 - t).clamp(0.0, 1.0);
                draw_circle_lines(p.x, p.y, 18.0 + 40.0 * t, 2.5, Color::new(1.0, 0.75, 0.5, a));
            }
        }

        fps.tick(now);
        fps.draw(w, h);
        let live = pointer_live && now >= buttons_from;
        let action = match state {
            State::Playing => {
                draw_crosshair(w, h);
                draw_hud(w, settings.duration as f64 - (now - round_start), hits, misses);
                None
            }
            State::Paused { at } => {
                draw_hud(w, settings.duration as f64 - (at - round_start), hits, misses);
                draw_paused(w, h);
                None
            }
            State::Menu => draw_menu(w, h, &sens_text, &settings, &scores, live),
            State::Results { rank } => draw_results(w, h, hits, misses, &settings, &scores, rank, live),
        };
        let action = if demo.is_some() { None } else { action };
        match action {
            Some(Action::Start) => {
                commit_sens(&mut sens_text, &mut settings);
                aim = Aim::centre();
                grid = Grid::new(seed());
                hits = 0;
                misses = 0;
                fx.clear();
                round_start = now;
                grab(true, &mut last_raw);
                state = State::Playing;
            }
            Some(Action::Menu) => state = State::Menu,
            None => {}
        }

        if let Some(d) = demo.as_mut() {
            d.after_draw(now);
        }
        next_frame().await;
    }
}

/// Frames per second, averaged over half a second so the number is readable.
#[derive(Default)]
struct FpsMeter {
    window_start: f64,
    frames: u32,
    fps: f64,
    usage: usage::Usage,
}

impl FpsMeter {
    fn tick(&mut self, now: f64) {
        self.frames += 1;
        let span = now - self.window_start;
        if span >= 0.5 {
            self.fps = self.frames as f64 / span;
            self.usage.sample(now);
            self.frames = 0;
            self.window_start = now;
        }
    }

    fn draw(&self, w: f32, h: f32) {
        if self.fps <= 0.0 {
            return;
        }
        let mut t = format!("{:.0} FPS  {:.1} ms", self.fps, 1000.0 / self.fps);
        if self.usage.available() {
            t += &format!("   CPU {:.1}%   RAM {:.0} MB", self.usage.cpu_pct, self.usage.ram_mb);
        }
        let m = measure_text(&t, None, 22, 1.0);
        draw_text(&t, w - m.width - 16.0, h - 14.0, 22.0, DIM);
    }
}

fn draw_hud(w: f32, left: f64, hits: u32, misses: u32) {
    let shots = hits + misses;
    let acc = if shots == 0 { 100.0 } else { hits as f32 * 100.0 / shots as f32 };
    centered(&format!("{:.0}", left.max(0.0).ceil()), w / 2.0, 54.0, 44.0, WHITE);
    draw_text(&format!("Hits {hits}"), 28.0, 50.0, 32.0, WHITE);
    draw_text(&format!("Misses {misses}"), 28.0, 86.0, 32.0, MISS_RED);
    let acc_s = format!("Acc {acc:.0}%");
    let m = measure_text(&acc_s, None, 32, 1.0);
    draw_text(&acc_s, w - m.width - 28.0, 50.0, 32.0, WHITE);
}

fn draw_paused(w: f32, h: f32) {
    let top = panel(w, h, 190.0);
    let cx = w / 2.0;
    centered("PAUSED", cx, top + 70.0, 48.0, WHITE);
    centered("Click to carry on", cx, top + 125.0, 30.0, Color::from_rgba(255, 138, 61, 255));
    centered("Esc menu", cx, top + 160.0, 20.0, Color::from_rgba(160, 168, 185, 255));
}

/// True while the game window is the one in front. When you press the Windows key or
/// alt-tab, another process (Start menu, Explorer...) owns the foreground window.
#[cfg(windows)]
fn has_focus() -> bool {
    use std::ffi::c_void;
    #[link(name = "user32")]
    extern "system" {
        fn GetForegroundWindow() -> *mut c_void;
        fn GetWindowThreadProcessId(hwnd: *mut c_void, pid: *mut u32) -> u32;
    }
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return false;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        pid == std::process::id()
    }
}

#[cfg(not(windows))]
fn has_focus() -> bool {
    true
}

fn grab(on: bool, last_raw: &mut Option<Vec2>) {
    set_cursor_grab(on);
    show_mouse(!on);
    // Raw motion is added to the last known cursor spot (in device pixels).
    let (x, y) = mouse_position();
    let s = macroquad::miniquad::window::dpi_scale();
    *last_raw = Some(vec2(x * s, y * s));
}

fn seed() -> u64 {
    (macroquad::miniquad::date::now() * 1_000_000.0) as u64 ^ 0x9E37_79B9_7F4A_7C15
}

fn edit_sens(text: &mut String, s: &mut Settings) {
    while let Some(c) = get_char_pressed() {
        if (c.is_ascii_digit() || (c == '.' && !text.contains('.'))) && text.len() < 6 {
            text.push(c);
        }
    }
    if is_key_pressed(KeyCode::Backspace) {
        text.pop();
    }
    let step = if is_key_down(KeyCode::LeftShift) { 0.1 } else { 0.01 };
    if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Down) {
        let cur = text.parse::<f32>().unwrap_or(s.sensitivity);
        let v = if is_key_pressed(KeyCode::Up) { cur + step } else { cur - step };
        s.sensitivity = settings::clamp_sens(v);
        *text = s.sens_text();
        s.save();
    }
    if is_key_pressed(KeyCode::Enter) {
        commit_sens(text, s);
    }
}

fn commit_sens(text: &mut String, s: &mut Settings) {
    if let Ok(v) = text.parse::<f32>() {
        s.sensitivity = settings::clamp_sens(v);
    }
    *text = s.sens_text();
    s.save();
}

fn centered(t: &str, x: f32, y: f32, size: f32, c: Color) {
    let m = measure_text(t, None, size as u16, 1.0);
    draw_text(t, x - m.width / 2.0, y, size, c);
}

fn panel(w: f32, h: f32, ph: f32) -> f32 {
    draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
    let pw = 620.0_f32.min(w - 40.0);
    let (px, py) = ((w - pw) / 2.0, (h - ph) / 2.0);
    draw_rectangle(px, py, pw, ph, Color::from_rgba(22, 26, 36, 240));
    draw_rectangle_lines(px, py, pw, ph, 2.0, Color::from_rgba(255, 138, 61, 200));
    py
}

fn draw_menu(w: f32, h: f32, sens_text: &str, s: &Settings, scores: &Scores, live: bool) -> Option<Action> {
    let top = panel(w, h, 580.0);
    let cx = w / 2.0;
    centered("GRIDSHOT", cx, top + 70.0, 64.0, ORANGE);
    centered("Valorant sensitivity", cx, top + 120.0, 26.0, DIM);
    let caret = if (get_time() * 2.0) as i64 % 2 == 0 { "_" } else { " " };
    centered(&format!("{sens_text}{caret}"), cx, top + 165.0, 48.0, WHITE);
    centered("type a number, or Up / Down to change it", cx, top + 195.0, 20.0, DIM);
    centered(&format!("High scores, {} s rounds", s.duration), cx, top + 245.0, 26.0, WHITE);
    draw_table(cx, top + 275.0, &scores.top(s.duration), 5, None);
    let start = button(cx, top + 470.0, "Start", live);
    centered("F11 fullscreen     Esc quit", cx, top + 560.0, 20.0, DIM);
    start.then_some(Action::Start)
}

#[allow(clippy::too_many_arguments)]
fn draw_results(
    w: f32,
    h: f32,
    hits: u32,
    misses: u32,
    s: &Settings,
    scores: &Scores,
    rank: Option<usize>,
    live: bool,
) -> Option<Action> {
    let top = panel(w, h, 610.0);
    let cx = w / 2.0;
    let shots = hits + misses;
    let acc = if shots == 0 { 0.0 } else { hits as f32 * 100.0 / shots as f32 };
    centered("TIME", cx, top + 50.0, 36.0, DIM);
    centered(&format!("{hits} hits"), cx, top + 110.0, 60.0, WHITE);
    let line = format!("Misses {misses}    Accuracy {acc:.1}%    {:.2} hits a second", hits as f32 / s.duration as f32);
    centered(&line, cx, top + 150.0, 24.0, WHITE);
    let place = match rank {
        Some(1) => "New high score!".to_owned(),
        Some(n) => format!("Number {n} on your high scores"),
        None => "Not in your top 10 this time".to_owned(),
    };
    centered(&place, cx, top + 188.0, 28.0, if rank.is_some() { ORANGE } else { DIM });
    draw_table(cx, top + 215.0, &scores.top(s.duration), scores::KEEP, rank);
    if button(cx - 120.0, top + 540.0, "Play again", live) {
        Some(Action::Start)
    } else if button(cx + 120.0, top + 540.0, "Menu", live) {
        Some(Action::Menu)
    } else {
        None
    }
}

/// A clickable button centred on `cx` with its top at `y`. Returns true when clicked.
/// While not `live` it is drawn greyed out and ignores the mouse.
fn button(cx: f32, y: f32, label: &str, live: bool) -> bool {
    let r = Rect::new(cx - 110.0, y, 220.0, 50.0);
    let (mx, my) = mouse_position();
    let hover = live && r.contains(vec2(mx, my));
    let fill = if hover { ORANGE } else { Color::from_rgba(32, 37, 50, 255) };
    draw_rectangle(r.x, r.y, r.w, r.h, fill);
    draw_rectangle_lines(r.x, r.y, r.w, r.h, 2.0, if live { ORANGE } else { DIM });
    let text = if hover { Color::from_rgba(20, 22, 30, 255) } else if live { WHITE } else { DIM };
    centered(label, cx, y + 34.0, 30.0, text);
    hover && is_mouse_button_pressed(MouseButton::Left)
}

/// High score rows. `mark` (1-based) is highlighted as the round just played.
fn draw_table(cx: f32, y: f32, rows: &[&Score], max: usize, mark: Option<usize>) {
    const COLS: [(&str, f32); 6] = [("#", -250.0), ("Hits", -195.0), ("Misses", -110.0), ("Acc", -10.0), ("Sens", 75.0), ("Date", 160.0)];
    let size = 22.0;
    let row_h = 29.0;
    for (name, x) in COLS {
        draw_text(name, cx + x, y + 20.0, size, DIM);
    }
    if rows.is_empty() {
        centered("No rounds yet", cx, y + 20.0 + row_h * 1.5, size, DIM);
        return;
    }
    for (i, r) in rows.iter().take(max).enumerate() {
        let ry = y + 20.0 + row_h * (i + 1) as f32;
        let here = mark == Some(i + 1);
        if here {
            draw_rectangle(cx - 262.0, ry - 21.0, 524.0, row_h - 2.0, Color::new(1.0, 0.54, 0.24, 0.22));
        }
        let c = if here { ORANGE } else { WHITE };
        let sens = Settings { sensitivity: r.sens, ..Default::default() }.sens_text();
        let cells = [
            format!("{}", i + 1),
            r.hits.to_string(),
            r.misses.to_string(),
            format!("{:.0}%", r.accuracy()),
            sens,
            r.short_date(),
        ];
        for ((_, x), text) in COLS.iter().zip(cells) {
            draw_text(&text, cx + x, ry, size, c);
        }
    }
}

/// Where a shot along `d` from the player meets the room's walls, floor or ceiling.
fn wall_point(d: Vec3) -> Vec3 {
    let mut t = f32::MAX;
    for (v, lo, hi) in [(d.x, -ROOM_X, ROOM_X), (d.y, FLOOR_Y, CEIL_Y), (d.z, BACK_Z, FRONT_Z)] {
        if v > 1e-6 {
            t = t.min(hi / v);
        } else if v < -1e-6 {
            t = t.min(lo / v);
        }
    }
    d * t * 0.995 // a hair in front of the wall
}

fn draw_crosshair(w: f32, h: f32) {
    let (cx, cy) = ((w / 2.0).floor() + 0.5, (h / 2.0).floor() + 0.5);
    let (gap, len, t) = (4.0, 7.0, 2.0);
    let outline = Color::new(0.0, 0.0, 0.0, 0.8);
    for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let (x0, y0) = (cx + dx * gap, cy + dy * gap);
        let (x1, y1) = (cx + dx * (gap + len), cy + dy * (gap + len));
        draw_line(x0 - dx, y0 - dy, x1 + dx, y1 + dy, t + 2.0, outline);
        draw_line(x0, y0, x1, y1, t, WHITE);
    }
    draw_rectangle(cx - 1.5, cy - 1.5, 3.0, 3.0, outline);
    draw_rectangle(cx - 1.0, cy - 1.0, 2.0, 2.0, WHITE);
}

fn to_screen(m: Mat4, p: Vec3, w: f32, h: f32) -> Option<Vec2> {
    let c = m * p.extend(1.0);
    if c.w <= 0.0 {
        return None;
    }
    let n = c.truncate() / c.w;
    Some(vec2((n.x + 1.0) / 2.0 * w, (1.0 - n.y) / 2.0 * h))
}

// ---------- room ----------

fn quad(a: Vec3, b: Vec3, c: Vec3, d: Vec3, col: [u8; 4]) {
    let v = |p: Vec3| Vertex { position: p, uv: Vec2::ZERO, color: col, normal: Vec4::ZERO };
    draw_mesh(&Mesh { vertices: vec![v(a), v(b), v(c), v(d)], indices: vec![0, 1, 2, 0, 2, 3], texture: None });
}

fn draw_room() {
    let (x0, x1, y0, y1, z0, z1) = (-ROOM_X, ROOM_X, FLOOR_Y, CEIL_Y, BACK_Z, FRONT_Z);
    quad(vec3(x0, y0, z0), vec3(x1, y0, z0), vec3(x1, y1, z0), vec3(x0, y1, z0), [30, 35, 48, 255]); // back
    quad(vec3(x0, y0, z1), vec3(x1, y0, z1), vec3(x1, y1, z1), vec3(x0, y1, z1), [24, 28, 38, 255]); // front
    quad(vec3(x0, y0, z0), vec3(x1, y0, z0), vec3(x1, y0, z1), vec3(x0, y0, z1), [20, 23, 31, 255]); // floor
    quad(vec3(x0, y1, z0), vec3(x1, y1, z0), vec3(x1, y1, z1), vec3(x0, y1, z1), [26, 30, 41, 255]); // ceiling
    quad(vec3(x0, y0, z0), vec3(x0, y1, z0), vec3(x0, y1, z1), vec3(x0, y0, z1), [25, 29, 40, 255]); // left
    quad(vec3(x1, y0, z0), vec3(x1, y1, z0), vec3(x1, y1, z1), vec3(x1, y0, z1), [25, 29, 40, 255]); // right

    let line = Color::from_rgba(44, 51, 68, 255);
    let e = 0.02;
    let mut x = x0;
    while x <= x1 + 0.01 {
        draw_line_3d(vec3(x, y0, z0 + e), vec3(x, y1, z0 + e), line);
        draw_line_3d(vec3(x, y0 + e, z0), vec3(x, y0 + e, z1), line);
        x += 2.0;
    }
    let mut y = y0;
    while y <= y1 + 0.01 {
        draw_line_3d(vec3(x0, y, z0 + e), vec3(x1, y, z0 + e), line);
        y += 2.0;
    }
    let mut z = z0;
    while z <= z1 + 0.01 {
        draw_line_3d(vec3(x0, y0 + e, z), vec3(x1, y0 + e, z), line);
        z += 2.0;
    }
}

// ---------- shaded spheres ----------

struct UnitSphere {
    points: Vec<Vec3>,
    indices: Vec<u16>,
}

fn sphere_mesh(rings: u16, slices: u16) -> UnitSphere {
    use std::f32::consts::PI;
    let mut points = Vec::new();
    for r in 0..=rings {
        let phi = PI * r as f32 / rings as f32;
        for s in 0..=slices {
            let th = 2.0 * PI * s as f32 / slices as f32;
            points.push(vec3(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin()));
        }
    }
    let mut indices = Vec::new();
    let row = slices + 1;
    for r in 0..rings {
        for s in 0..slices {
            let (a, b) = (r * row + s, (r + 1) * row + s);
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    UnitSphere { points, indices }
}

fn placed(u: &UnitSphere, center: Vec3, radius: f32, col: [u8; 4]) -> Mesh {
    let vertices = u
        .points
        .iter()
        .map(|&n| Vertex { position: center + n * radius, uv: Vec2::ZERO, color: col, normal: n.extend(0.0) })
        .collect();
    Mesh { vertices, indices: u.indices.clone(), texture: None }
}

const SPHERE_VS: &str = r#"#version 100
attribute vec3 position;
attribute vec4 color0;
attribute vec4 normal;
varying lowp vec4 color;
varying mediump vec3 nrm;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
    color = color0 / 255.0;
    nrm = normal.xyz;
}"#;

const SPHERE_FS: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying mediump vec3 nrm;
void main() {
    vec3 n = normalize(nrm);
    vec3 l = normalize(vec3(-0.35, 0.75, 0.55));
    vec3 v = vec3(0.0, 0.0, 1.0);
    float diff = max(dot(n, l), 0.0);
    float spec = pow(max(dot(n, normalize(l + v)), 0.0), 40.0);
    float rim = pow(1.0 - max(dot(n, v), 0.0), 3.0);
    vec3 c = color.rgb * (0.38 + 0.72 * diff) + vec3(0.55 * spec) + color.rgb * rim * 0.35;
    gl_FragColor = vec4(c, 1.0);
}"#;
