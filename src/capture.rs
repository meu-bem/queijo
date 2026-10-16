use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};

use image::RgbaImage;

use crate::monitors::Monitor;

/// A connection GNOME Shell accepts screenshot requests from.
#[derive(Clone)]
pub struct Shell {
    conn: zbus::blocking::Connection,
    dir: PathBuf,
}

impl Shell {
    fn path(&self, what: &str) -> PathBuf {
        self.dir.join(format!("queijo-{}-{what}.png", std::process::id()))
    }

    /// The window that has focus, frame included. `taken` is set once the Shell
    /// has answered, before decoding.
    pub fn window(&self, taken: &AtomicBool) -> Option<RgbaImage> {
        let shot = shell_shot(&self.conn, "ScreenshotWindow", &self.path("window"), |f| (true, false, false, f).into());
        taken.store(true, Ordering::Release);
        finish(shot)
    }

    /// The whole desktop with the mouse pointer drawn in. `requested` is set just
    /// before asking; the Shell grabs the pixels right away and only then spends
    /// a while encoding them.
    pub fn with_pointer(&self, requested: &AtomicBool) -> Option<RgbaImage> {
        requested.store(true, Ordering::Release);
        finish(shell_shot(&self.conn, "Screenshot", &self.path("pointer"), |f| (true, false, f).into()))
    }
}

fn finish(shot: Result<Decoder, String>) -> Option<RgbaImage> {
    shot.and_then(|h| h.join().map_err(|_| "decoder panicked".to_string())?).map_err(|e| eprintln!("queijo: {e}")).ok()
}

/// Grabs the whole desktop (every monitor) as one image. Also returns the
/// Shell connection when GNOME Shell did the capture, for follow-up requests.
pub fn capture() -> Result<(RgbaImage, Option<Shell>), String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let file = dir.join(format!("queijo-{}-desktop.png", std::process::id()));

    match shell_capture(&file) {
        Ok((desktop, conn)) => Ok((desktop, Some(Shell { conn, dir }))),
        Err(e) => {
            eprintln!("queijo: GNOME Shell screenshot failed ({e}), falling back to gnome-screenshot");
            let result = run("gnome-screenshot", &["-f"], &file).or_else(|e| {
                eprintln!("queijo: gnome-screenshot failed ({e}), trying grim");
                run("grim", &[], &file)
            });
            Ok((result.and_then(|_| load(&file))?, None))
        }
    }
}

fn load(path: &Path) -> Result<RgbaImage, String> {
    let image = image::open(path).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(path);
    Ok(image?.to_rgba8())
}

/// Asks GNOME Shell directly: no flash, no shutter sound, and ~5x faster than
/// spawning gnome-screenshot. The Shell only answers a short allowlist of bus
/// names, so we borrow gnome-screenshot's while the connection lives.
fn shell_capture(path: &Path) -> Result<(RgbaImage, zbus::blocking::Connection), String> {
    use zbus::fdo::RequestNameFlags;

    let conn = zbus::blocking::Connection::session().map_err(|e| e.to_string())?;
    let _ = conn.request_name_with_flags("org.gnome.Screenshot", RequestNameFlags::DoNotQueue.into());
    let desktop = shell_shot(&conn, "Screenshot", path, |f| (false, false, f).into())?;
    let desktop = desktop.join().map_err(|_| "decoder panicked".to_string())??;
    Ok((desktop, conn))
}

type Decoder = std::thread::JoinHandle<Result<RgbaImage, String>>;

enum Args<'a> {
    Screen(bool, bool, &'a str),
    Window(bool, bool, bool, &'a str),
}

impl<'a> From<(bool, bool, &'a str)> for Args<'a> {
    fn from((a, b, f): (bool, bool, &'a str)) -> Self {
        Args::Screen(a, b, f)
    }
}

impl<'a> From<(bool, bool, bool, &'a str)> for Args<'a> {
    fn from((a, b, c, f): (bool, bool, bool, &'a str)) -> Self {
        Args::Window(a, b, c, f)
    }
}

