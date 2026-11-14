//! Scripted runs that need no mouse or keyboard, switched on with the GRIDSHOT_DEMO
//! environment variable. Normal play never touches this.
//!
//! - `GRIDSHOT_DEMO=shots` (or `shots:<dir>`) draws each screen with sample data, saves
//!   menu.png, playing.png, paused.png and results.png (default dir `docs/screenshots`), then quits.
//! - `GRIDSHOT_DEMO=bench` (or `bench:<seconds>`, default 30) plays a round by itself, flicking
//!   to targets and shooting like a player, so CPU / memory / GPU use can be measured. Then quits.
//!   Settings and high scores are never saved in either mode.

use super::{wall_point, Aim, Grid, HitFx, Score, Scores, Shot, State};
use macroquad::audio::{play_sound, PlaySoundParams, Sound};
use macroquad::prelude::*;

const SAMPLE_SCORES: &str = "\
88 11 0.35 60 2026-11-14
85 2 0.4 60 2026-11-14
80 7 0.35 60 2026-11-12
78 3 0.35 60 2026-11-12
71 15 0.3 60 2026-11-10
69 4 0.35 60 2026-11-09
64 9 0.35 60 2026-11-08
";

const SHOTS: [&str; 4] = ["menu", "playing", "paused", "results"];

enum Mode {
    Shots { dir: String, scene: usize },
    Bench { secs: f64, flick: Option<Flick>, shots: u32 },
}

struct Flick {
    from: (f32, f32),
    to: (f32, f32),
    start: f64,
}

pub struct Demo {
    mode: Mode,
    scene_start: Option<f64>,
}

/// Everything in the main loop a demo is allowed to set.
pub struct World<'a> {
    pub state: &'a mut State,
    pub scores: &'a mut Scores,
    pub hits: &'a mut u32,
    pub misses: &'a mut u32,
    pub fx: &'a mut Vec<HitFx>,
    pub aim: &'a mut Aim,
    pub round_start: &'a mut f64,
    pub pointer_live: &'a mut bool,
    pub grid: &'a mut Grid,
    pub sound: Option<&'a Sound>,
}

impl Demo {
    pub fn from_env() -> Option<Demo> {
        let v = std::env::var("GRIDSHOT_DEMO").ok()?;
        let (kind, arg) = v.split_once(':').map_or((v.as_str(), None), |(k, a)| (k, Some(a)));
        let mode = match kind {
            "shots" => Mode::Shots { dir: arg.unwrap_or("docs/screenshots").to_owned(), scene: 0 },
            "bench" => Mode::Bench { secs: arg.and_then(|a| a.parse().ok()).unwrap_or(30.0), flick: None, shots: 0 },
            _ => return None,
        };
        Some(Demo { mode, scene_start: None })
    }

    /// Set up this frame. Returns false when the demo is finished and the game should quit.
    pub fn stage(&mut self, now: f64, w: World) -> bool {
        let start = *self.scene_start.get_or_insert(now);
        match &mut self.mode {
            Mode::Shots { scene, .. } => {
                if *scene >= SHOTS.len() {
                    return false;
                }
                shot_scene(*scene, now, w);
                true
            }
            Mode::Bench { secs, flick, shots } => {
                if now - start > *secs {
                    return false;
                }
                if *w.state != State::Playing {
                    *w.state = State::Playing;
                    *w.round_start = now;
                }
                if now - *w.round_start >= 59.0 {
                    *w.round_start = now; // keep the timer going for long runs
                }
                bench_frame(now, flick, shots, w);
                true
            }
        }
    }

    /// Call after the frame is drawn (before `next_frame`). Saves screenshots when due.
    pub fn after_draw(&mut self, now: f64) {
        let Mode::Shots { dir, scene } = &mut self.mode else { return };
        let Some(start) = self.scene_start else { return };
        // Wait long enough for the FPS / CPU meter to fill in.
        if *scene < SHOTS.len() && now - start > 1.2 {
            let _ = std::fs::create_dir_all(&*dir);
            let path = format!("{dir}/{}.png", SHOTS[*scene]);
            let mut img = get_screen_data();
            // The dimmed menu overlay leaves alpha below 1; screenshots should be opaque.
            img.bytes.chunks_exact_mut(4).for_each(|px| px[3] = 255);
            img.export_png(&path);
            *scene += 1;
            self.scene_start = Some(now);
        }
    }
}

