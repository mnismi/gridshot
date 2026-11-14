//! Settings saved as plain `key=value` lines in %APPDATA%\gridshot\settings.txt.

use std::path::PathBuf;

pub struct Settings {
    /// Valorant in-game sensitivity.
    pub sensitivity: f32,
    pub fullscreen: bool,
    /// Off = uncapped frame rate (lower input lag, more GPU). Read at start-up.
    pub vsync: bool,
    /// Round length in seconds.
    pub duration: u32,
    pub volume: f32,
    /// System-wide show / hide key, e.g. `ctrl+shift+g`, or `off`.
    pub hotkey: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { sensitivity: 0.35, fullscreen: false, vsync: true, duration: 60, volume: 0.5, hotkey: crate::hotkey::DEFAULT.to_owned() }
    }
}

pub fn clamp_sens(v: f32) -> f32 {
    (v.clamp(0.001, 10.0) * 1000.0).round() / 1000.0
}

impl Settings {
    fn path() -> PathBuf {
        let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        base.join("gridshot").join("settings.txt")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path()).map(|t| Self::parse(&t)).unwrap_or_default()
    }

    pub fn parse(text: &str) -> Self {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "sensitivity" => s.sensitivity = v.parse().map(clamp_sens).unwrap_or(s.sensitivity),
                "fullscreen" => s.fullscreen = v == "true",
                "vsync" => s.vsync = v != "false",
                "duration" => s.duration = v.parse::<u32>().unwrap_or(s.duration).clamp(5, 600),
                "hotkey" => s.hotkey = v.to_owned(),
                "volume" => s.volume = v.parse::<f32>().unwrap_or(s.volume).clamp(0.0, 1.0),
                _ => {}
            }
        }
        s
    }

    pub fn to_text(&self) -> String {
        format!(
            "sensitivity={}\nfullscreen={}\nvsync={}\nduration={}\nvolume={}\nhotkey={}\n",
            self.sens_text(),
            self.fullscreen,
            self.vsync,
            self.duration,
            self.volume,
            self.hotkey
        )
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(p, self.to_text());
    }

    /// Sensitivity as typed in Valorant: up to three decimals, no trailing zeros.
    pub fn sens_text(&self) -> String {
        let t = format!("{:.3}", self.sensitivity);
        let t = t.trim_end_matches('0').trim_end_matches('.');
        t.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let s = Settings { sensitivity: 0.287, fullscreen: true, vsync: false, duration: 30, volume: 0.25, hotkey: "alt+f9".into() };
        let back = Settings::parse(&s.to_text());
        assert_eq!(back.sensitivity, 0.287);
        assert!(back.fullscreen && !back.vsync);
        assert_eq!(back.duration, 30);
        assert_eq!(back.hotkey, "alt+f9");
    }

    #[test]
    fn bad_lines_fall_back_to_defaults() {
        let s = Settings::parse("sensitivity=abc\nnonsense\nduration=1\n");
        assert_eq!(s.sensitivity, 0.35);
        assert_eq!(s.duration, 5);
    }

    #[test]
    fn sens_text_looks_like_valorant() {
        let mut s = Settings::default();
        assert_eq!(s.sens_text(), "0.35");
        s.sensitivity = 1.0;
        assert_eq!(s.sens_text(), "1");
        s.sensitivity = clamp_sens(0.12345);
        assert_eq!(s.sens_text(), "0.123");
    }
}