/// Runs one Shell screenshot call and starts decoding the result in the background.
fn shell_shot(
    conn: &zbus::blocking::Connection,
    method: &str,
    path: &Path,
    args: impl for<'a> Fn(&'a str) -> Args<'a>,
) -> Result<Decoder, String> {
    let file = path.to_str().ok_or("non UTF-8 runtime dir")?;
    let dest = (Some("org.gnome.Shell.Screenshot"), "/org/gnome/Shell/Screenshot", Some("org.gnome.Shell.Screenshot"));
    let reply = match args(file) {
        Args::Screen(a, b, f) => conn.call_method(dest.0, dest.1, dest.2, method, &(a, b, f)),
        Args::Window(a, b, c, f) => conn.call_method(dest.0, dest.1, dest.2, method, &(a, b, c, f)),
    }
    .map_err(|e| format!("{method}: {e}"))?;
    let (ok, _): (bool, String) = reply.body().deserialize().map_err(|e| e.to_string())?;
    if !ok {
        let _ = std::fs::remove_file(path);
        return Err(format!("{method}: the shell refused"));
    }
    let path = path.to_path_buf();
    Ok(std::thread::spawn(move || load(&path)))
}

fn run(program: &str, args: &[&str], path: &Path) -> Result<(), String> {
    let out = Command::new(program).args(args).arg(path).output().map_err(|e| e.to_string())?;
    if out.status.success() && path.exists() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Cuts out each monitor from the full-desktop capture.
pub fn split(desktop: &RgbaImage, monitors: &[Monitor]) -> Vec<RgbaImage> {
    let min_x = monitors.iter().map(|m| m.x).min().unwrap_or(0);
    let min_y = monitors.iter().map(|m| m.y).min().unwrap_or(0);
    let max_x = monitors.iter().map(|m| m.x + m.width).max().unwrap_or(desktop.width() as i32);
    // The screenshot is in stage pixels; with HiDPI that can differ from logical units.
    let scale = desktop.width() as f64 / (max_x - min_x).max(1) as f64;

    monitors
        .iter()
        .map(|m| {
            let px = |v: i32| (v as f64 * scale).round() as u32;
            let x = px(m.x - min_x).min(desktop.width().saturating_sub(1));
            let y = px(m.y - min_y).min(desktop.height().saturating_sub(1));
            let w = px(m.width).min(desktop.width() - x).max(1);
            let h = px(m.height).min(desktop.height() - y).max(1);
            image::imageops::crop_imm(desktop, x, y, w, h).to_image()
        })
        .collect()
}

/// Finds where `window` sits inside `desktop` and returns its rectangle in
/// logical desktop coordinates as `[x, y, width, height]`.
///
/// GNOME doesn't let other apps list window positions, but it will hand us a
/// picture of the focused window; matching a few detailed patches of it against
/// the full capture tells us where it is.
pub fn locate_window(desktop: &RgbaImage, window: &RgbaImage, monitors: &[Monitor]) -> Option<[i32; 4]> {
    let window = trim_transparent(window)?;
    let (x, y) = locate(desktop, &window)?;

    let min_x = monitors.iter().map(|m| m.x).min().unwrap_or(0);
    let min_y = monitors.iter().map(|m| m.y).min().unwrap_or(0);
    let max_x = monitors.iter().map(|m| m.x + m.width).max().unwrap_or(desktop.width() as i32);
    let scale = desktop.width() as f64 / (max_x - min_x).max(1) as f64;
    let logical = |v: i64| (v as f64 / scale).round() as i32;
    Some([logical(x) + min_x, logical(y) + min_y, logical(window.width() as i64), logical(window.height() as i64)])
}

/// Drops see-through borders (client-side shadows), which aren't part of the visible window.
fn trim_transparent(img: &RgbaImage) -> Option<RgbaImage> {
    let (w, h) = img.dimensions();
    let opaque_row = |y: u32| (0..w).filter(|&x| img.get_pixel(x, y)[3] == 255).count() as u32 > w / 2;
    let opaque_col = |x: u32| (0..h).filter(|&y| img.get_pixel(x, y)[3] == 255).count() as u32 > h / 2;
    let top = (0..h).find(|&y| opaque_row(y))?;
    let bottom = (0..h).rev().find(|&y| opaque_row(y))?;
    let left = (0..w).find(|&x| opaque_col(x))?;
    let right = (0..w).rev().find(|&x| opaque_col(x))?;
    if right <= left || bottom <= top {
        return None;
    }
    Some(image::imageops::crop_imm(img, left, top, right - left + 1, bottom - top + 1).to_image())
}

/// Offset of `needle` inside `hay` in pixels; may be negative if the window is partly off screen.
fn locate(hay: &RgbaImage, needle: &RgbaImage) -> Option<(i64, i64)> {
    const P: u32 = 12;
    let (nw, nh) = needle.dimensions();
    let (hw, hh) = hay.dimensions();
    if nw < 3 * P || nh < 3 * P || hw < P || hh < P {
        return None;
    }
    let to_u32 = |img: &RgbaImage| -> Vec<u32> {
        img.as_raw().chunks_exact(4).map(|p| u32::from_le_bytes([p[0], p[1], p[2], p[3]])).collect()
    };
    let hay_px = to_u32(hay);
    let needle_px = to_u32(needle);
    let at = |px: &[u32], stride: u32, x: u32, y: u32| px[(y * stride + x) as usize];

    // Patches with lots of detail match in few places; flat ones match everywhere.
    let mut patches = Vec::new();
    for gy in 1..8 {
        for gx in 1..8 {
            let (x, y) = ((nw - P) * gx / 8, (nh - P) * gy / 8);
            let mut detail = 0;
            let mut opaque = true;
            for dy in 0..P {
                for dx in 0..P {
                    let p = at(&needle_px, nw, x + dx, y + dy);
                    opaque &= p >> 24 == 0xff;
                    if dx > 0 && p != at(&needle_px, nw, x + dx - 1, y + dy) {
                        detail += 1;
                    }
                }
            }
            if opaque && detail > 12 {
                patches.push((detail, x, y));
            }
        }
    }
    patches.sort_by_key(|p| std::cmp::Reverse(p.0));
    patches.truncate(6);

    let mut votes: std::collections::HashMap<(i64, i64), u32> = std::collections::HashMap::new();
    for &(_, px, py) in &patches {
        let first = at(&needle_px, nw, px, py);
        for y in 0..=hh - P {
            let row = &hay_px[(y * hw) as usize..((y + 1) * hw) as usize];
            for x in 0..=hw - P {
                if row[x as usize] != first {
                    continue;
                }
                let matches = (0..P)
                    .all(|dy| (0..P).all(|dx| at(&hay_px, hw, x + dx, y + dy) == at(&needle_px, nw, px + dx, py + dy)));
                if matches {
                    *votes.entry((x as i64 - px as i64, y as i64 - py as i64)).or_default() += 1;
                }
            }
        }
    }
    let needed = if patches.len() >= 2 { 2 } else { 1 };
    votes.into_iter().filter(|(_, v)| *v >= needed).max_by_key(|(_, v)| *v).map(|(k, _)| k)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn monitor(x: i32, y: i32, width: i32, height: i32) -> Monitor {
        Monitor { x, y, width, height, ..Default::default() }
    }

    /// Opaque, detailed pixels, the same every run (a flat image can't be located).
    fn noise(width: u32, height: u32) -> RgbaImage {
        let mut state = 0x2545_f491_u32;
        RgbaImage::from_fn(width, height, |_, _| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            Rgba([(state >> 8) as u8, (state >> 16) as u8, (state >> 24) as u8, 255])
        })
    }

    fn crop(img: &RgbaImage, x: u32, y: u32, w: u32, h: u32) -> RgbaImage {
        image::imageops::crop_imm(img, x, y, w, h).to_image()
    }

    /// A window picture with a see-through margin, like the shadow GNOME includes.
    fn with_shadow(window: &RgbaImage, margin: u32) -> RgbaImage {
        let mut out =
            RgbaImage::from_pixel(window.width() + 2 * margin, window.height() + 2 * margin, Rgba([0, 0, 0, 0]));
        image::imageops::replace(&mut out, window, margin as i64, margin as i64);
        out
    }

    #[test]
    fn splits_the_desktop_per_monitor() {
        let mut desktop = RgbaImage::from_pixel(4480, 1080, Rgba([255, 0, 0, 255]));
        for x in 2560..4480 {
            for y in 0..1080 {
                desktop.put_pixel(x, y, Rgba([0, 0, 255, 255]));
            }
        }
        let parts = split(&desktop, &[monitor(0, 0, 2560, 1080), monitor(2560, 0, 1920, 1080)]);
        assert_eq!((parts[0].dimensions(), parts[1].dimensions()), ((2560, 1080), (1920, 1080)));
        assert_eq!(*parts[0].get_pixel(0, 0), Rgba([255, 0, 0, 255]));
        assert_eq!(*parts[0].get_pixel(2559, 1079), Rgba([255, 0, 0, 255]));
        assert_eq!(*parts[1].get_pixel(0, 0), Rgba([0, 0, 255, 255]));
    }

    #[test]
    fn splitting_follows_the_screen_scale() {
        // Twice the pixels per logical pixel: the crops double, still one per monitor.
        let desktop = RgbaImage::new(3840 * 2, 1080 * 2);
        let parts = split(&desktop, &[monitor(0, 0, 1920, 1080), monitor(1920, 0, 1920, 1080)]);
        assert_eq!((parts[0].dimensions(), parts[1].dimensions()), ((3840, 2160), (3840, 2160)));
    }

    #[test]
    fn splitting_handles_negative_and_stacked_layouts() {
        let desktop = noise(1920 + 1280, 1080);
        let parts = split(&desktop, &[monitor(0, 0, 1920, 1080), monitor(-1280, 0, 1280, 720)]);
        assert_eq!(parts[1].dimensions(), (1280, 720));
        assert_eq!(parts[1].get_pixel(0, 0), desktop.get_pixel(0, 0), "the leftmost monitor starts the picture");
        assert_eq!(parts[0].get_pixel(0, 0), desktop.get_pixel(1280, 0));

        let stacked = noise(1920, 2160);
        let parts = split(&stacked, &[monitor(0, 0, 1920, 1080), monitor(0, 1080, 1920, 1080)]);
        assert_eq!(parts[1].get_pixel(5, 5), stacked.get_pixel(5, 1085));
    }

    #[test]
    fn splitting_never_panics_on_odd_input() {
        assert!(split(&noise(10, 10), &[]).is_empty());
        let parts = split(&noise(100, 100), &[monitor(0, 0, 5000, 5000)]);
        assert_eq!(parts[0].dimensions(), (100, 100), "clamped to the picture");
    }

    #[test]
    fn finds_a_window_on_the_desktop() {
        let desktop = noise(1200, 800);
        let window = crop(&desktop, 300, 200, 400, 300);
        let screen = [monitor(0, 0, 1200, 800)];
        assert_eq!(locate_window(&desktop, &window, &screen), Some([300, 200, 400, 300]));
        assert_eq!(
            locate_window(&desktop, &with_shadow(&window, 24), &screen),
            Some([300, 200, 400, 300]),
            "shadow trimmed"
        );
    }

    #[test]
    fn window_positions_are_in_logical_pixels_of_the_whole_desktop() {
        let desktop = noise(1200, 800); // 2x the 600x400 logical monitor at (100, 50)
        let window = crop(&desktop, 300, 200, 400, 300);
        assert_eq!(locate_window(&desktop, &window, &[monitor(100, 50, 600, 400)]), Some([250, 150, 200, 150]));
    }

    #[test]
    fn does_not_invent_a_window() {
        let desktop = noise(1200, 800);
        let elsewhere = RgbaImage::from_fn(300, 200, |x, y| Rgba([(x * 3) as u8, (y * 5) as u8, (x ^ y) as u8, 255]));
        assert_eq!(locate_window(&desktop, &elsewhere, &[monitor(0, 0, 1200, 800)]), None);
        // Flat windows have nothing to match on; better no answer than a wrong one.
        let flat = RgbaImage::from_pixel(300, 200, Rgba([30, 30, 30, 255]));
        assert_eq!(locate_window(&desktop, &flat, &[monitor(0, 0, 1200, 800)]), None);
        assert_eq!(locate_window(&desktop, &RgbaImage::new(300, 200), &[monitor(0, 0, 1200, 800)]), None, "invisible");
    }

    #[test]
    fn trims_shadows_but_not_the_window() {
        let window = noise(80, 60);
        let trimmed = trim_transparent(&with_shadow(&window, 10)).unwrap();
        assert_eq!(trimmed, window);
        assert_eq!(trim_transparent(&window).unwrap(), window, "nothing to trim");
        assert!(trim_transparent(&RgbaImage::new(20, 20)).is_none());

        // A soft, semi-transparent shadow edge is trimmed too.
        let mut soft = with_shadow(&window, 10);
        for x in 7..10 {
            for y in 0..soft.height() {
                soft.put_pixel(x, y, Rgba([0, 0, 0, 90]));
            }
        }
        assert_eq!(trim_transparent(&soft).unwrap(), window);
    }
}
