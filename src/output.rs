use std::collections::HashMap;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use image::RgbaImage;

use crate::config::{ImageFormat, Target};
use std::sync::atomic::{AtomicBool, Ordering};

pub fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Light compression: several times faster to encode, which is what matters
/// for the clipboard (the receiving app re-encodes anyway).
pub fn encode_png_fast(image: &RgbaImage) -> Result<Vec<u8>, String> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;

    let mut bytes = Vec::new();
    PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Sub)
        .write_image(image.as_raw(), image.width(), image.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Puts the PNG on the clipboard. `wl-copy`/`xclip` fork into the background and
/// keep serving the data after we exit, which is what makes paste work later.
pub fn copy_png(png: &[u8]) -> Result<(), String> {
    copy_data("image/png", png)
}

pub fn copy_data(mime: &str, data: &[u8]) -> Result<(), String> {
    let png = data;
    let wl: [&str; 2] = ["--type", mime];
    let x: [&str; 5] = ["-selection", "clipboard", "-t", mime, "-i"];
    let attempts: [(&str, &[&str]); 2] = if crate::monitors::is_wayland() {
        [("wl-copy", &wl), ("xclip", &x)]
    } else {
        [("xclip", &x), ("wl-copy", &wl)]
    };

    let mut last_err = crate::i18n::t("error.no_clipboard").to_string();
    for (program, args) in attempts {
        match pipe_to(program, args, png) {
            Ok(()) => return Ok(()),
            Err(e) => last_err = format!("{program}: {e}"),
        }
    }
    Err(last_err)
}

fn pipe_to(program: &str, args: &[&str], data: &[u8]) -> Result<(), String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    child.stdin.take().ok_or("no stdin")?.write_all(data).map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("exited with {status}"))
    }
}