fn shot_scene(scene: usize, now: f64, w: World) {
    *w.pointer_live = false;
    match SHOTS[scene] {
        "menu" => {
            *w.pointer_live = true;
            *w.scores = Scores::parse(SAMPLE_SCORES);
            *w.state = State::Menu;
            *w.aim = Aim::centre();
        }
        "playing" => {
            *w.grid = Grid::new(11);
            *w.state = State::Playing;
            *w.hits = 37;
            *w.misses = 3;
            *w.round_start = now - 25.6;
            *w.aim = Aim { yaw: 3.1, pitch: 1.4 };
            // A target just shot, and two recent misses.
            let shot = (0..9).find(|c| !w.grid.live_cells().contains(c)).unwrap_or(0);
            w.fx.clear();
            w.fx.push(HitFx { pos: super::game::cell_center(shot), born: now - 0.12, miss: false });
            w.fx.push(HitFx { pos: wall_point(Aim { yaw: -4.0, pitch: 4.5 }.dir()), born: now - 0.3, miss: true });
            w.fx.push(HitFx { pos: wall_point(Aim { yaw: 9.5, pitch: -2.0 }.dir()), born: now - 0.5, miss: true });
        }
        "paused" => {
            w.fx.clear();
            *w.hits = 58;
            *w.misses = 5;
            *w.round_start = now - 41.3;
            *w.state = State::Paused { at: now };
        }
        _ => {
            w.fx.clear();
            *w.hits = 94;
            *w.misses = 4;
            let mut s = Scores::parse(SAMPLE_SCORES);
            let rank = s.add(Score { hits: 94, misses: 4, sens: 0.35, duration: 60, date: "2026-11-15".into() });
            *w.scores = s;
            *w.state = State::Results { rank };
            *w.pointer_live = true; // show the buttons as clickable
            *w.aim = Aim::centre();
        }
    }
}

/// Flick to a live target in 0.22 s, shoot, repeat. Every seventh shot lands just wide.
fn bench_frame(now: f64, flick: &mut Option<Flick>, shots: &mut u32, w: World) {
    const FLICK_SECS: f64 = 0.22;
    let f = flick.get_or_insert_with(|| {
        let i = (*shots as usize) % 3;
        let p = w.grid.live_centers().nth(i).unwrap_or(Vec3::NEG_Z);
        let wide = if *shots % 7 == 6 { 4.5 } else { 0.0 };
        let yaw = p.x.atan2(-p.z).to_degrees() + wide;
        let pitch = p.y.atan2((p.x * p.x + p.z * p.z).sqrt()).to_degrees();
        Flick { from: (w.aim.yaw_signed(), w.aim.pitch), to: (yaw, pitch), start: now }
    });
    let t = ((now - f.start) / FLICK_SECS).min(1.0) as f32;
    let ease = 1.0 - (1.0 - t).powi(3);
    w.aim.yaw = (f.from.0 + (f.to.0 - f.from.0) * ease).rem_euclid(360.0);
    w.aim.pitch = f.from.1 + (f.to.1 - f.from.1) * ease;
    if t >= 1.0 {
        match w.grid.shoot(Vec3::ZERO, w.aim.dir()) {
            Shot::Hit(pos) => {
                *w.hits += 1;
                w.fx.push(HitFx { pos, born: now, miss: false });
                if let Some(s) = w.sound {
                    play_sound(s, PlaySoundParams { looped: false, volume: 0.0 });
                }
            }
            Shot::Miss => {
                *w.misses += 1;
                w.fx.push(HitFx { pos: wall_point(w.aim.dir()), born: now, miss: true });
            }
        }
        *shots += 1;
        *flick = None;
    }
}
