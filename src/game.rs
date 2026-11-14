//! Pure game logic: the 3x3 grid, spawning, hit testing, and mouse-to-angle math.

use macroquad::math::{vec3, Vec3};

/// Valorant turns 0.07 degrees per mouse count at sensitivity 1.0.
pub const VALORANT_YAW: f32 = 0.07;

/// Distance from the player to the grid plane (metres).
pub const TARGET_Z: f32 = -12.0;
/// Gap between neighbouring grid spots, centre to centre.
pub const SPACING: f32 = 2.0;
/// Target sphere radius.
pub const RADIUS: f32 = 0.65;
/// Targets on screen at once.
pub const LIVE: usize = 3;

pub enum Shot {
    Hit(Vec3),
    Miss,
}

/// Where the player looks, in degrees. Yaw 0 / pitch 0 looks straight at the grid.
pub struct Aim {
    pub yaw: f32,
    pub pitch: f32,
}

impl Aim {
    pub fn centre() -> Self {
        Aim { yaw: 0.0, pitch: 0.0 }
    }

    /// Yaw in -180..180, for smooth turning across 0.
    pub fn yaw_signed(&self) -> f32 {
        if self.yaw > 180.0 {
            self.yaw - 360.0
        } else {
            self.yaw
        }
    }

    pub fn dir(&self) -> Vec3 {
        let (y, p) = (self.yaw.to_radians(), self.pitch.to_radians());
        vec3(p.cos() * y.sin(), p.sin(), -p.cos() * y.cos())
    }

    /// Turn by raw mouse counts, exactly like Valorant at `sens`.
    pub fn turn(&mut self, dx_counts: f32, dy_counts: f32, sens: f32) {
        let k = sens * VALORANT_YAW;
        self.yaw = (self.yaw + dx_counts * k).rem_euclid(360.0);
        self.pitch = (self.pitch - dy_counts * k).clamp(-89.0, 89.0);
    }
}

pub struct Grid {
    live: [usize; LIVE],
    rng: u64,
}

impl Grid {
    pub fn new(seed: u64) -> Self {
        let mut g = Grid { live: [usize::MAX; LIVE], rng: seed | 1 };
        for i in 0..LIVE {
            g.live[i] = g.pick_free(None);
        }
        g
    }

    pub fn live_cells(&self) -> [usize; LIVE] {
        self.live
    }

    pub fn live_centers(&self) -> impl Iterator<Item = Vec3> + '_ {
        self.live.iter().map(|&c| cell_center(c))
    }

    /// Shoot a ray from `origin` along `dir`. On a hit the target moves to a
    /// random free spot that is not the one just shot.
    pub fn shoot(&mut self, origin: Vec3, dir: Vec3) -> Shot {
        let mut best: Option<(usize, f32)> = None;
        for (i, &c) in self.live.iter().enumerate() {
            if let Some(t) = ray_sphere(origin, dir, cell_center(c), RADIUS) {
                if best.map_or(true, |(_, bt)| t < bt) {
                    best = Some((i, t));
                }
            }
        }
        match best {
            Some((i, _)) => {
                let shot_cell = self.live[i];
                self.live[i] = usize::MAX;
                self.live[i] = self.pick_free(Some(shot_cell));
                Shot::Hit(cell_center(shot_cell))
            }
            None => Shot::Miss,
        }
    }

    fn pick_free(&mut self, not: Option<usize>) -> usize {
        let free: Vec<usize> = (0..9).filter(|c| !self.live.contains(c) && Some(*c) != not).collect();
        free[(self.next() % free.len() as u64) as usize]
    }

    fn next(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11
    }
}

/// Cell 0 is top-left, 8 is bottom-right.
pub fn cell_center(cell: usize) -> Vec3 {
    let col = (cell % 3) as f32 - 1.0;
    let row = (cell / 3) as f32 - 1.0;
    vec3(col * SPACING, -row * SPACING, TARGET_Z)
}