/// Writes the image where `target` says, named by its template.
pub fn save_image(
    image: &RgbaImage,
    target: &Target,
    format: ImageFormat,
    jpeg_quality: u8,
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(target.dir()).map_err(|e| e.to_string())?;
    let path = target.unique_path(format.extension(), image.width(), image.height());
    let bytes = match format {
        ImageFormat::Png => encode_png(image)?,
        ImageFormat::Jpeg => {
            use image::codecs::jpeg::JpegEncoder;
            // JPEG has no transparency: areas outside every monitor become black.
            let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
            let mut bytes = Vec::new();
            JpegEncoder::new_with_quality(&mut bytes, jpeg_quality.clamp(1, 100))
                .encode_image(&rgb)
                .map_err(|e| e.to_string())?;
            bytes
        }
    };
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

static NOTIFICATIONS: AtomicBool = AtomicBool::new(true);

/// Turns the informational notifications ("saved", …) on or off. Errors always show.
pub fn set_notifications(enabled: bool) {
    NOTIFICATIONS.store(enabled, Ordering::Relaxed);
}

pub fn notify(summary: &str, body: &str, icon: Option<&Path>) {
    if NOTIFICATIONS.load(Ordering::Relaxed) {
        notify_always(summary, body, icon);
    }
}

pub fn notify_error(summary: &str, body: &str) {
    notify_always(summary, body, None);
}

fn notify_always(summary: &str, body: &str, icon: Option<&Path>) {
    // Tests and CI run the real binary; they shouldn't pop up desktop notifications.
    if std::env::var_os("QUEIJO_NO_NOTIFY").is_some() {
        return;
    }
    if let Err(e) = notify_dbus(summary, body, icon) {
        eprintln!("queijo: notification over D-Bus failed ({e}), trying notify-send");
        notify_send(summary, body, icon);
    }
}

/// `org.freedesktop.Notifications.Notify`, which every desktop's notifications speak.
fn notify_dbus(summary: &str, body: &str, icon: Option<&Path>) -> Result<(), String> {
    use zbus::zvariant::Value;

    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let mut hints: HashMap<&str, Value> = HashMap::new();
    if let Some(icon) = icon {
        hints.insert("image-path", icon.to_string_lossy().into_owned().into());
    }
    conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &("Queijo", 0u32, "", summary, body, Vec::<&str>::new(), hints, NOTIFICATION_MS),
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

const NOTIFICATION_MS: i32 = 4000;

fn notify_send(summary: &str, body: &str, icon: Option<&Path>) {
    let mut cmd = Command::new("notify-send");
    cmd.args(["--app-name=Queijo", &format!("--expire-time={NOTIFICATION_MS}")]);
    if let Some(icon) = icon {
        cmd.arg(format!("--icon={}", icon.display()));
    }
    let _ = cmd.arg(summary).arg(body).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// `~/Pictures/x.png` instead of `/home/me/Pictures/x.png`, for display.
pub fn tilde(path: &Path) -> String {
    tilde_in(path, std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

/// Compares whole path components, so `/home/ann2/x` isn't under `/home/ann`.
fn tilde_in(path: &Path, home: Option<&Path>) -> String {
    match home.filter(|h| !h.as_os_str().is_empty()).and_then(|h| path.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ImageFormat, Target};
    use image::Rgba;

    fn sample(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            Rgba([(x * 7 % 256) as u8, (y * 5 % 256) as u8, ((x + y) % 256) as u8, 255])
        })
    }

    fn target(dir: &Path, name: &str) -> Target {
        Target { folder: dir.to_string_lossy().into_owned(), name: name.into() }
    }

    #[test]
    fn png_round_trips_exactly() {
        let image = sample(64, 48);
        let decoded = image::load_from_memory(&encode_png(&image).unwrap()).unwrap().to_rgba8();
        assert_eq!(decoded, image);
    }

    #[test]
    fn fast_png_is_lossless_too() {
        let image = sample(200, 120);
        let fast = encode_png_fast(&image).unwrap();
        assert_eq!(image::load_from_memory(&fast).unwrap().to_rgba8(), image);
    }

    #[test]
    fn saves_a_png_named_by_the_template() {
        let dir = tempfile::tempdir().unwrap();
        let image = sample(30, 20);
        let path = save_image(&image, &target(dir.path(), "shot_{width}x{height}"), ImageFormat::Png, 90).unwrap();
        assert_eq!(path, dir.path().join("shot_30x20.png"));
        assert_eq!(image::open(&path).unwrap().to_rgba8(), image);
    }

    #[test]
    fn saves_a_jpeg_without_transparency() {
        let dir = tempfile::tempdir().unwrap();
        let mut image = sample(40, 40);
        image.put_pixel(0, 0, Rgba([0, 0, 0, 0])); // outside every monitor
        let path = save_image(&image, &target(dir.path(), "photo"), ImageFormat::Jpeg, 92).unwrap();
        assert_eq!(path.extension().unwrap(), "jpg");
        let decoded = image::open(&path).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (40, 40));
        assert!(!decoded.color().has_alpha());
    }

    #[test]
    fn never_overwrites_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let image = sample(8, 8);
        let t = target(dir.path(), "same");
        let first = save_image(&image, &t, ImageFormat::Png, 90).unwrap();
        let second = save_image(&image, &t, ImageFormat::Png, 90).unwrap();
        let third = save_image(&image, &t, ImageFormat::Png, 90).unwrap();
        assert_eq!(first.file_name().unwrap(), "same.png");
        assert_eq!(second.file_name().unwrap(), "same_2.png");
        assert_eq!(third.file_name().unwrap(), "same_3.png");
    }

    #[test]
    fn creates_missing_folders() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        let path = save_image(&sample(4, 4), &target(&nested, "x"), ImageFormat::Png, 90).unwrap();
        assert!(path.is_file());
    }

    #[test]
    fn a_name_cannot_escape_its_folder() {
        let dir = tempfile::tempdir().unwrap();
        let path = save_image(&sample(4, 4), &target(dir.path(), "../evil"), ImageFormat::Png, 90).unwrap();
        assert_eq!(path.parent().unwrap(), dir.path());
    }

    #[test]
    fn shortens_the_home_folder() {
        let home = Path::new("/home/ann");
        assert_eq!(tilde_in(Path::new("/home/ann/Pictures/a.png"), Some(home)), "~/Pictures/a.png");
        assert_eq!(tilde_in(Path::new("/home/ann"), Some(home)), "~");
        // A folder that merely starts with the same letters is not inside it.
        assert_eq!(tilde_in(Path::new("/home/ann2/x"), Some(home)), "/home/ann2/x");
        assert_eq!(tilde_in(Path::new("/mnt/home/ann/x"), Some(home)), "/mnt/home/ann/x");
        assert_eq!(tilde_in(Path::new("/tmp/x"), None), "/tmp/x");
        assert_eq!(tilde_in(Path::new("/tmp/x"), Some(Path::new(""))), "/tmp/x");
    }
}
