//! High scores, saved in %APPDATA%\gridshot\scores.txt as one round per line:
//! `hits misses sensitivity duration date`.
//! Rounds are ranked by hits, then by fewer misses, then by who got there first.
//! Each round length (60 s, 30 s, ...) keeps its own top list.

use std::path::PathBuf;

pub const KEEP: usize = 10;

#[derive(Clone, Debug, PartialEq)]
pub struct Score {
    pub hits: u32,
    pub misses: u32,
    pub sens: f32,
    pub duration: u32,
    /// Local date the round was played, `YYYY-MM-DD`.
    pub date: String,
}

impl Score {
    pub fn accuracy(&self) -> f32 {
        let shots = self.hits + self.misses;
        if shots == 0 {
            0.0
        } else {
            self.hits as f32 * 100.0 / shots as f32
        }
    }

    /// `15 Nov` from `2026-11-15`.
    pub fn short_date(&self) -> String {
        const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let mut p = self.date.split('-').skip(1);
        match (p.next().and_then(|m| m.parse::<usize>().ok()), p.next().and_then(|d| d.parse::<u32>().ok())) {
            (Some(m @ 1..=12), Some(d)) => format!("{d} {}", M[m - 1]),
            _ => self.date.clone(),
        }
    }
}

#[derive(Default)]
pub struct Scores {
    list: Vec<Score>,
}

impl Scores {
    fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("gridshot").join("scores.txt")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).map(|t| Self::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, self.to_text());
    }

    pub fn parse(text: &str) -> Self {
        let mut list: Vec<Score> = text
            .lines()
            .filter_map(|line| {
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.len() != 5 {
                    return None;
                }
                Some(Score {
                    hits: f[0].parse().ok()?,
                    misses: f[1].parse().ok()?,
                    sens: f[2].parse().ok()?,
                    duration: f[3].parse().ok()?,
                    date: f[4].to_owned(),
                })
            })
            .collect();
        sort(&mut list);
        Scores { list }
    }

    pub fn to_text(&self) -> String {
        self.list
            .iter()
            .map(|s| format!("{} {} {} {} {}\n", s.hits, s.misses, s.sens, s.duration, s.date))
            .collect()
    }

    /// Add a finished round. Returns its place (1 = best) if it made the top list.
    pub fn add(&mut self, s: Score) -> Option<usize> {
        let duration = s.duration;
        // Newest goes last among equals, so an equal score does not push an older one down.
        let at = self.list.iter().position(|o| better(&s, o)).unwrap_or(self.list.len());
        self.list.insert(at, s);
        // Trim this round length back to KEEP entries.
        let mut seen = 0;
        let mut rank = None;
        let mut i = 0;
        self.list.retain(|o| {
            let idx = i;
            i += 1;
            if o.duration != duration {
                return true;
            }
            seen += 1;
            if idx == at {
                rank = (seen <= KEEP).then_some(seen);
            }
            seen <= KEEP
        });
        rank
    }

    pub fn top(&self, duration: u32) -> Vec<&Score> {
        self.list.iter().filter(|s| s.duration == duration).take(KEEP).collect()
    }
}

/// Strictly better: more hits, or the same hits with fewer misses.
fn better(a: &Score, b: &Score) -> bool {
    (a.hits, std::cmp::Reverse(a.misses)) > (b.hits, std::cmp::Reverse(b.misses))
}

fn sort(list: &mut [Score]) {
    // Stable sort keeps file order (older first) among equal scores.
    list.sort_by(|a, b| (b.hits, a.misses).cmp(&(a.hits, b.misses)));
}

/// Today's local date as `YYYY-MM-DD`.
#[cfg(windows)]
pub fn today() -> String {
    #[repr(C)]
    #[derive(Default)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        ms: u16,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLocalTime(t: *mut SystemTime);
    }
    let mut t = SystemTime::default();
    unsafe { GetLocalTime(&mut t) };
    format!("{:04}-{:02}-{:02}", t.year, t.month, t.day)
}

#[cfg(not(windows))]
pub fn today() -> String {
    "1970-01-01".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(hits: u32, misses: u32, duration: u32) -> Score {
        Score { hits, misses, sens: 0.35, duration, date: "2026-11-15".into() }
    }

    #[test]
    fn ranks_by_hits_then_fewer_misses() {
        let mut b = Scores::default();
        assert_eq!(b.add(s(80, 5, 60)), Some(1));
        assert_eq!(b.add(s(90, 20, 60)), Some(1));
        assert_eq!(b.add(s(80, 2, 60)), Some(2));
        assert_eq!(b.add(s(80, 5, 60)), Some(4), "a tie goes below the older score");
        let hits: Vec<(u32, u32)> = b.top(60).iter().map(|x| (x.hits, x.misses)).collect();
        assert_eq!(hits, [(90, 20), (80, 2), (80, 5), (80, 5)]);
    }

    #[test]
    fn keeps_ten_per_round_length() {
        let mut b = Scores::default();
        for h in 1..=12 {
            b.add(s(h * 10, 0, 60));
        }
        b.add(s(5, 0, 30));
        assert_eq!(b.top(60).len(), KEEP);
        assert_eq!(b.top(60).last().unwrap().hits, 30);
        assert_eq!(b.add(s(1, 0, 60)), None, "too low to make the list");
        assert_eq!(b.top(60).len(), KEEP);
        assert_eq!(b.top(30)[0].hits, 5, "other round lengths are untouched");
    }

    #[test]
    fn saves_and_loads() {
        let mut b = Scores::default();
        b.add(s(70, 3, 60));
        b.add(s(75, 9, 60));
        let back = Scores::parse(&b.to_text());
        assert_eq!(back.top(60), b.top(60));
        assert!(Scores::parse("garbage\n1 2\n").top(60).is_empty());
    }

    #[test]
    fn accuracy_and_date() {
        assert_eq!(s(9, 1, 60).accuracy(), 90.0);
        assert_eq!(s(0, 0, 60).accuracy(), 0.0);
        assert_eq!(s(1, 0, 60).short_date(), "15 Nov");
    }
}
