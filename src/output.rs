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
    let mut cmd = Command::new("notify-send");
    cmd.args(["--app-name=Queijo", "--expire-time=4000"]);
    if let Some(icon) = icon {
        cmd.arg(format!("--icon={}", icon.display()));
    }
    let _ = cmd.arg(summary).arg(body).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// `~/Pictures/x.png` instead of `/home/me/Pictures/x.png`, for display.
pub fn tilde(path: &Path) -> String {
    let s = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && s.starts_with(&home) => format!("~{}", &s[home.len()..]),
        _ => s,
    }
}