/// Distance along the ray to the sphere, if the ray hits it in front of the origin.
fn ray_sphere(origin: Vec3, dir: Vec3, center: Vec3, r: f32) -> Option<f32> {
    let d = dir.normalize();
    let oc = center - origin;
    let t = oc.dot(d);
    if t <= 0.0 {
        return None;
    }
    let miss2 = oc.length_squared() - t * t;
    (miss2 <= r * r).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aim_at(p: Vec3) -> Vec3 {
        p.normalize()
    }

    #[test]
    fn starts_with_three_different_spots() {
        for seed in 0..200 {
            let g = Grid::new(seed);
            let c = g.live_cells();
            assert!(c.iter().all(|&x| x < 9));
            assert!(c[0] != c[1] && c[1] != c[2] && c[0] != c[2]);
        }
    }

    #[test]
    fn hit_respawns_in_a_free_spot_that_is_not_the_one_shot() {
        let mut g = Grid::new(42);
        for _ in 0..500 {
            let before = g.live_cells();
            let target = before[0];
            match g.shoot(Vec3::ZERO, aim_at(cell_center(target))) {
                Shot::Hit(p) => assert_eq!(p, cell_center(target)),
                Shot::Miss => panic!("aimed at the centre of a live target"),
            }
            let after = g.live_cells();
            assert!(!after.contains(&target), "came back in the spot just shot");
            assert!(after[1] == before[1] && after[2] == before[2], "other targets moved");
            assert!(after[0] != after[1] && after[0] != after[2]);
        }
    }

    #[test]
    fn every_spot_gets_used() {
        let mut g = Grid::new(7);
        let mut seen = [false; 9];
        for _ in 0..300 {
            for c in g.live_cells() {
                seen[c] = true;
            }
            let t = g.live_cells()[1];
            g.shoot(Vec3::ZERO, aim_at(cell_center(t)));
        }
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn shooting_empty_space_misses() {
        let mut g = Grid::new(3);
        let before = g.live_cells();
        assert!(matches!(g.shoot(Vec3::ZERO, vec3(0.0, 1.0, 0.0)), Shot::Miss));
        assert!(matches!(g.shoot(Vec3::ZERO, vec3(0.0, 0.0, 1.0)), Shot::Miss));
        assert_eq!(before, g.live_cells());
    }

    #[test]
    fn edge_of_target_counts_but_just_outside_does_not() {
        let mut g = Grid::new(9);
        let c = cell_center(g.live_cells()[0]);
        let inside = c + vec3(RADIUS * 0.98, 0.0, 0.0);
        let outside = c + vec3(RADIUS * 1.05, 0.0, 0.0);
        // `outside` could land on a neighbour only if spacing were tiny.
        assert!(SPACING - RADIUS * 1.05 > RADIUS);
        assert!(matches!(g.shoot(Vec3::ZERO, outside.normalize()), Shot::Miss));
        assert!(matches!(g.shoot(Vec3::ZERO, inside.normalize()), Shot::Hit(_)));
    }

    #[test]
    fn valorant_sensitivity_turn_rate() {
        // 0.35 sens: 360 / (0.35 * 0.07) = 14693.9 counts for a full turn.
        let mut a = Aim::centre();
        a.turn(1000.0, 0.0, 0.35);
        assert!((a.yaw - 24.5).abs() < 1e-3);
        // Mouse moved toward the player (positive y) looks down.
        a.turn(0.0, 100.0, 1.0);
        assert!((a.pitch + 7.0).abs() < 1e-4);
        // Pitch stops at straight up/down.
        a.turn(0.0, -1.0e6, 1.0);
        assert_eq!(a.pitch, 89.0);
    }

    #[test]
    fn centre_aim_looks_at_the_middle_spot() {
        let d = Aim::centre().dir();
        let mid = cell_center(4).normalize();
        assert!((d - mid).length() < 1e-6);
        // Yaw right looks to +x.
        let r = Aim { yaw: 10.0, pitch: 0.0 }.dir();
        assert!(r.x > 0.0);
    }
}
