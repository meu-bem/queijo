//! Client for the "Queijo helper" GNOME Shell extension (see `gnome-extension/`),
//! which can list and capture windows — something GNOME offers no public API for.

use std::path::PathBuf;

use image::RgbaImage;

const DEST: &str = "org.gnome.Shell";
pub(crate) const PATH: &str = "/app/queijo/Queijo";
pub(crate) const IFACE: &str = "app.queijo.Queijo";

/// A window on the active workspace, as the extension reports it.
pub struct WindowInfo {
    pub id: u64,
    pub title: String,
    pub app: String,
    /// The visible window, in desktop coordinates: [x, y, width, height].
    pub frame: [i32; 4],
    /// What `CaptureWindow` returns (frame plus client-side shadows).
    pub buffer: [i32; 4],
    pub focused: bool,
}

/// A window ready for the picker.
pub struct WindowShot {
    pub title: String,
    pub app: String,
    pub frame: [i32; 4],
    pub focused: bool,
    /// The window's own pixels (even where other windows cover it).
    pub image: Option<RgbaImage>,
    /// Small copy for the picker.
    pub thumb: Option<RgbaImage>,
}

type Row = (u64, String, String, String, i32, i32, i32, i32, i32, i32, i32, i32, bool);

/// `None` when the extension isn't running (not installed, or not loaded yet).
pub fn list_windows(conn: &zbus::blocking::Connection) -> Option<Vec<WindowInfo>> {
    let reply = conn.call_method(Some(DEST), PATH, Some(IFACE), "ListWindows", &()).ok()?;
    let (rows,): (Vec<Row>,) = reply.body().deserialize().ok()?;
    Some(
        rows.into_iter()
            .map(|(id, title, app, _class, fx, fy, fw, fh, bx, by, bw, bh, focused)| WindowInfo {
                id,
                title,
                app,
                frame: [fx, fy, fw, fh],
                buffer: [bx, by, bw, bh],
                focused,
            })
            .collect(),
    )
}

/// Captures every window (in parallel) and cuts each down to its frame.
pub fn capture_all(conn: &zbus::blocking::Connection, windows: Vec<WindowInfo>) -> Vec<WindowShot> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let jobs: Vec<_> = windows
        .into_iter()
        .map(|w| {
            let path = dir.join(format!("queijo-{}-win-{}.png", std::process::id(), w.id));
            let ok = path.to_str().is_some_and(|file| {
                conn.call_method(Some(DEST), PATH, Some(IFACE), "CaptureWindow", &(w.id, file))
                    .ok()
                    .and_then(|r| r.body().deserialize::<(bool,)>().ok())
                    .is_some_and(|(ok,)| ok)
            });
            std::thread::spawn(move || {
                let image = ok.then(|| image::open(&path).ok()).flatten().map(|i| i.to_rgba8());
                let _ = std::fs::remove_file(&path);
                let image = image.and_then(|img| crop_to_frame(&img, w.frame, w.buffer));
                let thumb = image.as_ref().map(|img| {
                    let s = (480.0 / img.width() as f32).min(300.0 / img.height() as f32).min(1.0);
                    let (tw, th) = ((img.width() as f32 * s) as u32, (img.height() as f32 * s) as u32);
                    image::imageops::thumbnail(img, tw.max(1), th.max(1))
                });
                WindowShot { title: w.title, app: w.app, frame: w.frame, focused: w.focused, image, thumb }
            })
        })
        .collect();
    jobs.into_iter().filter_map(|j| j.join().ok()).collect()
}

fn crop_to_frame(img: &RgbaImage, [fx, fy, fw, fh]: [i32; 4], [bx, by, bw, _]: [i32; 4]) -> Option<RgbaImage> {
    let s = img.width() as f32 / bw.max(1) as f32;
    let px = |v: i32| (v as f32 * s).round().max(0.0) as u32;
    let (x, y) = (px(fx - bx), px(fy - by));
    if x >= img.width() || y >= img.height() {
        return None;
    }
    let (w, h) = (px(fw).min(img.width() - x), px(fh).min(img.height() - y));
    (w > 0 && h > 0).then(|| image::imageops::crop_imm(img, x, y, w, h).to_image())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn numbered(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| Rgba([(x % 256) as u8, (y % 256) as u8, 0, 255]))
    }

    #[test]
    fn cuts_the_frame_out_of_the_buffer() {
        // The buffer starts 10 px up and left of the frame (that is the shadow).
        let cut = crop_to_frame(&numbered(120, 100), [0, 0, 100, 80], [-10, -10, 120, 100]).unwrap();
        assert_eq!(cut.dimensions(), (100, 80));
        assert_eq!(*cut.get_pixel(0, 0), Rgba([10, 10, 0, 255]));
    }

    #[test]
    fn scales_with_the_buffer() {
        let cut = crop_to_frame(&numbered(240, 200), [0, 0, 100, 80], [-10, -10, 120, 100]).unwrap();
        assert_eq!(cut.dimensions(), (200, 160), "twice the pixels per logical pixel");
        assert_eq!(*cut.get_pixel(0, 0), Rgba([20, 20, 0, 255]));
    }

    #[test]
    fn stays_inside_the_picture() {
        let cut = crop_to_frame(&numbered(100, 100), [0, 0, 500, 500], [0, 0, 100, 100]).unwrap();
        assert_eq!(cut.dimensions(), (100, 100));
        assert!(crop_to_frame(&numbered(100, 100), [500, 0, 50, 50], [0, 0, 100, 100]).is_none());
        assert!(crop_to_frame(&numbered(100, 100), [10, 10, 0, 50], [0, 0, 100, 100]).is_none(), "empty frame");
    }

    /// The program and the Shell extension agree on every D-Bus name and path; a
    /// mismatch would silently switch off the window picker and the indicator.
    #[test]
    fn the_extension_uses_the_names_the_program_does() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("gnome-extension");
        let extension = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.is_dir())
            .expect("the extension folder");
        let js = std::fs::read_to_string(extension.join("extension.js")).unwrap();
        // The one field we need, without pulling in a JSON library for a test.
        let metadata = std::fs::read_to_string(extension.join("metadata.json")).unwrap();
        let uuid = metadata.split("\"uuid\"").nth(1).and_then(|rest| rest.split('"').nth(1)).expect("a uuid");

        assert!(js.contains(&format!("const OBJECT_PATH = '{PATH}';")), "service path");
        assert!(js.contains(&format!("<interface name=\"{IFACE}\">")), "service interface");
        let recorder = crate::record::RECORDER_NAME;
        assert!(js.contains(&format!("const RECORDER_NAME = '{recorder}';")), "recorder name");
        assert!(js.contains(&format!("const RECORDER_PATH = '{}';", crate::record::RECORDER_PATH)), "recorder path");
        assert!(
            include_str!("record.rs").contains(&format!("#[zbus::interface(name = \"{recorder}\")]")),
            "the recorder object's interface is named like its bus name"
        );
        assert_eq!(uuid, extension.file_name().unwrap().to_str().unwrap(), "folder name is the uuid");
    }
}
