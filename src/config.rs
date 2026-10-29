//! User settings, stored in `~/.config/queijo/config.toml`.
//!
//! Global shortcuts are not stored here: they are GNOME keyboard shortcuts and
//! GNOME's settings are the source of truth (see `shortcut.rs`).

use std::path::{Path, PathBuf};

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
    /// `auto` (follow the system) or a language code such as `en` or `pt-BR`.
    pub language: String,
    pub start_area: StartArea,
    pub start_kind: StartKind,
    pub show_pointer: bool,
    /// Show a still picture of the screen under the dim instead of the live screen.
    pub freeze_screen: bool,
    /// Languages tesseract reads in, as it names them (`por+eng`); empty follows the system.
    pub ocr_languages: String,
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
    /// What the desktop's screen-casting portal gave back to pick the same screen
    /// again without asking (empty until it has been used).
    pub screencast_token: String,

    /// Keep the last colour and tool sizes for next time.
    pub remember_tools: bool,
    pub color: [u8; 3],
    pub sizes: Vec<f32>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            language: crate::i18n::AUTO.into(),
            start_area: StartArea::Selection,
            start_kind: StartKind::Screenshot,
            show_pointer: false,
            freeze_screen: false,
            ocr_languages: String::new(),
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
            screencast_token: String::new(),
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
        Self::load_from(&Self::path())
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("queijo: ignoring {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        self.save_to(&Self::path())
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        std::fs::create_dir_all(path.parent().unwrap_or(Path::new("."))).map_err(|e| e.to_string())?;
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        // Write-then-rename so a crash never leaves half a file.
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
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

#[cfg(test)]
mod file_tests {
    use super::*;

    #[test]
    fn every_placeholder_is_filled() {
        let name = render("{year}-{month}-{day} {hour}:{minute}:{second} {width}x{height} #{timestamp}", 1920, 1080);
        assert!(!name.contains('{'), "{name}");
        assert!(name.contains("1920x1080"));
        let parts: Vec<&str> = name.split([' ', '-', ':']).collect();
        assert_eq!(parts[0].len(), 4, "year");
        assert!(parts[1..6].iter().all(|p| p.len() == 2), "month, day, hour, minute, second: {name}");
    }

    #[test]
    fn unique_paths_count_up() {
        let dir = tempfile::tempdir().unwrap();
        let target = Target { folder: dir.path().to_string_lossy().into_owned(), name: "shot".into() };
        let first = target.unique_path("png", 1, 1);
        assert_eq!(first.file_name().unwrap(), "shot.png");
        std::fs::write(&first, b"x").unwrap();
        let second = target.unique_path("png", 1, 1);
        assert_eq!(second.file_name().unwrap(), "shot_2.png");
        std::fs::write(&second, b"x").unwrap();
        assert_eq!(target.unique_path("png", 1, 1).file_name().unwrap(), "shot_3.png");
        assert_eq!(target.unique_path("gif", 1, 1).file_name().unwrap(), "shot.gif", "other extensions are separate");
    }

    #[test]
    fn expands_the_home_folder() {
        assert_eq!(expand("~"), home());
        assert_eq!(expand("~/Pictures"), home().join("Pictures"));
        assert_eq!(expand("  ~/x  "), home().join("x"));
        assert_eq!(expand(""), home());
        assert_eq!(expand("/var/tmp"), PathBuf::from("/var/tmp"));
        assert_eq!(expand("~other/x"), PathBuf::from("~other/x"), "only the current user's home");
    }

    #[test]
    fn saves_and_loads_every_setting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("config.toml");
        let cfg = Config {
            language: "pt-BR".into(),
            start_area: StartArea::Window,
            start_kind: StartKind::Gif,
            show_pointer: true,
            notifications: false,
            copy_also_saves: true,
            save_also_copies: true,
            image_format: ImageFormat::Jpeg,
            jpeg_quality: 70,
            countdown: 0,
            video_fps: 60,
            gif_fps: 10,
            gif_max_width: 960,
            color: [1, 2, 3],
            sizes: vec![1.0, 2.5, 30.0],
            gifs: Target { folder: "/tmp/g".into(), name: "g_{time}".into() },
            ..Config::default()
        };
        cfg.save_to(&path).unwrap();
        assert_eq!(Config::load_from(&path), cfg);
        let leftovers: Vec<_> = std::fs::read_dir(path.parent().unwrap()).unwrap().collect();
        assert_eq!(leftovers.len(), 1, "no temporary file is left next to the config");
    }

    #[test]
    fn a_missing_or_broken_file_means_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Config::load_from(&dir.path().join("nope.toml")), Config::default());
        let broken = dir.path().join("broken.toml");
        std::fs::write(&broken, "countdown = [oops").unwrap();
        assert_eq!(Config::load_from(&broken), Config::default());
        let wrong_type = dir.path().join("wrong.toml");
        std::fs::write(&wrong_type, "countdown = \"three\"").unwrap();
        assert_eq!(Config::load_from(&wrong_type), Config::default());
    }

    #[test]
    fn older_and_newer_files_still_load() {
        // A file from before `language` existed, with a key a later version might add.
        let cfg: Config = toml::from_str("countdown = 5\nsome_future_option = true\n").unwrap();
        assert_eq!(cfg.countdown, 5);
        assert_eq!(cfg.language, crate::i18n::AUTO);
    }

    #[test]
    fn defaults_are_valid() {
        let cfg = Config::default();
        assert_eq!(cfg.language, crate::i18n::AUTO);
        assert!(cfg.countdown <= 10);
        assert!([24, 30, 60].contains(&cfg.video_fps));
        assert!([10, 20, 25, 50].contains(&cfg.gif_fps));
        assert!((50..=100).contains(&cfg.jpeg_quality));
        for target in [&cfg.screenshots, &cfg.videos, &cfg.gifs] {
            assert!(!target.folder.is_empty());
            assert!(target.name.contains("{date}") && target.name.contains("{time}"), "names are unique by default");
        }
        assert_ne!(cfg.videos.name, cfg.gifs.name);
        assert_eq!(ImageFormat::Png.extension(), "png");
        assert_eq!(ImageFormat::Jpeg.extension(), "jpg");
    }

    #[test]
    fn the_file_uses_readable_names() {
        let text = toml::to_string_pretty(&Config::default()).unwrap();
        assert!(text.contains("start_area = \"selection\""), "{text}");
        assert!(text.contains("image_format = \"png\""), "{text}");
        assert!(text.contains("[screenshots]"), "{text}");
    }
}
