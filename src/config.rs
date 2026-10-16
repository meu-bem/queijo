//! User settings, stored in `~/.config/queijo/config.toml`.
//!
//! Global shortcuts are not stored here: they are GNOME keyboard shortcuts and
//! GNOME's settings are the source of truth (see `shortcut.rs`).

use std::path::PathBuf;

use chrono::Local;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StartArea {
    #[default]
    Selection,
    Screen,
    Window,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StartKind {
    #[default]
    Screenshot,
    Video,
    Gif,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpg",
        }
    }
}

/// Where one kind of capture goes and how it is named.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Target {
    /// `~` is expanded.
    pub folder: String,
    /// See [`render`] for the placeholders.
    pub name: String,
}

#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub start_area: StartArea,
    pub start_kind: StartKind,
    pub show_pointer: bool,
    pub notifications: bool,
    /// Ctrl+C also writes a file.
    pub copy_also_saves: bool,
    /// Ctrl+S also puts the image on the clipboard.
    pub save_also_copies: bool,

    pub screenshots: Target,
    pub videos: Target,
    pub gifs: Target,
    pub image_format: ImageFormat,
    pub jpeg_quality: u8,

    /// Seconds counted down before a recording starts (0 = start right away).
    pub countdown: u32,
    pub video_fps: u32,
    pub gif_fps: u32,
    /// Wider GIFs are scaled down (0 = keep the recorded size).
    pub gif_max_width: u32,

    /// Keep the last colour and tool sizes for next time.
    pub remember_tools: bool,
    pub color: [u8; 3],
    pub sizes: Vec<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            start_area: StartArea::Selection,
            start_kind: StartKind::Screenshot,
            show_pointer: false,
            notifications: true,
            copy_also_saves: false,
            save_also_copies: false,
            screenshots: Target { folder: "~/Pictures".into(), name: "Screenshot_{date}_{time}".into() },
            videos: Target { folder: "~/Videos/Screencasts".into(), name: "Screencast_{date}_{time}".into() },
            gifs: Target { folder: "~/Videos/Screencasts".into(), name: "GIF_{date}_{time}".into() },
            image_format: ImageFormat::Png,
            jpeg_quality: 92,
            countdown: 3,
            video_fps: 30,
            gif_fps: 25,
            gif_max_width: 0,
            remember_tools: true,
            color: [255, 59, 48],
            sizes: Vec::new(),
        }
    }
}

impl Config {
    pub fn path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home().join(".config"));
        base.join("queijo").join("config.toml")
    }

    /// Missing file → defaults. A broken file is reported and ignored rather than fatal.
    pub fn load() -> Self {
        match std::fs::read_to_string(Self::path()) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("queijo: ignoring {}: {e}", Self::path().display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Write-then-rename so a crash never leaves half a file.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    }
}

impl Default for Target {
    fn default() -> Self {
        Self { folder: "~/Pictures".into(), name: "Queijo_{date}_{time}".into() }
    }
}

impl Target {
    pub fn dir(&self) -> PathBuf {
        expand(&self.folder)
    }

    /// A path in the folder that doesn't exist yet: `name.ext`, or `name_2.ext`, …
    pub fn unique_path(&self, extension: &str, width: u32, height: u32) -> PathBuf {
        let dir = self.dir();
        let stem = render(&self.name, width, height);
        let mut path = dir.join(format!("{stem}.{extension}"));
        let mut n = 2;
        while path.exists() {
            path = dir.join(format!("{stem}_{n}.{extension}"));
            n += 1;
        }
        path
    }
}

/// Fills a file name template. Unknown placeholders are left alone; slashes are
/// replaced so a name can't escape its folder.
pub fn render(template: &str, width: u32, height: u32) -> String {
    let now = Local::now();
    let mut name = template.to_string();
    for (key, value) in [
        ("{date}", now.format("%Y-%m-%d").to_string()),
        ("{time}", now.format("%H-%M-%S").to_string()),
        ("{year}", now.format("%Y").to_string()),
        ("{month}", now.format("%m").to_string()),
        ("{day}", now.format("%d").to_string()),
        ("{hour}", now.format("%H").to_string()),
        ("{minute}", now.format("%M").to_string()),
        ("{second}", now.format("%S").to_string()),
        ("{width}", width.to_string()),
        ("{height}", height.to_string()),
        ("{timestamp}", now.timestamp().to_string()),
    ] {
        name = name.replace(key, &value);
    }
    let name = name.replace(['/', '\0'], "-");
    let name = name.trim();
    if name.is_empty() {
        "Queijo".into()
    } else {
        name.to_string()
    }
}

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into()))
}

pub fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    if path == "~" {
        home()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home().join(rest)
    } else if path.is_empty() {
        home()
    } else {
        PathBuf::from(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_templates() {
        assert_eq!(render("Shot_{width}x{height}", 10, 20), "Shot_10x20");
        assert_eq!(render("a/b {unknown}", 1, 1), "a-b {unknown}");
        assert_eq!(render("  ", 1, 1), "Queijo");
        assert!(render("{date}", 1, 1).len() == 10);
    }

    #[test]
    fn partial_files_use_defaults() {
        let cfg: Config = toml::from_str("countdown = 5\n[gifs]\nfolder = \"/tmp\"\nname = \"x\"").unwrap();
        assert_eq!(cfg.countdown, 5);
        assert_eq!(cfg.gifs.folder, "/tmp");
        assert_eq!(cfg.video_fps, 30);
    }
}
